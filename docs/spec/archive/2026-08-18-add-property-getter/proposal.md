# Proposal: add-property-getter —— 命名属性的计算 getter（`{ get { ... } }`）

## Why

z42 当前**不支持命名属性的计算 getter**：`public bool IsBig { get { return this.raw > 10; } }` 会被 parser
当成 auto-property（带隐藏 backing field）、**忽略 getter body**，运行时 `x.IsBig` 读到 null backing field
报 `FieldGet: expected object, got Null`。而 **indexer**（`this[i] { get { return ...; } }`）与**方法**
（`bool Foo() { return ...; }`）的 body 都是支持的——只有命名属性的 body 缺失。

这个缺口挡住了「在属性之上计算派生值」的常见 C# 模式。直接触发场景：类可见性反射
（[[add-type-visibility-reflection]]）要对齐 C# 的 `Type.IsPublic` / `IsNestedPrivate` 等——C# 里这些是
**计算属性**（在 `Type.Attributes` flags enum 之上派生）。没有计算 getter，z42 只能退而用方法
`t.IsPublic()`（语法偏离 C#）或每个 bool 各背一个 native builtin（interop 面膨胀）。补上计算 getter 后，
可以「一个 interop 返回 enum + 6 个 bool 计算属性」完全对齐 C#。

## What Changes

- 命名属性的 getter 支持块体：`T Name { get { <stmts>; return <expr>; } }`。
- 计算属性**不合成 backing field**（`__prop_X`）——getter 是真实函数体，`x.Name` 编译成调用 `get_Name()`。
- get-only（本变更不含计算 setter；`set { ... }` 仍走 auto，不在 scope）。
- 复用既有 indexer 的「body-getter → 编译成方法 → member-access 派发 `get_X`」流水线，无新 IR op、无 VM 改动。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/Decl.z42` | MODIFY | `PropertyDecl` 加 `HasGetBody` + `GetBody`（post-construction，镜像 `ClassDecl.IsPartial`） |
| `src/compiler/z42c.syntax/src/MemberParser.z42` | MODIFY | `_parseProperty`：`get` 后遇 `{` 则 `_parseBlock()` 捕获块体 |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 新增 PropertyDecl 分支绑 getter body（indexer get 分支简化版，无索引参数） |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | PropertyDecl 分支加 `HasGetBody` 情形 → `FunctionEmitter.EmitFunction` 编译真实 get_X |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | `HasGetBody` 时不合成 `__prop_X` own-field |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | `HasGetBody` 时不合成 `__prop_X` 运行时 field |
| `src/compiler/z42c.syntax/tests/decl/decl_tests.z42` | MODIFY | `test_computed_property_getter` parser golden（计算属性解析成 get-body 而非 auto） |
| `src/tests/types/computed_property.z42` | NEW | e2e golden：计算属性运行结果正确 |
| `docs/book/src/language/member-accessors.md` | MODIFY | 属性机制页加计算 getter（auto vs 计算；lowering 为 get_X；无 backing field） |

> 注：`Decl.z42` 的 MODIFY 含 `PropertyDecl.Dump` 更新（计算 getter 显示块体，供 parser golden 断言）。

**只读引用**：
- `src/compiler/z42c.syntax/src/MemberParser.z42` `_parseIndexer`（178-208）——body-getter 解析模板
- `src/compiler/z42c.semantics/src/*` 的 IndexerDecl 分支——body-binding / emit 模板

## Out of Scope

- **计算 setter**（`set { ... }`）——本变更 get-only；setter 仍是 auto。
- **expression-bodied 属性**（`Name => expr`）——独立语法糖，另开。
- **在 z42c 自身源码 / stdlib 里使用**计算属性——受 bootstrap-seed 两阶段纪律约束，本 PR 只落 **support**，
  stdlib（`Type.z42`）的 **use** 由后续 [[add-type-visibility-reflection]] 在下一 nightly 落地。

## Open Questions

- [ ] 单-PR 可行性：若冷启动种子 z42c mis-compile Type.z42 的计算属性不影响 bootstrap（本地实测），
      则 support+use 可并成一个 PR；否则严格两阶段。**实测结论待填。**
