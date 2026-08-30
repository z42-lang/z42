# Proposal: fix-repl-launcher-process-group

## Why
`z42 repl`（launcher 转发）在**真 tty** 下完全不可用：无 `>>>` 提示、输入后无任何求值输出
（cooked echo）。而 `z42i`（apphost 直跑）正常。根因：`Process.Run()` 的 native 实现无条件
`cmd.process_group(0)`（为 run-timeout 能杀整棵子进程树），把 launcher `_forwardRepl` 转发的
交互式 REPL vm 放进**自己的后台进程组**——拿不到控制终端（rustyline 行编辑器初始化失败），
后台读 tty 触发 SIGTTIN 阻塞（不求值）。管道模式无 tty 所以不复现。

受控 pty 实验实证：孙子 vm 独立进程组 → 复现症状；继承父（前台）进程组 → `>>>` + 求值全恢复。

## What Changes
- runtime `__process_run` 读取可选 arg 14 `own_process_group`（缺省/`true` = 旧行为：子进程独立
  进程组，timeout 树杀不变；`false` = 子进程留在调用方进程组）。**可选读取**（缺省 true）→ 新旧
  VM / zpkg 双向兼容，无 arity / 两-nightly 耦合。
- `kill_process_tree` 只在子进程确有独立进程组时才 `kill(-pid)` 组杀，否则只杀直接子进程
  （防 `own_process_group=false`+timeout 误杀调用方进程组的 footgun）。
- stdlib `Std.IO.Process` 新增 `ShareProcessGroup()` builder（`ProcessNative.Run` 加 `ownProcessGroup` 形参）。
- launcher `_forwardRepl` 转发交互式 REPL 时调 `.ShareProcessGroup()`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/process.rs` | MODIFY | 读 arg 14；`process_group(0)` 与 `kill(-pid)` 按 flag 门控 |
| `src/runtime/src/corelib/process_tests.rs` | MODIFY | 3 个进程组回归测试（own/share/absent-defaults） |
| `src/libraries/z42.core/src/IO/ProcessNative.z42` | MODIFY | `Run` extern 加 `bool ownProcessGroup` 形参 |
| `src/libraries/z42.io/src/Process.z42` | MODIFY | `_ownProcessGroup` 字段 + `ShareProcessGroup()` builder + Run 传参 |
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | `_forwardRepl` 调 `.ShareProcessGroup()` |
| `src/libraries/z42.io/README.md` | MODIFY | Process 功能索引加 ShareProcessGroup |
| `docs/design/toolchain/repl.md` | MODIFY | 记 launcher 转发的进程组要求 |

**只读引用**：
- `src/toolchain/interactive/core/interactive_main.z42` — 理解 REPL 读循环
- `src/runtime/src/corelib/repl_native.rs` — 理解编辑器 tty 依赖

## Out of Scope
- `__process_spawn` 不加此参数（Spawn 从不作 tty 透传；`ShareProcessGroup` 字段只 `Run` 读）。
- Windows 进程组语义（`process_group` 仅 `#[cfg(unix)]`；Windows 行为不变）。

## Open Questions
- 无。
