# Proposal: file-scoped usings（强制）+ global using

> Status: IMPL 中（2026-08-07）；分类 lang；子系统 compiler（前端 + pipeline）。
> ⚠️ 基于 #141（0.34）开发——origin/main #140 刚 bump zpkg 0.35，本地无 0.35 种子（格式 bump 窗口）；
> 本变更格式无关（只加诊断、不改 emit 字节），rebase 到 #140 后 CI 验证 0.35。

## Why

z42 现状：一个文件 `using Std.Text;`，**整个包**的所有文件都能用 `StringBuilder`——using 包级泄漏
（`allUsings` 跨文件聚合激活依赖包，见 PackageCompile）。这与 C#/Rust/Python/Go/TS **全部**的
文件级作用域相悖，是「处处隐式 global using、且无法关闭」的异类：

- **暗雷**：文件能用它从没 import 的符号（因兄弟文件 import 了）→ 删兄弟的 using 或删兄弟文件 →
  不相关文件神秘崩（spooky action at a distance）。
- **可读性/可重构性差**：读一个文件看不出它的真实依赖；搬文件会因丢兄弟 using 而崩。
- z42c 源码**其实已在遵守** file-scope 纪律（compiler-z42c.md），泄漏只是没人利用的隐患。

pre-1.0 正是修地基的窗口。终态 = **file-scope 默认 + `global using` 可选逃生舱**（C# 10 模型）。

## What Changes

**1. file-scoped usings（强制，E0436）**

每个文件用到的**跨包依赖 ns** 必须被本文件的 `using`（∪ prelude `{Std, Std.Runtime}` ∪ 本 ns）
覆盖，否则报 `E0436: namespace X is used but not imported; add using X;`。

- 实现：`IrDump.BuildPackageCus` 每个 CU 编完后 `_enforceFileScope`——比对 `cm.UsedDepNs`
  （codegen 已算好、DEPS 用的同一份跨包 ns 集）与本文件 usings。**只读 UsedDepNs + 追加诊断，
  不改任何 emit 字节** → 自举字节不动点不破。
- 破坏面：全代码库仅 **5 个 toolchain 文件** under-import（都补 `using Std.Cli;`/`Std.Test;`）——
  z42c / stdlib(24 lib) / goldens(241) 全 0，证实代码库高度自律。

**2. global using（逃生舱，现在有意义）**

`global using X;`（`global` 上下文 token，非新关键字）包级生效——注入到包内每个 CU 的 using 集，
满足其 file-scope 检查。团队 prelude / 真正处处要的 ns 一条 global using 搞定。

- 实现：`UsingDecl.IsGlobal`；Parser 顶层 `global`+`using` 识别；`IrDump._injectGlobalUsings`
  收集全包 global usings 注入每个 CU 的 Decls（既有 per-CU using 提取点 + `_enforceFileScope` 自动纳入）。

## Scope（改动文件）

| 文件 | 改动 |
|------|------|
| `z42c.core/src/DiagnosticCodes.z42` | +`MissingUsing = E0436` |
| `z42c.syntax/src/Decl.z42` | `UsingDecl` 加 `IsGlobal` |
| `z42c.syntax/src/Parser.z42` | `global`(ctx)+`using` → IsGlobal using |
| `z42c.semantics/src/IrDump.z42` | `_injectGlobalUsings`（注入）+ `_enforceFileScope`（E0436） |
| `scripts/test/xtask_test.z42`、`xtask_test_lib.z42` | 补 `using Std.Cli;` |
| `src/toolchain/builder/core/builder_hooks.z42` | 补 `using Std.Test;` |
| `src/toolchain/builder/core/builder_publish.z42`、`launcher/core/launcher_export.z42` | 补 `using Std.Cli;` |
| `docs/design/language/namespace-using.md` | file-scoped usings + global using 语法/语义节 |
| `examples/global_using/`、`src/tests/...` | 示例 + 跨文件 golden |

## Out of Scope

- **同包跨 ns** 也要求 using（严格 C# 语义）：本轮只管**跨包**（确认的泄漏）；同包跨-ns 留 follow-up。
- **类型位置完备追踪**：`UsedDepNs` 是 codegen 追踪（调用可达完备）；纯类型位（字段类型从不使用）
  的补追踪留 follow-up（当前 5 文件已覆盖全部实测 under-import）。
- z42c/stdlib 源码使用 global using：两阶段 nightly 纪律。
