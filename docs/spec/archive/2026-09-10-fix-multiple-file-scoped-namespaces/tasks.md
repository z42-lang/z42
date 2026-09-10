# Tasks: fix-multiple-file-scoped-namespaces

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：lang（新诊断 E0457）

**变更说明：** 一个文件里写多个文件级 `namespace X;`（或把 `namespace` 写在类型声明之后）此前
**静默 last-wins**，全部声明被登记进最后那个 ns —— **连限定名都随之解析错**。现在报 **E0457**。

```z42
namespace First;
public static class Probe { public static string Who() { return "in-First"; } }
namespace Last;
void Main() { Console.WriteLine(Probe.Who()); }
```

改动前：**rc=0、零诊断**，模块名 `Last`，`Probe` 被登记进 `Last`（`First.Main` 找不到、`Last.Main` 能跑）。
更早的实测里，`A.Helper.Who()` 这种**限定**调用打印的是 `B` 的实现 —— 这不是「少报一条诊断」，
是**解析到错误的类型**。

## 根因

z42 的编译单元只有**一个**命名空间（`CompilationUnit.Namespace` 是单值，`StubCollector` 登记类时
用的就是 `cu.HasNamespace ? cu.Namespace : ""`）。而 `Parser.ParseCompilationUnit` 的 `namespace`
分支位于**顶层声明循环内**：

```z42
if (k == TokenKind.Namespace) {
    this._advance(); ns = this._declP._parseQualifiedName(); hasNs = true; this._expectSemi();
    continue;   // ← 每命中一次就整个覆盖 ns，且不报任何错
}
```

## 修复

拒绝编译器表示不了的写法（对齐 C# CS8907 / CS8955），两条规则一个码：

| 情况 | 消息 |
|---|---|
| 第二个及之后的 `namespace` | `a file may declare only one namespace (\`X\` was already declared above)…` |
| `namespace` 排在类型/函数声明之后 | `` `namespace` must appear before any type or function declaration in the file `` |

- `using` / `global using` / `using alias` **不算**声明（新增 `sawTypeDecl` 只在类型/函数声明追加时置真），
  所以 `using Std; namespace N;` 仍合法。
- 恢复策略：保留**第一个** ns（读者对文件顶部那一行的直觉），不被后来者覆盖。**已用单测钉死**。
- 块形式 `namespace X { }` 本就不支持（`_expectSemi` 报 parse 错），不在本 change 范围。
- **语法层用字面量 `"E0457"` 发码**（同 E0449–E0456 手法，避 core→syntax 新跨成员符号撞 F2
  冷启动 stale-cache）；常量仍登记进 `DiagnosticCodes.z42` 作文档。

## 🔴 差点报出去的假警报（配方教训）

调查时我从兄弟 worktree `../z42-b3name` 拷 artifacts 供种，据此得出两条"发现"：
「**parse 错误在 `--emit-zbc` 上全部漏网**」与「**块形式 `namespace X { }` 能编过**」。
两条**都是假的** —— 那棵树的 git HEAD 虽在 `526acb72`，产物却是合并 #550（开门）**之前**建的。
用本树自建的编译器重验：parse 错误正常报（rc=1、无产物），块形式是 parse 错。

⭐ **配方**：换种子 / 用兄弟树产物后，**第一件事是跑门校准**（一个必错的源 + 一个必对的源），
证明 harness 有能力报错，再判任何"零诊断"的观测值。这条在 [[verify-conclusion-after-reseeding]]
里写过，我这次还是先踩了才想起来。
（附带：`rc=$?` 跟在管道后面取到的是**管道最后一条命令**的退出码，我为此误读了两次。）

## 验证

🔴 全仓实测：多 `namespace` 的文件 **0 个**、`namespace` 排在声明之后的文件 **0 个**
⇒ 这条诊断在真实代码上永远不响，**单测就是它唯一的门**。

`src/libraries/z42c.syntax/tests/stmt.z42` 新增 5 条（含退回对照）：

| 用例 | 期望 | 撤掉检查后 |
|---|---|---|
| `second_file_scoped_namespace_is_reported` | 1 | **0**（真门） |
| `namespace_after_type_declaration_is_reported` | 1 | **0**（真门） |
| `duplicate_namespace_keeps_the_first` | `"First"` | **`"Last"`**（恢复策略真门） |
| `namespace_after_usings_is_fine` | 0 | 0（守不误报） |
| `single_namespace_at_top_reports_nothing` | 0 | 0（守不误报） |

- [x] 端到端：两种写法各报 1 条 E0457、非零退出、**不写产物**；正常写法 rc=0 有产物
- [x] `xtask test` 全绿
