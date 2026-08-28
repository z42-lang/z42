# Proposal: 拆分 z42.repl（终端交互层）出 z42.scripting

> 类型：`refactor`（重组，不改外部行为）。REPL 交互行为逐字节等价。
> 归属：`stdlib-interop-and-repl-split-program` 轴 2 的 **PR1**（多 PR 收敛程序的第一步）。

## Why

`z42.scripting` 现在把两类不同关注点塞进一个包：
- **跨平台 eval-core**（编译一段源 → 内存加载 → 反射求值 → 补全 / 完整性判定）——playground / wasm 也用；
- **tier1 终端交互**（rustyline 行编辑、缩进感知键位）——只在真 tty + 终端 REPL 用。

二者混在一起，妨碍后续「eval-core 进 stdlib 供 playground 共享 / 终端交互作可缺席能力库门控」的分层
（见程序方向：编译器件增量收敛进 stdlib）。本 PR 把终端交互层拆成独立包 `z42.repl`，先把 tier 边界切干净；
**不碰编译器依赖收敛**（那是 PR2+），故 bootstrap 风险低。

## What Changes

- 新建包 `z42.repl`（`src/toolchain/repl/`，命名空间 `Std.Repl`），收 **Repl（rustyline tty 绑定）+ ReplEditing（键位策略）**。
- `ReplEditing` 命名空间 `Std.Scripting` → `Std.Repl`（连带注册串 `Std.Scripting.replKeyEdit` → `Std.Repl.replKeyEdit`）。
- **根因修复循环依赖**：`Repl.MemberNames`（`__repl_member_names`，读静态字段 live 值成员名——本质是**反射查询**、
  非 tty 行编辑，历史误置于 `Std.Repl.Repl`）下移到 `Std.Scripting.Engine.MemberNames`（native 名不变）。
  否则 `Completer`（留 scripting）调 `Repl.MemberNames` → scripting→repl，而 repl→scripting（ReplEditing→Completeness）成环。
  下移后：scripting 不依赖 repl；repl 单向依赖 scripting（Completeness）。
- `Completer` 改用 `Engine.MemberNames`、去掉 `using Std.Repl`。
- `Completer` / `replComplete`（Tab 补全）**留 scripting**——补全是跨平台反射，playground 也用。
- z42.interactive 增加 `z42.repl` 依赖；`SetKeyEditor` 注册串更新。
- build：`_buildScriptingLib` 在建 z42.scripting 后追加建 z42.repl，并入合并 libs 目录供 interactive 解析。
- runtime 注释（`repl.rs` / `repl_editing.rs`）里 `Std.Scripting.ReplEditing` 的 FQN 引用同步为 `Std.Repl.ReplEditing`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/repl/z42.repl.z42.toml` | NEW | 新包清单（deps: z42.core + z42.scripting） |
| `src/toolchain/repl/src/Repl.z42` | NEW | 从 scripting 迁入；`Std.Repl.Repl` 只留 tty 三件套（ReadLine/SetCompleter/SetKeyEditor），删 MemberNames |
| `src/toolchain/repl/src/ReplEditing.z42` | NEW | 从 scripting 迁入；命名空间 `Std.Scripting`→`Std.Repl` |
| `src/toolchain/repl/README.md` | NEW | 目录 README（六段制） |
| `src/toolchain/repl/tests/repl_editing/driver.z42` | NEW | 从 scripting 迁入；`using Std.Scripting`→`using Std.Repl` |
| `src/toolchain/repl/tests/repl_editing/expected_output.txt` | NEW | 从 scripting 迁入（内容不变） |
| `src/toolchain/scripting/src/Repl.z42` | DELETE | 迁往 z42.repl |
| `src/toolchain/scripting/src/ReplEditing.z42` | DELETE | 迁往 z42.repl |
| `src/toolchain/scripting/tests/repl_editing/driver.z42` | DELETE | 迁往 z42.repl/tests |
| `src/toolchain/scripting/tests/repl_editing/expected_output.txt` | DELETE | 迁往 z42.repl/tests |
| `src/toolchain/scripting/src/Engine.z42` | MODIFY | 增 `MemberNames` [Native("__repl_member_names")] |
| `src/toolchain/scripting/src/Completer.z42` | MODIFY | 去 `using Std.Repl`；`Repl.MemberNames`→`Engine.MemberNames` |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引 / 核心文件去 Repl·ReplEditing；关联到 z42.repl |
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | `SetKeyEditor("Std.Scripting.replKeyEdit")`→`"Std.Repl.replKeyEdit"` |
| `src/toolchain/interactive/core/z42.interactive.z42.toml` | MODIFY | deps 增 `"z42.repl" = "0.1.0"` |
| `scripts/build/xtask_toolchain.z42` | MODIFY | `_buildScriptingLib` 追加建 z42.repl + 并入合并目录 |
| `src/runtime/src/corelib/repl.rs` | MODIFY | 注释 FQN `Std.Scripting.ReplEditing`→`Std.Repl.ReplEditing` |
| `src/runtime/src/corelib/repl_editing.rs` | MODIFY | 注释 FQN 同上 |
| `src/toolchain/README.md` | MODIFY | 子目录表增 `repl/`（REPL 终端交互库） |
| `docs/design/toolchain/repl.md` | MODIFY | 反映包拆分（scripting=eval-core / repl=tty 交互） |

**只读引用**（理解上下文，不改）：
- `src/runtime/src/corelib/repl_editing_tests.rs` / `repl_tests.rs` — Rust 单测测 native 适配壳，按 builtin 名不受 z42 包拆分影响
- `scripts/packages.toml` — stdlib-glob 自动发现 libs 目录里的 zpkg（z42.repl.zpkg 建到 libs → 自动入包），**无需改**

## Out of Scope

- **scripting 迁 `src/libraries/`**：需先做语法/门面收敛（PR2+）让它变 stdlib-only，本 PR 不做。
- **tier1 能力门控**（`Platform.Capabilities()` 判 z42.repl 可缺席）：留 follow-up。
- **Completer 迁移**：补全留 scripting（eval-core）。
- **编译器依赖抽象收敛**（z42.syntax 小库 / 扩 ICompiler 门面）：PR2+。

## Open Questions

- 无（MemberNames 下移是消环的唯一干净解，已在 What Changes 说明）。
