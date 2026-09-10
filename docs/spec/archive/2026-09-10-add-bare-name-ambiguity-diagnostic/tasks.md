# Tasks: add-bare-name-ambiguity-diagnostic

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：lang（新诊断 E0456）

**变更说明：** 非限定短名同时匹配**多个可见命名空间**里的类型时，不再静默择一，报 **E0456**
（对标 C# CS0104）。

```z42
// a.z42: namespace A; public static class Helper { ... }
// b.z42: namespace B; public static class Helper { ... }
// main.z42:
namespace R; using A; using B;
int f() { return Helper.Who(); }
//               ^^^^^^ E0456: `Helper` is an ambiguous reference between `A.Helper` and `B.Helper`
```

## 为什么需要

`SymbolTable.Classes` 按**裸名**键（本地 last-wins / 导入 first-wins），同短名跨 ns 时只活一份，
选谁取决于加载顺序。这正是 `restore-emit-zbc-diagnostics` 冷扫掀出的 216 条 E0401 的形状：
`Assert.Greater(...)` 的 `Assert` 被 first-wins 定到 `Std.Assert`，z42.test 那份的方法全报
「no static method」。#532 合并两份 Assert 消掉了**起因**，但**择一本身仍是静默的**。

## 根因：歧义信息在三个生产点都被塌掉了

| 表 | 形态 | 为什么判不了歧义 |
|---|---|---|
| `SymbolTable.Classes` | 裸名 → 单个 `Z42ClassType` | 单赢家 |
| `SymbolTable.ClassesByFqn` | `ns.Name` → 类型 | **只登记本地类**，imported 从不进 |
| `ImportedSymbols.ClassNamespaces` | 裸名 → **单个** ns | 被 first-wins 守卫盖住 |

⇒ 不是「读一下 FQN 表就能报歧义」，得先**把这个信息造出来**。

## 落地

- **新表 `SymbolTable.ClassNsAll`**：裸名 → 全部声明 ns（`"|"` 连接）。累积逻辑
  `SymbolTable.NoteNsInto` **一份共用**（两份各自演化正是这类 bug 的温床）。
- **两个生产点都在 first-wins 守卫之外累积**：`StubCollector`（本地，ns 取 `cu.Namespace`）、
  `ImportedSymbolLoader`（跨包，ns 取 `em.Namespace`；`active[i]` 已把模块限制在本 CU 可见的那些）。
  `SymbolCollector._mergeImports` 逐 ns 并进符号表（不是整表覆盖 —— 本地也可能声明同一短名）。
- **判定 `TypeChecker.ChkAmbiguousBareName`**（对齐 C#）：候选 ns → 只算本 CU 可见的
  （using 集 ∪ 本 ns ∪ **全局 ns**）→ **当前 ns 的那份直接胜出**（近者优先）→ 剩余 ≥ 2 才报。
  🔴 全局 ns（`""`）必须手动当作可见：`_isVisibleNs("")` 恒 false（它服务的是 ns **前缀**匹配）。
- **接线 11 处**：静态调用 `Foo.Bar()`（`MemberResolver` 裸类名分支）、`new Foo()`（`ConstructTyper`）、
  以及 `ChkAmbiguousBareNameT`（cast / `as` / `typeof` / `is` / catch / 局部变量声明 / 4 处模式）。
  **限定名永不歧义** —— 判定要求源 `TypeExpr` 是不含点的 `NamedType`，resolved 的 `Z42Type` 里
  没有这个信息。刻意**不**把它塞进 `_chkTypeRef`：那要改 10 个调用点的 span 实参，会连带改动
  既有 access-control 诊断的 span（golden 跟着漂）。

## 🔴 fixture 必须是多 CU —— 我为此白跑了一轮

`StubCollector` 登记类时用的 ns 是 **`cu.Namespace`（整个 CU 一份）**。一个文件里写多个
`namespace X;` 会把全部声明塌进 CU 的那一个 ns ⇒ 单文件 fixture 造不出跨 ns 同短名，
只会变成「同一 ns 内两个同名类」。我最初就是用单文件 fixture 验的，E0456 不响，一度以为实现坏了。

## 附带挖出两个静默 bug（均未修，已登记）

| # | 现象 | C# | Deferred |
|---|---|---|---|
| ① | 同一 ns 内两个同名类 → **静默 last-wins**（实测打印 `second`，零诊断） | CS0101 | `dup-type-name-in-namespace` |
| ② | 一个文件多个 `namespace X;` → 全塌进 `cu.Namespace`，**连限定名都解析错**（实测 `A.Helper.Who()` 打印 `"B"`） | 单文件多 file-scoped ns 即编译错误 | `multiple-file-scoped-namespaces` |

## 验证

🔴 全仓 **914 个类型声明、跨 ns 同短名 = 0** ⇒ 这条诊断在真实代码上永远不响
⇒ **单测就是它唯一的门**，必须自带 fixture。

`src/compiler/z42c.semantics/tests/typecheck/bare_name_ambiguity/`（6 条，走
`IrDump.BuildPackage` 多 CU 路径）：

| 用例 | 期望 | 守什么 |
|---|---|---|
| `bare_name_across_two_usings_reports_E0456` | 1 | 真门（差分） |
| `ambiguous_type_annotation_reports_E0456` | 1 | 类型注解位接线 |
| `ambiguous_new_expression_reports_E0456` | 1 | `new` 位接线 |
| `qualified_name_is_never_ambiguous` | 0 | 限定名不误报 |
| `current_namespace_wins_over_usings` | 0 | C# 近者优先 |
| `single_visible_declaration_reports_nothing` | 0 | 不把不可见的算进来 |

⚠️ 辅助函数一律带 `amb` 前缀：同一测试单元内所有文件的自由函数**共享一个平坦命名空间**，
且 z42 自由函数**不按参数类型重载** —— 我最初叫 `countCode`，被同单元另一个文件里同名同 arity
的那个顶掉（报 `cannot assign String[] to DiagnosticBag`）。
