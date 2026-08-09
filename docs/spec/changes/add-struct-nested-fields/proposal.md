# Proposal: 嵌套 struct 字段（值语义，A 阶段 Deferred 第一项）

## Why

struct 值语义 A-use（#148）已让**扁平**多字段 struct（各字段=基元/引用叶子）以字节 blob 内联、
获得 C# 真值语义。但 A-use 首版**显式排除了含 struct 类型字段的 struct**（`struct Line { P a; P b; }`）——
`IsBlobStruct` 见到任一 `StructLeafKind.Struct` 字段就回落引用语义。

结果是：设计里 Decision β（嵌套递归展平）+ 3a（`line.a.x=3` 原地可变）+ P1 golden 明确要求的
**嵌套 struct 字段读写**，目前不工作。本 change 补上它——这是 A 阶段 Deferred 的第一项，也是风险最低的一项。

关键事实（决定实现形态）：
- **布局侧已就绪**：`StructLayout._compute` 早已递归展平嵌套 struct（算真实 offset/size + 把嵌套引用叶子
  按偏移平移并入父布局的引用位图）。缺的只是 **codegen 侧的字段访问**与 **`IsBlobStruct` 的准入**。
- **零现存回归**：全仓（compiler / stdlib / src/tests / examples）**没有任何 struct 以 struct 为字段**——
  本 change 纯增量，不改任何现有程序的行为。
- **无格式 bump、无运行时改动**：嵌套叶子访问 = 累积 byte offset 后发射**现有** `StructFieldGetPrim/SetPrim`
  （运行时早已接受任意 `byte_off`）；整字段复制 = 逐叶子分解为现有 Get/SetPrim。故 zbc/zpkg 格式不动、
  z42vm 不动。

## What Changes

- **准入放宽**：`StructLayout.IsBlobStruct` 去掉"含嵌套 struct 字段即拒"的循环，改为**接受**嵌套；
  仍保留 `FieldCount>=2`（单字段 wrapper 塌缩=Phase B）。新增**自引用/零大小防护**：布局计算判定为
  环（`ErrorType` 命中）或 `Size==0` 的 struct 不准入（避免 0 字节 blob 越界；见下 E0438）。
- **嵌套叶子读写（3a 原地）**：`ExprEmitter._emitMember` / `_emitAssign` 对 blob struct 字段访问改为
  **沿成员链累积 byte offset**（`line.a.x` → `off(Line,a)+off(P,x)`），对根 blob 句柄发射单条
  `StructFieldGetPrim` / `StructFieldSetPrim`。扁平单层（`a.x`）是其退化情形，行为不变。
- **整个嵌套 struct 字段复制**：`P p = line.a`（读出）/ `line.a = p`（写入）= 对子 struct 的叶子
  **逐叶子分解复制**（递归到真叶子，基元走字节 codec、引用走 ref 侧表），复用现有 Get/SetPrim，
  不引入区间复制指令、不 bump 格式。
- **自引用值 struct 防护**（诊断 E0438 留 follow-up）：值 struct 直接/间接以自身为**值**字段
  （`struct Node { Node next; }`）= 无限大小（C# `CS0523`）。本 change 由 `IsBlobStruct` 的 `Size==0`
  兜底**防崩**（环 struct 的布局是空兜底 → 不准入 → 退化引用语义，与今日行为一致、不新增崩溃面）；
  显式诊断 **E0438**（`StructValueCycle`，E0416 已被 `const` 占用故取 E0438）留作紧邻 follow-up
  （SymbolCollector 对"struct→struct 值字段"图做环检测）——与嵌套字段 codegen 正交，先落核心能力。
- **无两-nightly 顾虑**：z42c / stdlib / xtask 源码本就不用嵌套 struct，本 change 不改其源；纯 codegen
  能力扩展，无新语法/新格式，故不受 support-first 纪律约束。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/StructLayout.z42`（IsBlobStruct 放宽 + Size==0 防护）
- `src/compiler/z42c.semantics/src/ExprEmitter.z42`（嵌套链 access + 整字段复制 + owner 嵌套字段守卫）
- `src/compiler/z42c.core/src/DiagnosticCodes.z42`（E0438 号预留注释；诊断实现 follow-up）
- `src/tests/types/struct_nested.z42`（golden）
- `docs/book/` struct 机制页（嵌套小节）+ 本 change 归档

## 非目标（本 change 不做）

- struct `==` 值相等（下一 change，需新指令+格式 bump）
- struct 存 class 字段 / struct[]（P3，对象/数组存储介质）
- 单叶子 struct 塌缩（Phase B）
- JIT 值路径（P5）
