# Proposal: REPL MVP 元指令补齐（.reset / .clear / .using / .types / .version）

## Why

`docs/design/toolchain/repl.md` 声称 MVP 首发 13 个元指令，但 `interactive_main.z42` 实际只落地 5 个
（`.exit` `.quit` `.help` `.vars` `.usings`）——**文档超前于实现**，且与同文件 follow-up 段「`.reset`/
`.save`/`.history` 未接」自相矛盾。本 change 补齐其中**零上游依赖**的一批，让代码追上 MVP 标注、消除文档冲突。

选取判据：只补「纯宿主 or 仅需 scripting 库小改」的指令，**不引入新机制、不碰 runtime/格式**：

- `.reset` `.clear` `.using <ns>` — 纯宿主循环
- `.types` — 宿主 + scripting 库记录声明种类（类型 vs 自由函数）
- `.version` — 打印 **zbc/zpkg 格式版本**（编译器常量 `ZbcVersion` / `ZpkgWriterZ`，宿主经 scripting 库已可达）

**明确不做**（需上游基建，留 follow-up）：`.history` / `.save`（需宿主 transcript 存储）、`.mode`
（需 `ExecMode` 接口）、`.version` 的 **z42vm 运行时版本串**（需新 runtime builtin 暴露
`CARGO_PKG_VERSION`）。

## What Changes

- **`.reset`**：清空变量 / using / 声明，回到空白会话（`s = Script.Create()`）。**关键正确性**：REPL 每轮
  字节码按 `Repl.R{N}` 命名空间加载进**进程级 VM**；`Script.Create()` 会把 `Counter` 归零，下一轮又发
  `Repl.R1` → 与 VM 中已加载的旧 `R1` 撞名。故 reset 后**承接旧 `Counter` 继续递增**，保证命名空间单调、
  不撞名（旧模块成孤儿留在进程内，仅内存，无害）。
- **`.clear`**：ANSI 清屏 + 光标归位。z42 词法器不支持 `\x1b`/`\u` 转义 → 用 `(char)27` 构造 ESC。非终端
  （`!Console.IsTerminal()`）跳过，避免污染管道输出。
- **`.using <ns>`**：复用 `Script.Eval` 既有的 `using X;` 累积路径（含去重），宿主只做参数提取。
- **`.types`**：列会话内声明的**类型**（区别于 `.vars` 列变量）。`Classifier` 在 classify 时已区分类型声明
  （class/struct/enum/interface/record）与自由函数，但 `ParsedInput`/`ScriptState` 只存合并的
  `DeclNames` → 加 `IsTypeDecl` 标志 + `DeclTypeNames` 子集列表。
- **`.version`**：`Script.FormatVersion()` 返回 `"zbc <maj>.<min>, zpkg <maj>.<min>"`（strict-pin 值）。
- **`.help`** 文案同步补新指令。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/scripting/src/Classifier.z42` | MODIFY | `ParsedInput` 加 `IsTypeDecl`；类型声明分支置真 |
| `src/toolchain/scripting/src/ScriptState.z42` | MODIFY | 加 `DeclTypeNames`（类型名子集，供 `.types`）|
| `src/toolchain/scripting/src/Script.z42` | MODIFY | `_evalDecl` 类型声明时并记 `DeclTypeNames`；新增 `FormatVersion()`（+ `using Z42.IR.BinaryFormat`）|
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | 派发新增 `.reset`/`.clear`/`.using`/`.types`/`.version`；`_help` 同步；新增 `_types`/`_addUsing`/`_clear` 助手 |
| `src/toolchain/scripting/tests/repl_decls_multiline/{driver.z42,expected_output.txt}` | MODIFY | 追加 `DeclTypeNames` 断言（`.types` 数据源；类型 Adder/Color 排除自由函数）|
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引加 `IsTypeDecl` / `DeclTypeNames` / `FormatVersion` 行 |
| `docs/design/toolchain/repl.md` | MODIFY | 元指令落地状态刷新（消除 doc-vs-code 冲突）；`.version` 仅格式版本 + runtime 串 follow-up |

## 子系统

`toolchain`（interactive 宿主）+ `stdlib`（`z42.scripting` 库）。无 runtime、无格式 bump。

> `z42.scripting` 仅被 REPL 宿主（`z42.interactive`）引用，**不被 z42c / xtask 源引用** → 不触
> bootstrap 种子的 stdlib-API 轴（axis ②/④）；改其 API 安全。

## 非目标

`.history` / `.save` / `.mode` / `.version` 运行时串 / `ResultFormatter` 对象反射展示 —— 均留 follow-up。
