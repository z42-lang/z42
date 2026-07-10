# Design: 方法修饰符元数据（unify-type-metadata P1-c）

## Architecture

```
z42c 声明修饰符(Mods 串)                    Rust 运行期
  MethodDecl.Mods "virtual"/"abstract"        FuncSig.method_flags ─┐
     │ IrGen._methodFlags(mods)                                     ├→ Function.method_flags
     ▼                                                              │   （reader 灌入，同 is_static/visibility）
  IrFunction.MethodFlags:int  ──ZbcWriter──►  SIGS +method_flags:u8 ┘
     （bit0 virtual, bit1 abstract）              │ read_sigs
                                                  ▼
  ZbcReader/ZpkgReader ◄── 读侧对称消费     build_method_info → MethodInfo.IsVirtual/IsAbstract
```

SIGS 每函数条目布局（P1-c 后）：
`name / param_count / ret_tag / ret_idx / exec / is_static / visibility / **method_flags** / param_types... / tp块 / attr块`

## Decisions

### Decision 1: method_flags 位分配
**决定**：`bit0 = virtual`，`bit1 = abstract`。`static` **不**入 method_flags（既有独立
`is_static` 字节已表达，不重复——避免两处真相）。位留高 6 位给未来（sealed-override/new 等，若需要）。

### Decision 2: `IsVirtual` 语义 = virtual ∪ override ∪ abstract（镜像 C#）
**问题**：`virtual` 声明、`override` 覆盖、`abstract` 抽象——哪些置 virtual 位？
**决定**：三者皆置 bit0（C# `MethodInfo.IsVirtual` 对 virtual/override/abstract 均为 true）。
`_methodFlags(mods)`：`_hasWord(mods,"virtual") || _hasWord(mods,"override") || _hasWord(mods,"abstract")`
→ bit0；`_hasWord(mods,"abstract")` → bit1。
**理由**：override/abstract 方法在 vtable 中确实是虚派发的；与 C# 反射语义一致，最小惊讶。

### Decision 3: `IsVirtual` 从 vtable 启发式 → 声明 flag（行为收敛）
**现状**：`build_method_info(is_virtual)` 的 `is_virtual` 来自「该方法从 vtable 迭代得来 vs 从
own_methods 得来」。**决定**：改为读 `Function.method_flags` bit0。
**影响面**：vtable 成员基本 = virtual/override 方法，故绝大多数一致；差异在于「碰巧进 vtable 但
非 virtual 声明」的边界（若有）现在如实反映声明。`is_virtual` 参数保留用于回退（flag 缺失的
合成函数），但优先取 flag。用现有 reflection.z42 [Test] + 新增用例守住。

### Decision 4b: abstract 方法 signature-only emission（User 裁决扩 scope）
**问题**：实例 `abstract` 方法无 body → IrGen（`md.HasBody` 门）完全不 emit → 无 SIGS/FUNC 条目 →
反射看不到该方法 → `IsAbstract` 永远观测不到 true（元数据里根本没有 abstract 方法）。
**决定**：IrGen 新增 `_emitAbstractStub`——为实例 abstract 方法发 signature-only 死体桩进 SIGS/FUNC。
- **桩体**：`ret null`（非 void，reg 类型 Ref 占位，永不执行故无碍）/ 裸 `ret`（void）。abstract 方法
  只被 override 经 vtable 派发，桩本身（在抽象类的 slot）是死代码。
- **SIGS/FUNC 按 index 配对**（VM `read_zbc` 组装以 FUNC body 序为主键 + `sigs.get(i)`）→ 不能只发
  SIGS，必须发 FUNC body → 故用死体桩而非纯签名条目。
- **限实例 abstract**（`abstract ∧ ¬static`）：`static abstract` 接口成员（`INumber` 泛型静态抽象）
  不动——那是独立的静态抽象接口特性，含泛型 default-body 等更多边界，不在本砖。
- **零 byte 漂移**：z42c / stdlib 源**无实例 abstract 方法**（仅 INumber 静态抽象，被排除）→ 现有
  zpkg 字节不变 → 自举不动点（gen1==gen2）不受影响、无需额外 gen；纯 codegen、无格式二次 bump。
**为何非 Deferred**：User 裁决要 IsAbstract 真观测（见阶段 6.5 讨论），而非仅接通字段。
**实施踩坑（根因）**：`MethodDecl.RetType` 是 **`TypeExpr`（NamedType 对象），非 string**——
真方法经 `EmitFunction._sigTypeName` 解析为类型名串；stub 初把 `md.RetType` 原样塞进
string 类型的 `IrFunction.RetType` → NamedType 对象被当串 intern 进 STRS 池
（`"Z42.Syntax.NamedType{...}"`）→ 池污染 → `BuildStrs._splitDots` 遇 null 崩。修：
`retName = this._symbols.ResolveType(md.RetType).Name()`。**诊断难点**：z42 的
`null.Length`/`x == null`/`is string` 对 Null 值均不可靠（原始崩溃即 null-比较类型不匹配）→
靠 `Console.WriteLine` 逐条 dump STRS 池、肉眼逮出 `NamedType{...}` 异常条目定位。

### Decision 4: 非 gated + 两代自举（同 P1-b，纪律已固化）
每函数固定 +1 字节 → 旧 reader 读新数据/新 reader 读旧数据都会错位 → **writer + 全部 4 个
reader（Rust zbc_reader + z42c ZbcReader + z42c ZpkgReader.ReadModuleSigs）必须同提交对称改**。
zbc 1.23→1.24 / zpkg 0.27→0.28；两代自举 0.27→0.28，gen1-stdlib 用 **EMPTY Z42_LIBS**
（新 reader 只碰 0.28 兄弟）。此纪律已由 P1-b 踩坑固化，见
[archive/2026-07-10-add-member-visibility](../../archive/2026-07-10-add-member-visibility/tasks.md)。

## Implementation Notes

- `IrFunction.MethodFlags:int`（默认 0，字段式，构造后设——同 `Visibility` 范式）。
- IrGen 三处（与 `Visibility` 同址）：显式方法（~170）、impl 方法（~290）、free/其余（~353）；
  free 函数天然 0（无 virtual）。getter/setter/ctor/extern 桩默认 0。
- ZbcWriter WriteSigEntries：`w.WriteU8(f.MethodFlags)` 紧接 visibility 之后。
- Rust：`FuncSig.method_flags:u8` + `Function.method_flags:u8`（`#[serde(default)]`）；
  read_sigs 在 visibility 后读；Function 构造两处（zbc_reader 1384/1812 附近）灌
  `sig.map(|s| s.method_flags).unwrap_or(0)`。
- `resolve_func_sig` 返回元组追加 method_flags；`build_method_info`：
  `IsVirtual = (mf & 1)!=0 || is_virtual`（flag 优先，回退 vtable），`IsAbstract = (mf & 2)!=0`。
- METHOD_FLAG_VIRTUAL=1<<0 / METHOD_FLAG_ABSTRACT=1<<1（bytecode.rs 常量）。

## Testing Strategy

- 单元：z42c golden hex（empty/f5 再 +1 字节，selfcheck header minor 24）；Rust read 往返 + pinned 24/28。
- Golden/反射：`reflection.z42` 加 class（virtual/override/abstract/普通 方法）→ 断言
  `IsVirtual`/`IsAbstract`（含 base virtual + derived override + abstract 基类）。
- VM 验证：xtask test 全 stage + 自举不动点 7/7 + cargo lib + zbc_compat + lazy_loader（zpkg-format regen）。
