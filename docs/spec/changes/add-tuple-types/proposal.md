# Proposal: 元组类型 + 元组字面量 + 元组模式（值元组，零格式 bump）

## Why

z42 目前**完全没有元组**（`Z42Type.z42` 无元组节点；`LanguageFeatures.z42:62` 的 `tuples` 标志是死标志、
零消费点；`ZbcReaderInstr.z42:13` 注释直证「z42 无 tuple」）。模式匹配引擎（A1–D + #316）已完备，但缺元组
就缺了「轻量多值分组 + 元组解构」这一 Rust/Swift/C# 模式匹配的核心形态——`Deconstruct`、多返回值、
`(x,y)` 模式全都依赖它。本变更引入元组，补齐模式引擎最后一块结构化载体。

**性能定调（参考其他语言）**：性能语言的共识是元组必须是**值类型、零堆分配**——Rust（栈内联单态化）、
C#（`System.Tuple` 引用版**已弃**→ `System.ValueTuple` struct 值版）、Swift（值类型）、Go（多返回值走栈）
都如此；Scala/Kotlin 的堆 `Tuple2`/`Pair` 是公认性能疣（热循环每元组一次 GC 分配）。因此 z42 元组
**后端用 struct 值元组**（对齐 C# ValueTuple），复用 z42 已有的 struct blob 机制（`StructAlloc`/
`StructFieldGetPrim` 字节偏移、无对象头），元组解构直接骑 #316 给 struct record 模式接的那条字节偏移快路。

## What Changes（路线 A：元组降级为合成 struct 值类型；零格式 bump）

- **类型引用是字符串**：zbc/zpkg 里类型引用一律 intern 进字符串池（`ZpkgWriter.z42:340`，非封闭 tag enum）
  → 元组类型名（内部映射为合成 `Std.ValueTuple<T1,…,Tn>` struct）只是又一个字符串，**类型引用通道天然不需 bump**。
- **元组字面量 `(a, b)`**：脱糖为合成 struct record 的构造 —— `StructAlloc` + 逐字段 `StructFieldSetPrim`
  （值语义、blob 无对象头）。全用**现有 opcode**，对既有程序字节不变、golden 不动。
- **元组类型 `(int, int)`**：新 `TupleTypeExpr` 语法节点 + 新 `Z42TupleType`（或直接解析为
  `Z42InstantiatedType(ValueTuple, [int,int])`）。作参数/返回/字段类型。
- **元组模式 `(x, y)`**：新 `TuplePattern` 语法节点 + `BoundTuplePattern`；binder 独立 `_bindTuple`（按元数
  逐位递归绑定子模式，**不能复用** `_bindPositional`——后者硬要 `IsRecord`）；emit 逐位 `StructFieldGetPrim`
  字节偏移读 + 递归子模式。接入 switch/is/解构声明三位点。
- **不走路线 B**：不落实 spec 预留的原生 `tuple.new 0x93`/`tuple.get 0x94` opcode + tuple tag——那必然格式
  bump（zbc 1.36→1.37 + zpkg 0.41→0.42，9 处同步 + fixture 全重生 + 一次性 CI 红），而 z42 blob struct 本就
  无对象头、原生 opcode 边际收益很小、不值。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42c.syntax/src/TypeExpr.z42` | MODIFY | 新 `TupleTypeExpr{ Elems: TypeExpr[] }` 节点 |
| `src/libraries/z42c.syntax/src/TypeParser.z42` | MODIFY | 类型位 `(`(:90-91) 现独占函数类型 → 解析括号列表后按尾随 `->` 有无分流 tuple-type vs func-type；处理嵌套 `((int,int),int)` / `(int,int)->R` |
| `src/libraries/z42c.syntax/src/Ast.z42` | MODIFY | 新 `TupleExpr{ Elems: Expr[] }` 节点 |
| `src/libraries/z42c.syntax/src/ExprParser.z42` | MODIFY | 括号分组(:296-301) 解析首 expr 后若遇 `Comma` → 收集为 `TupleExpr`；与 lambda(`_isLambdaStart` 尾随 `=>`)/cast/分组前瞻共存（≥2 元素才当元组，`(x)` 仍分组）|
| `src/libraries/z42c.syntax/src/Pattern.z42` | MODIFY | 新 `TuplePattern{ Elems: Pattern[] }` 节点 |
| `src/libraries/z42c.syntax/src/PatternParser.z42` | MODIFY | `_parsePrimaryPattern`(:45) 加 `k == LParen` 分支解析元组模式（裸 `(` 不与 type-led 位置模式冲突；单元素 `(x)` 照 C# 视为分组/非法）|
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | 新 `Z42TupleType`（或复用 `Z42InstantiatedType` + 合成 `ValueTuple` def）；`Name()` 产 `(int, int)` |
| `src/compiler/z42c.semantics/src/BoundPattern.z42` | MODIFY | 新 `BoundTuplePattern{ Elems, ElemTypes }` |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | 新 `_bindTuple`：按元数逐位取元素类型递归绑定子模式；`_bindPattern` 派发加 `is TuplePattern` 分支 |
| `src/compiler/z42c.semantics/src/PatternEmitter.z42` | MODIFY | `BoundTuplePattern` emit：逐位 `StructFieldGetPrim` 字节偏移 + 递归子模式（形态类似 `BoundPositionalPattern` struct 字段 emit）|
| `src/compiler/z42c.semantics/src/*Typer/*Emitter` | MODIFY | `TupleExpr` 类型检查 + emit（脱糖 `StructAlloc`+`StructFieldSetPrim`）；元组类型解析/单态化 struct 布局接线 |
| `src/libraries/z42.core/src/ValueTuple.z42` | NEW(可能) | 合成 `[Record] struct ValueTuple<T1,…>`（若采「元组=命名合成 struct」路线）；或纯编译器内部合成、无 stdlib 源 |
| `src/tests/tuples/tuple_basic.z42` | NEW | e2e：元组字面量/返回/解构声明/switch/is 模式；嵌套元组 `((x,y),z)`；含引用元素 `(string,int)`；jit 双验 |
| `docs/book/src/language/tuples.md` | NEW | 元组类型/字面量/模式 + 值语义（struct blob）+ 零 bump 决策记录 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 元组模式接入三位点 |

**只读引用**（理解上下文必须读，不修改）：
- `src/libraries/z42.ir/src/BinaryFormat/ZpkgWriter.z42`(:340)/`ExportedTypes.z42` — 类型引用=字符串池证据
- `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` — struct blob opcode `StructAlloc 0xC0`/`StructFieldGetPrim 0xC2`；Tag(:69-118)
- `src/libraries/z42c.syntax/src/ExprParser.z42` — lambda/cast/分组四方前瞻(:262-301)、`_isLambdaStart`(:12)
- `src/compiler/z42c.semantics/src/PatternEmitter.z42` — struct 字段 emit 快路（#316）
- `docs/design/runtime/zbc.md`(:103,208) — spec 预留的原生 tuple 编码（**路线 B，本变更不采用**）

## Out of Scope

- **路线 B 原生元组 opcode/tag**（`tuple.new`/`tuple.get`/tuple type tag）——需格式 bump，除非出现性能刚需
- **元组作为 `Deconstruct` 返回载体**（record `Deconstruct` 方法）——后续独立 change，本变更只做元组本体
- 具名元组字段 `(x: int, y: int)`（C# named tuple elements）——首版只做位置元组，具名后议
- reified 元组实参（运行时校验元素类型）——沿用擦除/struct 单态化

## Open Questions

- [ ] **泛型 struct 布局单态化（首要风险）**：`ValueTuple<int,int>` vs `<string,int>` blob 布局不同 → 需按
  实例化生成 struct 布局。z42 现有泛型 struct record 的 blob 布局是否已单态化？若首版太重，**退化为
  class-backed 元组**（`ObjNew`+`FieldSet`）先保正确、后续优化到值类型——但目标锁定值元组。design 定首版后端。
- [ ] **类型位 `(` 双关**：`TypeParser.z42:91` 现把 `(` 独占函数类型，引入 `(int,int)` 后按 `->` 有无回填分流，
  需保 `FuncTypeExpr` 既有 golden 不破；嵌套 `(int,int)->R`（元组作参/返回）解析正确。
- [ ] **表达式位 `(a,b)` 前瞻**：插进 `ExprParser` 已堆叠的 C-cast/数组 cast/用户 cast/分组四条 `LParen` 前瞻
  且不破坏（尤其 `(Ident)` 已被 cast/分组占用；`(a,b)` ≥2 元素才安全）；lambda `(a,b)=>` 靠尾随 `=>` 区分，
  确认 `_isLambdaStart` 回退到元组正确。
- [ ] 元组模式 binder 独立 `_bindTuple`（不复用位置模式的 `IsRecord` 强制）——确认与解构声明 irrefutable
  路径自洽。
- [ ] 依赖关系：元组含 struct/元组元素时依赖 `complete-struct-pattern-destructuring` 的嵌套 struct 值副本；
  泛型元组依赖 `add-generic-record-destructuring` 的替换分支。建议本 change **排在 1、2 之后** IMPL。
