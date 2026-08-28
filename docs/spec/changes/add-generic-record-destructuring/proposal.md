# Proposal: 位置/属性模式支持泛型 record 解构

## Why

模式引擎补齐批（#316）后，位置模式 `Point(x,y)` / 属性模式 `Point{X:x}` 对**非泛型** record 已完整。
但泛型 record（`Box<T>(value)`、`Pair<A,B>(a,b)`）在位置/属性模式里**被 defer**：`Box<int>(x)` 虽能解析
（`PatternParser` 类型引导分支调完整 `_parseType()`，泛型实参完整带进 `PositionalPattern.Type`），但绑定期
`_bindPositional` 的类型 guard `if (!(resolved is Z42ClassType))` 判假——`Box<int>` 经 `env.ResolveType`
得到的是 `Z42InstantiatedType`（非 `Z42ClassType`）→ 直接报「positional pattern requires a record type」。

这是纯 binder 层的缺口，非语言/运行时限制。z42 的泛型类型模式 `x is Box<int> b` 本就以**擦除式**支持
（`TypeOpEmitter._emitIs` 发 `IsInstance(obj, "Box")`，运行时只测擦除基类、不 reify 实参；narrowed 变量类型带实参）。
结构化模式应镜像同一擦除模型，把这个缺口补上，让泛型 record 成为模式匹配的一等公民。

## What Changes

- **位置模式 `Box<int>(x)` / 属性模式 `Box<int>{Value:x}`**：绑定期识别 `Z42InstantiatedType`，解开 `.Def`
  走 `IsRecord`/arity 校验，字段类型经**已存在的** `MemberResolver._substGeneric(fieldType, inst)`
  （public static）做类型参数替换（`T`→`int`），子模式拿到替换后的具体类型递归绑定。
- **擦除式运行时语义**（对齐 `is`）：emit 侧 `IsInstance` 只测擦除基类 `Box`，不校验 `<int>`——与
  `TypeOpEmitter._emitIs` 一致。emit 基本无需改（`ExprEmitter._receiverClassType` 已解开
  `Z42InstantiatedType.Def`；class record 字段读 `FieldGetInstr(subj, fieldName)` 按名、类型擦除）。
- **首版限 class record 泛型**：泛型 **struct** record 继续 defer（沿用现有 struct defer 策略——
  `_guardStructSubject`/`_guardNestedStructField` 目前只吃 `Z42ClassType`，泛型 struct 布局单态化另议；
  由 `complete-struct-pattern-destructuring` / `add-tuple-types` 相关工作推进）。
- **无新语法、无 token、无 zbc/zpkg 格式 bump、无 runtime 改动**（纯 semantics binder 层补线）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | ①位置 guard(:178) 加 `Z42InstantiatedType` 分支：取 `inst.Def` 判 `IsRecord`/arity，保留 `inst` 供替换；②位置字段类型(:202) `fty = MemberResolver._substGeneric(_fieldType(inst.Def, fname), inst)`；③属性 guard(:256) + 字段类型(:258) 同款；④`CheckIrrefutable`(:43,53) 对 `Z42InstantiatedType.Name()`（`"Box<int>"`）的名比对确认拼写一致 |
| `src/compiler/z42c.semantics/src/BoundPattern.z42` | MODIFY(若需) | `BoundPositionalPattern.Type`/`BoundPropertyPattern.Type` 存 resolved（instantiated）类型；emit 侧已能解开，多半无需改，实施期坐实 |
| `src/tests/pattern-matching/pattern_generic.z42` | NEW | e2e 自检（Assert）：`Box<int>(x)`/`Pair<A,B>(a,b)` switch-stmt/expr + is + 解构声明；嵌套泛型字段 `Pair<int, Box<int>>`；jit 双验 |
| `src/compiler/z42c.semantics/tests/analyzer/analyzer_tests.z42` | MODIFY(若需) | 若加负例（泛型 struct record 仍 defer 报错）用 `SemanticDump.FirstErrorCode` 单测 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 泛型 record 解构从 Deferred 上移正文，记录擦除式语义 + `_substGeneric` 替换机制 |
| `src/compiler/z42c.semantics/README.md` | MODIFY(若需) | 功能索引更新 |

**只读引用**（理解上下文必须读，不修改）：
- `src/compiler/z42c.semantics/src/MemberResolver.z42` — `_substGeneric`(:259-276) 替换设施 + GS6 字段访问替换范例(:176-187)
- `src/compiler/z42c.semantics/src/SymbolTable.z42` — `ResolveTypeP`(:140-158) 泛型实例解析、arity-mangle `Name$N`
- `src/compiler/z42c.semantics/src/Z42Type.z42` — `Z42InstantiatedType`(:281-303)、`Z42GenericParamType`(:269)
- `src/compiler/z42c.semantics/src/TypeOpTyper.z42` / `TypeOpEmitter.z42` — is-expr 泛型擦除参照(:70/:19)
- `src/compiler/z42c.semantics/src/PatternEmitter.z42` — 字段读 emit（擦除、多半无需改）

## Out of Scope

- **泛型 struct record 解构**（需单态化 struct blob 布局；由 struct/tuple 相关 change 推进）
- **实参 reify**（运行时校验 `<int>`）——z42 泛型全程擦除，本变更保持一致，不引入 reified generics
- 任何 zbc/zpkg 格式变更（无）

## Open Questions

- [ ] `Z42InstantiatedType.Def` 的 `GenericParamNames`/`GenericParamCount` 对 record 类型是否已由
  StubCollector/SymbolCollector 回填？若未填，`_substGeneric` 退化成 Unknown → 需确认回填路径（实施首步坐实）。
- [ ] `CheckIrrefutable` 对 `Z42InstantiatedType.Name()`（含 `", "` 分隔的实参串）的逐字节名比对是否与
  解构声明 init 静态类型的 `Name()` 拼写完全一致（空格/实参序）？不一致会误报 irrefutable 失败——实施期坐实。
- [ ] 嵌套泛型字段 `Pair<int, Box<int>>` 的字段类型 `Box<int>` 经 `_substGeneric` 递归替换后再解构，是否
  自洽递归（复用同一新分支）而非特例？design 给方案。
