# Design: Phase A —— blob 值类型（多字段 struct）IR 指令 + 帧字节存储 + zbc 格式 bump

> 状态：🟡 gate（2026-08-07 并入 B-radical 后重定位）。**本文 = 统一值类型程序的 Phase A（blob 值类型，
> 多字段 struct）**，见总架构 [design-radical.md](design-radical.md)。
>
> **与 radical 的对账**：① Phase A **不碰基元**——基元保持现有 phantom-struct 模型直到 Phase B 整体替换，
> 故本文 D-γ（基元包装 struct 交互）对 Phase A **不适用/推迟到 B**，无需在此裁决。② **A-support（2a+2b）
> 不 bump 格式、不 emit**（只加编码/解码/执行能力，z42c 不发射→无 zbc 字节变化→self-host 字节不动）；
> **A-use（2c）才 bump + emit**（codegen flip）。这比原文"2a 就 bump"更省一次格式窗口。③ D-α（per-context
> 字节 arena）、D-δ（最小 4 条指令）、指令编码 D-δ 仍有效。
>
> 原文（下）保留作 blob 部分的指令/编码/Frame/GC 细节参考；stdlib 基元 struct 段落只在 Phase B 生效。

## 目标

让**局部 struct**真正以字节 blob 内联存在 + 复制语义 + 局部嵌套 lvalue（P1 收敛面）。这一步引入新 IR
指令 + 帧字节存储 + zbc 格式 bump，是 P1 从"惰性地基"进入"改格式 + 运行时 dispatch + GC"的质变。

## ⚠️ 关键发现：stdlib 基元包装类型是 struct（影响 staging 与风险）

`grep` 实测：z42c 编译器源**零** struct 声明；但 **stdlib 有 14 个 struct**，且**基元包装类型本身是
struct**——`Std.Int64`/`UInt64`/`Int16`/`Single`/`Boolean`/`Char`/`GCHandle` 等
（`public struct Int64 : IComparable<long>, INumber<long>`）。

含义：
- **z42c 自编译不受 codegen flip 影响**（编译器源不用 struct）→ 自举侧相对安全。
- **但 stdlib 编译受影响**：翻转 struct 值语义会改变这些基元包装 struct 的编译方式，且与现有
  **primitive-value-boxing**（`Value::Boxed{class, inner}`，[[primitive-value-boxing]]）交互——
  `int`→`object` 装箱当前进 `Value::Boxed`（class=`Std.Int64`），而 `Std.Int64` 现在要变值类型。
- **风险点**：基元包装 struct 的字段（包装的裸基元）内联 + 值语义，不能破坏"基元热路径拿裸 `inner`"
  的既有性能与正确性。这是 codegen flip（2c）最需谨慎处、也是本 gate 要 User 拍板的核心。

## 阶段 2 子分期（每步独立 GREEN；支撑先行）

按 bootstrap-seed 两阶段纪律 + 降低单步风险，把 阶段 2 拆成三个可验证增量：

| 子期 | 内容 | 格式/自举影响 | 验证 |
|------|------|--------------|------|
| **2a 格式 + 指令支持（不改行为）** | 新 opcode + IrInstr 类 + zbc 编码/两个解码器 + TYPE section struct 布局块（size+引用位图）+ 版本 bump；**z42c 不发射新指令**（codegen 不变） | zbc 1.29→1.30 / zpkg 0.34→0.35；self-host 仍 gen1==gen2（内容同、仅格式版本变）；CI 两代自举自愈 | fixture 重生 + golden hex 重截 + `cargo test zbc_compat/lazy_loader` + self-host 5/5 |
| **2b 运行时执行机制（惰性）** | per-context 字节 arena + `exec_struct.rs` handlers + GC 按引用位图扫字节区；**仍不被 codegen 发射** | 无格式变（2a 已 bump）；运行时纯新增 | Rust 单测（手构字节码/直接调 handler）+ 手写 .z42 用新指令的 golden |
| **2c codegen flip（USE，值语义生效）** | z42c ExprEmitter 对**局部 struct** 发射字节区分配 + StructCopy（赋值/传参/返回）+ 字节 offset 字段访问 + 局部嵌套 lvalue；IrGen 调 `BuildFromSymbols` 消费布局 | **必须晚于 2a/2b 发布的 nightly**（support-first）；self-host 输出改变（正常，D7 一代重建自愈）；stdlib 基元 struct 编译改变 | 值语义 golden 全场景 + stdlib 全绿 + 自举字节不动点重建 |

> **两阶段 nightly 落点**：2a（格式+解码支持）必须先随一个 nightly 发布，**之后**才做 2c（z42c/stdlib
> 源产出/使用新指令）。否则上一版 z42c 读不了新格式 → 自举断链。2a 与 2c **不得同一 nightly**。

## Decisions

### ⚠️ D-α：帧字节存储 = per-context 字节 arena（推荐）

**问题**：局部 struct blob 存哪？

- **选项 1（否决）per-frame `Vec<u8>`**：struct 按值传进子帧 ctor/方法时，per-frame 索引在子帧里失效
  （与 escape-analysis 遇到的"ctor 在子帧跑、this 以 Value 传"同款问题，[stack_alloc.rs:11-16]）。
- **选项 2（推荐）per-context 字节 arena**：仿 `StackArena`（[stack_alloc.rs:51]）建 `StructArena`
  （`Vec<u8>` 池 + 每 live struct 记 `{offset, len, TypeDesc, frame_id}`），`alloc/with/truncate/
  scan_roots` + frame_id staleness guard，pop_frame 时 LIFO truncate。struct 句柄
  `{arena_offset, frame_id, type_id}` 任何帧可解 → 传参进子帧不失效。

**推荐选项 2**：直接复用 escape-analysis 的 arena 模式（已验证），把 `scan_roots` 改成按
`TypeDesc.ref_bitmap` 扫字节区引用叶子。

### ⚠️ D-β：值 struct 的 `Value` 表示

未装箱 struct 不作为单个 `Value` 流转（走字节区间），但操作数怎么在 IR/寄存器层引用它？

- **方案**：struct 局部/临时 = 一个"struct 句柄"寄存器值 `Value::StructRef{arena_offset, frame_id,
  type_id}`（新 `Value` 变体，仿 `StackObject`），指向字节 arena 里的 blob。IR 指令 StructCopy/
  StructFieldGet/SetPrim 以该句柄 + byte_offset + size 操作字节区间。
- 字段基元读写：`StructFieldGetPrim`（blob 字节 → Value 寄存器）/`StructFieldSetPrim`（Value → blob 字节）
  做字节⇄Value 编解码。
- 装箱：`struct→object` 把 blob 拷进堆 `ScriptObject`（或复用 `Value::Boxed`？见 D-γ）。

### ⚠️ D-γ：与 primitive-value-boxing 的交互（核心风险，需 User 裁决）

基元包装 struct（`Std.Int64` 等）+ 现有 `Value::Boxed{class, inner}` 装箱 + 新值语义如何共存：

- **选项 A**：值语义**只作用于用户 struct**，基元包装 struct（`Std.*` 数值/Char/Boolean）保持现有
  boxing 特殊路径不变（它们本就是"基元的类型标签载体"，极少作值实例）。最小风险、最快落地；代价是
  "值语义不完全统一"（基元 struct 是特例）。
- **选项 B**：基元包装 struct 也走统一值语义（单字段 blob = 裸基元字节），boxing 改为"blob→堆"。
  统一优雅，但要同时改 boxing/unboxing + 热路径基元取值，风险最高。
- **推荐 A**（v1）：基元包装 struct 维持现状特例，值语义先覆盖用户 struct；统一留后续。这与"P1 收敛面=
  纯局部用户 struct"一致，也避免动基元热路径。

### D-δ：指令编码（对照模板）

新 opcode 取连续空档（如 `0xC0+`）。指令头恒 `op:u8 + type_tag:u8 + dst:u16`。初拟指令集：

| 指令 | 操作数编码 | 语义 |
|------|-----------|------|
| `StructAlloc dst, type_id(u32 pool), size(u32)` | | 在字节 arena 分配 size 字节的 blob，零初始化，dst=StructRef 句柄 |
| `StructCopy dst, src, size(u32)` | 两 StructRef | 复制 blob：纯基元→memcpy；含引用叶子→按位图逐叶子 Arc::clone/GcRef+屏障 |
| `StructFieldGetPrim dst_reg, base(StructRef), byte_off(u32), kind(u8)` | | blob 基元叶子→Value 寄存器 |
| `StructFieldSetPrim base, byte_off(u32), kind(u8), src_reg` | | Value→blob 基元叶子（原地，lvalue） |
| `StructFieldGet dst(StructRef), base, byte_off(u32), size(u32)` | | 取 struct 子字段区间（复制出）|
| `StructFieldSet base, byte_off(u32), size(u32), src(StructRef)` | | 存 struct 子字段区间（复制入/原地）|

- 每条要点：ZbcInstr 编码分支 + **InternStrings**（若带类型名/字段名字符串）+ **_regtInstr** reg-walk +
  两个解码器（Rust `zbc_reader.rs` + z42 `ZbcReaderInstr.z42`）+ exec_instr 穷尽 match 分支。

### D-ε：TYPE section struct 布局块（gated）

仿 enum/iface 的 gated 追加块：class record 尾部、`(Flags & CLASS_FLAG_STRUCT)!=0` 时追加
`struct_size:u32 + refLeafCount:u16 + (byte_offset:u32, kind:u8)×n`（带种类引用位图）。非 struct 类零
字节、旧布局不变。Rust `read_type` + `ClassDesc`/`TypeDesc` 加对称字段（size + ref bitmap），供 GC
arena 扫描 + StructCopy 分流。

## 改动清单（2a 首增量，对照 escape-analysis 模板）

| 层 | 文件 | 2a 改点 |
|----|------|---------|
| 版本 | `ZbcFormat.z42:8` / `zbc_reader.rs:110` | zbc Minor 29→30 + changelog |
| | `ZpkgWriter.z42` / `zbc_reader.rs:194` | zpkg Minor 34→35 |
| IR | `IrInstr.z42` | 6 条新 struct 指令类 |
| | `ZbcFormat.z42:19` | 新 opcode 常量 |
| 编码 | `ZbcInstr.z42`（WriteInstr + InternStrings） | 编码 + intern 分支 |
| | `ZbcWriter.z42`（BuildType + _regtInstr） | TYPE struct 布局块 + REGT reg-walk |
| z42 解码 | `ZbcReaderInstr.z42` | DecodeOne 分支 |
| Rust 解码 | `bytecode.rs`（Instruction enum + opcode const + ClassDesc） | 变体 + payload + TYPE struct 块 |
| | `zbc_reader.rs`（decode_instr + read_type） | 指令 + TYPE 块解码 |
| | `types.rs`（TypeDesc/FieldSlot） | struct size + ref bitmap 字段 |
| exec | `exec_instr.rs:63` | 穷尽 match 加分支（2a 可先 `bail!("not yet")` 占位，2b 实现）|
| fixture | `src/tests/zbc-format/*` + `zbc_tests.z42` hex + docs zbc/zpkg changelog | 重生 + 重截 |

2b/2c 的 Frame arena / exec_struct / GC / codegen 改点见各子期展开。

## Testing Strategy

- 2a：fixture 逐字节重生 + golden hex + `cargo test zbc_compat/lazy_loader` + self-host 5/5（内容不变）。
- 2b：Rust 单测手构字节码跑 struct 指令；手写 .z42（内联 zbc）golden。
- 2c：值语义 golden 全场景（spec [P1]）+ stdlib 全绿 + 自举字节不动点重建（D7 一代自愈）。
- **本地 warm 用主树种子会有 vscode-syntax/zbc-format 环境假阴性**（见 [[add-const-keyword]]/记忆），
  真信号看 compiler + e2e + cargo test；格式 bump 的完整验证以 CI 两代自举为准。

## Open Questions（本 gate 待 User 确认）

- [ ] **D-α**：帧字节存储走 per-context 字节 arena（推荐）？
- [ ] **D-γ**：v1 基元包装 struct 维持 boxing 特例（选项 A，推荐）vs 统一值语义（选项 B）？
- [ ] **D-δ**：指令集 6 条是否合理，还是先做最小子集（StructAlloc/StructCopy/FieldGet/SetPrim 四条，
      子字段区间 get/set 随嵌套场景再加）？
- [ ] **子分期 2a→2b→2c** 是否接受（support-first，2a 先随 nightly 发布再做 2c）？
