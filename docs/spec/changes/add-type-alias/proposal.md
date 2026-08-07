# Proposal: 类型别名 `using Id = T;`

> Status: IMPL 完成（2026-08-07；批量授权 A→B→C 的 B）；分类 lang；子系统 compiler（前端 + 语义）。
> 基于 origin/main（含 #143 file-scope，zpkg 0.35；nightly 已 0.35，本地可建）。

## Why

长泛型类型（`Dictionary<string, List<Foo>>`）复用啰嗦；语义命名（`UserId` 而非裸 `int`）缺失。
C# 的 `using X = T;` 给类型起文件级别名，解决二者。承接声明与初始化简化系列。

## What Changes

```z42
using UserId = int;                       // 基本类型别名
using Row    = Dictionary<string, int>;   // 泛型目标
```

- **语法**：`using` 后 `Identifier =` → 类型别名（区别于 `using ns;` / `global using`）。目标为任意
  `TypeExpr`。
- **语义**：文件级；别名名在本文件的所有类型位置（字段/参数/返回/局部/new/is/as/…）替换为目标；
  与目标类型完全互通。
- **实现**：`UsingAliasDecl`；`SymbolTable.CurrentAliases`（per-CU 设置于符号收集 3 个类型解析
  pass + `TypeChecker.Infer`）；`SymbolTable.ResolveTypeP` 对裸名·0 实参的 NamedType 查别名递归解析
  （类型形参优先）。别名在解析处替换 → 下游 codegen/emit 无需改动、零格式 bump。

## Scope（改动文件）

| 文件 | 改动 |
|------|------|
| `z42c.syntax/src/Decl.z42` | +`UsingAliasDecl` |
| `z42c.syntax/src/Parser.z42` | `using Id =` → UsingAliasDecl |
| `z42c.semantics/src/SymbolTable.z42` | `CurrentAliases` + `BuildAliases` + ResolveTypeP 替换 |
| `z42c.semantics/src/SymbolCollector.z42` | `_passMembers/_passImpls/_passInheritFields` 设别名 |
| `z42c.semantics/src/TypeChecker.z42` | `Infer` 设别名 |
| `docs/design/language/namespace-using.md` | 类型别名节 |
| `examples/type_alias.z42`、`src/tests/basic/type_alias.z42` | 示例 + golden |

## Out of Scope

- 别名作**开放泛型**再带实参（`using L = List;` 然后 `L<int>`）：本轮别名仅裸名·0 实参替换。
- 泛型别名带自己的类型形参（`using Pair<T> = ...`）。
- z42c/stdlib 源码使用别名：两阶段 nightly 纪律。
