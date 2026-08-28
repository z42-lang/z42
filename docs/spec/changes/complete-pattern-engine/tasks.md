# Tasks: 模式引擎补齐（属性解构声明 + is or/@ + struct record 解构）

## 0. DRAFT + 6.5
- [x] proposal 落地（`proposal.md`）
- [x] User 6.5 确认（5 决策全按推荐：① T{...} 精确类型 ② is 放开 or 带绑定 ③ 嵌套 struct 字段 defer
      ④ struct 全位点覆盖 ⑤ 三独立 e2e 文件）

## 特性 1：属性形态解构声明 `{ X: x } = p`
- [x] `StmtParser._isDeconstructDeclStart`：加 `{...}=` / `T{...}=` 两支 + `_braceBalancedThenEq`
- [x] `Parser.ParseStatement`：`{` 起始先判解构声明再落块语句
- [x] `PatternBinder._bindProperty`：省类型 `{F:p}` 用 subjType 解析字段（线程 subjType）
- [x] `PatternBinder.CheckIrrefutable`：加 `BoundPropertyPattern` 分支（带类型精确匹配 + 部分字段合法）
- [x] `PatternEmitter.EmitIrrefutable`：加属性分支（逐列字段读 + 递归）
- [x] e2e `pattern_prop_destructure.z42`（省类型 / 带类型 / 部分字段 / 嵌套位置·属性）interp+jit 双绿

## 特性 2：`is` 放开 or `|` / `@`
- [x] `ExprParser` is 分支：`_parsePrimaryPattern` → `_parsePattern` + or-链续接 + `Ident @` 前瞻
- [x] `PatternParser._continueOrChain`（从首 alt 续 or-链，供 is 复用）
- [x] binder / emitter 零改（`Bind` / `EmitMatch` 位点无关）
- [x] e2e `pattern_is_oral.z42`（多类型 or / or 带绑定 / @ 绑整体 / 嵌套 or）interp+jit 双绿
- [x] 字节不动点安全：grep 坐实源无单管道 `is` 用法

## 特性 3：struct record 位置 / 属性解构
- [x] `PatternBinder._bindPositional`：删 `IsStruct` defer；线程 subjType
- [x] `PatternEmitter._emitPatFieldRead`：blob-struct 走 `StructFieldGetPrim`（字节偏移 + TypeTag），
      class 走 `FieldGet`；positional/property 调用点 struct 时 `needTest=false`（不发 IsInstance）
- [x] defer 守卫（`E0402`）：`_guardStructSubject`（boxed struct，Name 比对）+ `_guardNestedStructField`
      （嵌套 struct-record 字段，`GetClass` 规范类型判 `IsStruct && IsRecord` 排除基元标量）
- [x] e2e `pattern_struct_record.z42`（switch / switch-expr / 属性 / 解构声明 / is / 引用字段）**jit 双绿**

## 诊断单测（durable，e2e --emit-zbc 不 gate 诊断 → 补语义单测）
- [x] `src/compiler/z42c.semantics/tests/pattern/pattern_tests.z42`：属性 irrefutable（常量/类型不符→E0402）
      + struct 嵌套/boxed defer（→E0402）+ 合法正例（→""）；经 `SemanticDump.FirstErrorCode` 断言

## GREEN + 文档 + 落地
- [x] clean-cold `xtask build compiler`（fresh nightly seed；z42c self-build 绿）
- [x] `xtask test compiler`（semantics 单测 + self-build + 自举字节不动点 gen1==gen2）
- [x] 回归：pattern_core / a2 / a3 / is / destructure / with_expr interp+jit 双绿
- [x] 文档：`docs/book/src/language/pattern-matching.md`（三特性 + Deferred 更新）
- [ ] PR + CI 全绿（自举不动点 + test-host×4 含 jit）
