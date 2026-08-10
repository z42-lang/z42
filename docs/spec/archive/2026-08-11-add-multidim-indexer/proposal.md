# Proposal: 多维索引器使用侧 `obj[a, b]`

## Why

z42 已支持 C# 风格索引器**声明** `T this[int r, int c] { get; set; }`——声明侧支持任意参数
个数与任意键类型，lower 成 `get_Item(r,c)` / `set_Item(r,c,value)` 方法，跨 zpkg 导出也就绪。
但**使用侧** `ExprParser` 的后缀 `[` 分支只解析单个下标表达式后就 expect `]`，导致多维索引器
「能声明、不能调用」：`m[0,0]` 在逗号处报 `E0202: expected ']'`。

这是索引器能力对齐 C# 的唯一确切缺口。补齐后单维（现已可用）与多维索引器使用侧一致。

## What Changes

- `IndexExpr` 由单 `Index` 改为携带下标数组 `Expr[] Indices` + `int IndexCount`。
- `ExprParser` 后缀 `[` 分支循环解析逗号分隔的多个下标表达式。
- 读路由 `_bindIndex` / 写路由 `_bindAssign` 把 N 个下标全部传给 `get_Item` / `set_Item`
  （写侧在下标之后再追加 value 实参）。
- **数组**遇多下标 `a[i,j]` → 明确报错（z42 数组是单维 jagged，不支持多维数组下标）。
- 集合字面量脱糖里合成的单下标 `IndexExpr`（3 处）适配数组载荷（经小 helper）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/Ast.z42` | MODIFY | `IndexExpr` 携带 `Expr[] Indices` + `int IndexCount`，`Dump()` 输出全部下标 |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | 后缀 `[` 循环解析逗号分隔下标 |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | 读/写路由传 N 下标；数组路径取 `Indices[0]` + 多下标报错；3 处合成点经 helper 适配 |
| `src/compiler/z42c.syntax/tests/parser/parser_tests.z42` | MODIFY | 新增 `m[a,b]` 解析用例 |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | 多下标派发 `get_Item`/`set_Item` 的 IR 用例 |
| `src/tests/classes/indexer_multidim.z42` | NEW | 端到端断言测试（内联 Assert.Equal，镜像 `indexer_basic.z42` 约定） |
| `examples/indexer.z42` | NEW | 多维索引器示例 |
| `docs/book/src/language/indexers.md`（若存在则 MODIFY，否则新写并挂 SUMMARY） | MODIFY/NEW | 使用侧多维支持文档 |

**只读引用**：

- `src/compiler/z42c.semantics/src/Bound.z42` — 理解 `BoundIndex`（数组单下标）/ `BoundCall`（任意实参）
- `src/compiler/z42c.semantics/src/SymbolCollector.z42` — 确认 `get_Item`/`set_Item` 符号已就绪
- `src/compiler/z42c.semantics/src/IrGen.z42` — 确认索引器 lowering 已就绪

## Out of Scope

- **索引器重载**（一个类声明多个 `this[...]`，按键类型/元数区分）——需给 `get_Item`/`set_Item`
  引入重载解析，另立 change。
- **多维数组** `int[,]`（另一套类型系统 + IR 改动）。

## Open Questions

- 无（方案已与 User 确认）。
