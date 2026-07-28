# Proposal: REPL 多行输入 + 函数/类型声明累积

## Why

z42 REPL（`add-z42-repl` MVP）当前只能求值表达式 / var 声明 / 语句 / using——**无法定义函数或类型**，也**只读单行**。这是 Python 对齐交互体验的两块硬缺口：用户想在 REPL 里 `int add(int a, int b) { return a + b; }` 然后 `add(1, 2)`，或多行粘贴一个 `class`，现都做不到。

前置 `fix-imported-free-func-namespace`（commit 13ae506a，已在 main）已让**导入自由函数的跨包裸调按源命名空间限定**——这正是让声明累积「零 hack」的解锁点：声明进各自命名空间 `Repl.R{N}`，后续轮 `using` 它即可裸调，无需包裹类、无需改写调用点。

## What Changes

- **多行输入**：`interactive_main` 的读入从单行 `Std.Repl.ReadLine` 换成括号平衡的 `Std.Repl.ReadBlock`（native builtin 已存在），未闭合 `()[]{}` 时用 `... ` 续行提示继续读，直到平衡。EOF/Ctrl-D 退出逻辑不变。
- **函数/类型声明累积**：`Script.Eval` 的分类器新增识别**顶层声明**——自由函数（`RetType Name(params) {...}`）与全部类型形态（`class`/`struct`/`record`/`interface`/`enum`）。声明**直接**作为成员发进本轮命名空间 `Repl.R{N}`（不包裹 `Decls` 类、不改写函数体），经 `DepScan.ExtendWithPackage` 并入跨轮缓存依赖世界 + `Engine.LoadBytes` 进 live VM；后续每轮 prelude 追加 `using Repl.R{N};`，靠 fix A（自由函数）与既有 `ImportedClassNs`（类型）跨包裸调解析。
- **重定义 = ERROR**（MVP，不 supersede）：`ScriptState` 加 `DeclNames` 集，声明名命中即报错、会话不推进。
- **编译器跨包类型元数据修复**（实施期实测发现，User 授权扩张进 compiler 子系统）：REPL 增量导入路径此前不完整重建声明类型元数据——① 类**实例方法**跨轮 `no method`（`DepScan.ExtendWithPackage` 未把增量包并入 world → `TsigReconcile._rebuildClass` 定位不到类自身、读不到其 SIGS 方法）；② **enum 类型**跨包 `undefined`（`TsigReconcile._rebuildModule` 恒排除本地 enum、只导出内建 `GCHandleType`）。两处修复后实例方法与 enum 跨轮可用；enum 修复亦令**一般跨包 enum 导入**首次工作（非仅 REPL）。**无格式 bump**（enum 数据已在 zbc TYPE 段、world-extension 纯内存）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/scripting/src/ScriptState.z42` | MODIFY | 加 `DeclNames`（去重报错集）+ `DeclNamespaces`（`Repl.R{N}` 声明包列表，供后续轮 `using`）|
| `src/toolchain/scripting/src/Script.z42` | MODIFY | `_classify` 扩识别顶层函数/类型声明；`Eval` 新增声明轮分支（发成员 / ExtendWithPackage / LoadBytes / 登记 ns+名 / 不 Invoke）；每轮 prelude 追加 `using` 全部声明 ns |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | `ReadLine(">>> ")` → `ReadBlock(">>> ", "... ")` |
| `src/compiler/z42c.pipeline/src/DepScan.z42` | MODIFY | `ExtendWithPackage`：Rebuild 前把增量包并入 `scan.Wp`（world）→ 修 REPL 类实例方法跨轮不 resolve（compiler，User 授权扩张）|
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `_rebuildModule`：本地 enum 从 TYPE 段重建 `ExportedEnumZ` 导出 → 修跨包 enum 类型导入（一般能力；此前恒排除）|
| `src/tests/cross-zpkg/enum_cross_pkg/` | NEW | 跨包 enum 导入回归夹具（磁盘全量路径，非 REPL）|
| `src/toolchain/scripting/tests/repl_decls_multiline/driver.z42` | NEW | 驱动 `Script.Eval` 跨轮：fn 声明→裸调、type 声明→实例化、重定义报错、多行声明；打印结果 |
| `src/toolchain/scripting/tests/repl_decls_multiline/expected_output.txt` | NEW | 上述驱动的期望输出 |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引加「声明累积 / 多行输入」行 |
| `docs/design/toolchain/repl.md` | MODIFY | 声明累积机制（`Repl.R{N}` + `using`，取代旧 Growing-Transcript 顶层声明区叙述）+ 多行接线落地 + 新 Deferred（声明体捕获会话变量）|
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加声明体捕获条目；0.4.0 REPL 状态刷新 |

**只读引用**（理解上下文，不改）：

- `src/toolchain/scripting/src/Rewriter.z42` — 会话变量改写（声明体**不**走它，确认边界）
- `src/toolchain/scripting/src/Repl.z42` / `Engine.z42` / `EvalResult.z42` — native 绑定 / 引擎原语 / 结果类型（不改）
- `src/runtime/src/corelib/repl.rs` — `__repl_readblock` 语义（括号平衡、EOF、忽略串/注释内括号）
- `src/tests/cross-zpkg/free_func_cross_pkg/` — fix A 的跨包裸调回归，声明累积同机制

## Out of Scope

- **声明体捕获会话变量**：声明的函数/类型体内裸引用会话变量（`Vars{N}` 在另一 ns，需限定）**不支持**——因本 change 不对声明体做 Rewriter（与 plan 锁定「NO function Rewriter」一致）。自然编译报错 + 文档限制说明 → Deferred。
- **泛型返回类型的自由函数**（`List<int> f() {...}`）：分类器 MVP 只识别 `<type> <ident> (` 形（`type` 为单标识符/内建类型 token），泛型返回签名不识别为声明 → 落表达式路径报错。文档记 MVP 限制。
- **重定义 supersede / 覆盖**：MVP 报错，不做「新定义遮蔽旧定义」。
- **富元指令 / 富结果格式化 / tab 补全**：沿 `add-z42-repl` follow-up，不在本 change。

## Open Questions

- 无（声明范围 = 函数 + 全部类型形态；add-z42-repl 已归档：均 User 2026-07-27 裁决）。
