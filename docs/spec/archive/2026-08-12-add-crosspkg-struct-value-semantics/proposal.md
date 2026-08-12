# Proposal: 跨包 struct 值语义（P4a）

## Why

struct 值类型的单包功能面已闭合（interp + JIT）。但 **跨 zpkg 用 struct 会崩**：包 B `import` 包 A 定义的
`struct Point` 后，`A.Point p = new A.Point(1,2)` 运行期崩
`struct field write out of blob bounds (off=0, w=4, len=0)`。

根因是**一个标志位在跨包这一跳被丢弃**——`ExportedClassZ` 无 `IsStruct` 字段，`TsigReconcile` 从
`cd.Flags&4`（`CLASS_FLAG_STRUCT`）读到了 struct 标志却没传下去，`ImportedSymbolLoader` 造 imported
`Z42ClassType` 时从不设 `IsStruct`（默认 false）→ 消费编译器把 imported struct 当**引用类型**（不发
`StructAlloc`/`StructFieldGetPrim`），而 A 的构造函数是按 struct 编译的（发 `StructFieldSetPrim off=0`）→ 两
包对「Point 是否值类型」不一致 → 写进 0 长字节区崩。

**wire 层面什么都不缺**：zpkg TYPE 段已携带 struct 标志 + 字段名/类型 + 完整字节布局（`StructSize` + 引用
位图，zbc 1.31），消费方 `ZbcReader` 也已解码进 `IrClassDesc`——只是编译器内部没把「这是 struct」传过跨包
这一跳。**修复无需格式 bump**，纯编译器内部标志传播。

> **拆分说明**：原 P4 含「+ struct 字段反射」。反射需在 Rust 里复刻编译器的布局算法（`_compute` + 类型名
> 归一）才能按字段名定位字节偏移——跨语言复刻有漂移风险，值得独立 change 正经裁决「Rust 复刻 vs 格式 bump
> 写偏移表」。故本 change 只做 P4a（跨包值语义，修崩溃），**struct 字段反射拆为 follow-up change
> `add-struct-field-reflection`（P4b）**。

## What Changes

**跨包 struct 分类修复（单点，复用既有 `HasBase` 编码，不新增 stdlib API）：**
- `ImportedSymbolLoader` 造 imported `Z42ClassType` 时 `nct.IsStruct = !cl.HasBase`（**根因修复点**，单行）
  → 消费编译器把 imported struct 正确分类，`StructLayout.BuildFromSymbols` 从字段名/类型重算布局
  （`_compute` 确定性，与生产方持久化的 `StructSize`/引用位图逐字节一致——共享同一算法）。
- **为何用 `!cl.HasBase` 而非新增 `IsStruct` 字段**（bootstrap 约束，实测）：生产方
  `ExportedTypeExtractor` 已把 struct-ness 编码进 `ExportedClassZ.HasBase`（`hasBase = !isStruct`——非-struct
  class 恒 `HasBase=true`、struct 恒 `false`），故 `!cl.HasBase` **精确等价** isStruct。若给 `ExportedClassZ`
  （z42.ir/stdlib 类型）加新 `IsStruct` 字段并在 z42c 源立即使用，则上一 nightly 种子的 z42.ir 无此字段 →
  bootstrap 编当前 z42c 源报 `E0401: no field IsStruct`（axis ② stdlib API 面越界，实测抓到）。复用既有
  `HasBase` 零越界、一个 nightly 落地。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | `nct.IsStruct = !cl.HasBase`（~:88，紧随 IsSealed）——根因修复，单行，复用既有 HasBase 编码 |
| `src/tests/cross-zpkg/struct_cross_pkg/` | NEW | 跨包 struct golden（target 定义 struct/Line，ext 定义嵌 imported struct 的 Rect transitive，main 构造/字段/方法/传参 copy-in/`q=p` 值独立/嵌套/transitive）+ expected |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 加「跨包 struct（P4a）」节 + 页头对齐 |
| `docs/roadmap.md` | MODIFY | Deferred 索引更新（P4a 已落，剩 P4b 反射 / P5-B / B-radical） |

**只读引用**（理解上下文，不修改）：
- `src/compiler/z42c.semantics/src/StructLayout.z42` — `BuildFromSymbols`/`_compute`（确认 imported struct 被重算）
- `src/compiler/z42c.semantics/src/Z42Type.z42` — `Z42ClassType.IsStruct` 定义（`SymbolCollector` 对 local 设）
- `src/compiler/z42c.semantics/src/SymbolCollector.z42:828` — local `ct.IsStruct = c.Kind=="struct"`（对照）
- `src/tests/cross-zpkg/var_field_cross_pkg/` — 跨包 golden 项目结构模板

## Out of Scope

- **struct 字段反射（P4b）** —— 拆为 follow-up change `add-struct-field-reflection`。
- **格式 bump** —— 数据全在 zpkg 0.37 / zbc 1.32，本变更零格式改动。
- **跨包「重算布局 vs wire 传布局」的后者** —— 采用重算（确定性、无 wire 变更）。
- **B-radical / P5-B** —— 后续阶梯。

## Open Questions

- [ ] 无（根因清晰，实测已验修复）。
