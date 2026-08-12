# Design: 跨包 struct 值语义（P4a）

## Architecture

```
跨包 IsStruct 传播（编译器内部，无 wire 变更）：

生产方 A 编译:                          消费方 B 编译:
 SymbolCollector.IsStruct=true            zpkg A.TYPE 段（已含 Flags bit2 + 字段名/类型 + StructSize）
   ↓ ExportedTypeExtractor                  ↓ ZbcReader（已解码 IrClassDesc，含 StructSize）
 ExportedClassZ{IsStruct=true} ──wire──►  TsigReconcile: ecz.IsStruct = (cd.Flags & 4)!=0
 （zpkg TYPE/SIGS，格式不变）               ↓ ImportedSymbolLoader
                                          Z42ClassType.IsStruct = cl.IsStruct   ← 根因修复点
                                            ↓ StructLayout.BuildFromSymbols
                                          imported struct 进 _defs → 重算布局（与 A 逐字节同）
                                            ↓ ExprEmitter/FunctionEmitter
                                          发 StructAlloc/FieldGetPrim/SetPrim（正确字节 offset）
```

`ExportedClassZ` 用于两处：生产方 `ExportedTypeExtractor` 构造 + 消费方 `TsigReconcile` 从 zpkg 重建。
**消费方路径**（TsigReconcile → ImportedSymbolLoader）是修复关键；生产方也设 `IsStruct` 保持不变量一致。

## Decisions

### Decision D1: 复用既有 `HasBase` 编码 vs 新增显式 `IsStruct` 字段（bootstrap 约束定案）

**问题：** 消费编译器如何知道 imported 类型是 struct？

**初选（显式 `IsStruct`）被 bootstrap 否决：** 最初拟给 `ExportedClassZ` 加显式 `IsStruct` 字段（镜像
`IsSealed`，更自文档化、去 `HasBase` 重载）。但 `ExportedClassZ` 在 **z42.ir（stdlib 库）**，z42c.semantics
依赖它作**跨包 API**；`ImportedSymbolLoader`/`ExportedTypeExtractor`（z42c.semantics）一旦引用新 `IsStruct`
字段，bootstrap 用**上一 nightly 种子的 z42.ir**（无此字段）编当前 z42c 源即 `E0401: no field IsStruct`
（bootstrap-seed **axis ② stdlib API 面**：z42c 源新用 stdlib API 要晚一个 nightly）。**实测抓到**——
`xtask test bootstrap` 报 boundary violation。

**决定：复用既有 `HasBase` 编码，`nct.IsStruct = !cl.HasBase`。** 生产方 `ExportedTypeExtractor` 造
`ExportedClassZ` 时 `hasBase = !isStruct`（非-struct class 恒 `HasBase=true`、struct 恒 `false`；
`TsigReconcile._rebuildClass` 重建亦 `hasBase = !isStruct`）——故 `!HasBase` **精确等价** isStruct，不是启发式
猜测而是读**同一份已编码的权威 struct-ness**。零新 stdlib API → **一个 nightly 落地、无 bootstrap 越界**。
代价：`HasBase` 语义被复用（隐式耦合于「非-struct class 恒有 base」的编码不变量）；若未来要去重载，走
两-nightly 迁移到显式 `IsStruct`（support 先行、晚一 nightly 再 use）。

### Decision D2: 消费方**重算**布局 vs wire 传布局

**问题：** 消费编译器要 imported struct 的字节布局（offset/size/位图）才能发 FieldGetPrim。

**决定：选 A（重算），无 wire 变更。** 理由：`_compute` 是纯函数（字段名+类型→布局），生产方与消费方**同一份
算法** → 重算结果与生产方持久化的 `StructSize`/位图**逐字节一致**（否则正是崩溃复现，golden 端到端守住）。
避免 `ExportedClassZ` 膨胀、避免格式面扩张。**nested imported struct 递归**：`_compute` 里判字段是否 struct
依赖该字段类型也被分类 struct；D1 对**所有** imported struct 统一设 `IsStruct`，递归自洽。

### Decision D3: 只做 P4a，struct 反射拆 follow-up（实施期发现 + User 裁决）

**问题：** 原 P4 含 struct 字段反射。实施发现：反射按字段名读 boxed struct 需在 Rust **复刻** `_compute` +
`Z42Type.Canon` + `Tag.FromName` 才能定位字节偏移——跨语言复刻有漂移风险（尤其有符号/无符号解码），与
「数据从源头正确」原则相冲突；更干净的解是把 per-field 偏移写进 TYPE 段（单一真相源）但那是格式 bump。

**决定（User 2026-08-12「先 a 再 b」）：** 本 change 只做 P4a（跨包值语义，修崩溃，零风险零 bump）；**struct
字段反射拆为独立 change `add-struct-field-reflection`（P4b）**，届时正经裁决「Rust 复刻（零 bump 有漂移风险，
加 size 校验兜底）vs 格式 bump 写 per-field 偏移表（单一真相源）」。

## Implementation Notes

- **self-host 陷阱**：改 z42c 源（编译器）→ 必须 self-host 5/5 gen1==gen2。`ExportedClassZ.IsStruct` 是
  in-memory 字段（不进 zpkg 字节序列化，zpkg 从 `IrClassDesc.Flags` 写），且 z42c/stdlib 自身**零跨包多字段
  struct** → 行为惰性 → 应逐字节不变。`IsStruct` 不入 `ExportedClassZ` 构造函数 → 无构造点签名破坏。
- **一致性守护**：消费方重算布局 == 生产方 `cd.StructSize`（ZbcReader 已解码进 IrClassDesc）——由 golden 端到
  端行为覆盖（不一致即复现 blob-bounds 崩）。
- **跨包 struct 的 JIT**：一旦消费方正确发 struct 指令，JIT 路径由 P5-A（#175 已开）覆盖；#175 未合时跨包
  struct 在 JIT 下 bail→interp（功能正确，实测已验）。本 change 不碰 JIT。

## Testing Strategy

- **跨包 golden**（`src/tests/cross-zpkg/struct_cross_pkg/`）：A 定义 `struct Point`/`struct Line`，B 构造/
  字段读写/方法（`Sum`）/传参 copy-in（`Bump`）/返回/`q=p` 值独立/嵌套 `line.a.x`——`xtask test e2e --dir
  cross-zpkg` 覆盖。**这是主门**（复现并修掉崩溃）。**实测已验**：interp+jit 输出 `1 3 5 5 42 105 5 1 4 100 3`。
- **GREEN**：`cargo build --release`（z42vm 不涉改，但 gate 要过）+ `xtask test`（不传 Z42_HOME）+ self-host
  5/5 + `xtask test e2e --dir cross-zpkg`。格式中立 → 无 fixture 重生、无两代自举、warm 全程本地可验。
- `xtask test bootstrap`（改了 z42c）确认上一 nightly 仍能编当前源（无新语法/格式 → 应过）。

## Deferred / Future Work

### add-struct-field-reflection: struct 字段反射（P4b，拆出）

- **来源**：本 change D3。
- **触发原因**：反射按字段名读/写 struct 字段值需 per-field 字节偏移，运行时 `struct_layout` 只有 size + 无名
  引用位图。
- **前置依赖**：定「Rust 复刻 `_compute`+canon（零 bump，加 `computed.size==layout.size` 校验兜底）vs 格式
  bump 写 per-field 偏移表（单一真相源）」。
- **触发条件**：User 指定做 P4b（「先 a 再 b」的 b）。
- **当前 workaround**：反射面暂只覆盖非-struct 类的 slot 字段（`Value::Object`）；struct 字段值反射不支持。
