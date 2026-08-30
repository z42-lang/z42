# Design: fix-repl-launcher-process-group

## Architecture

```
terminal (foreground pgroup = shell-assigned)
  └─ z42 (launcher apphost)                      ← foreground
       └─ z42vm launcher.zpkg                    ← foreground
            └─ _forwardRepl: Process(vm).Stdin(Inherit)...Run()
                 └─ z42vm z42.interactive.zpkg   ← 若独立进程组 = BACKGROUND → 编辑器/求值坏
```

`z42i` 是单层 apphost spawn（vm 留前台）→ 正常。`z42 repl` 多一层 z42 级 `Process.Run()`，其
`process_group(0)` 让最内层 vm 落后台进程组。

## Decisions

### Decision 1: 显式 opt-out API vs stdin=Inherit 启发式
**问题：** 如何让交互式转发跳过 `process_group(0)`。
**选项：** A — `Process` 加 `ShareProcessGroup()` 显式方法；B — `run_sync` 检测 `stdin==Inherit && 无 timeout` 自动跳过。
**决定：** 选 A。意图显式、无隐式耦合；非交互 Inherit（如流式转发 build 输出）仍可各取所需。
（User 裁决：显式 opt-out 推荐。）

### Decision 2: 可选读取 arg 14（避免两-nightly）
**问题：** `__process_run` 加第 15 个参数是否触发 builtin arity 两-nightly 纪律。
**选项：** A — 必读 arg 14（严格 arity）；B — 可选读（缺省 true）。
**决定：** 选 B——`args.get(14)` 缺省 `true`。旧 zpkg（14 参）在新 VM 上仍旧行为、新 zpkg（15 参）
在旧种子 VM 上多余参被忽略（builtin 只读所需下标，无 arity 校验，见 runtime-review #10 未实现）。
双向兼容 → 无 arity 断链、无需 support/use 分两 nightly。

### Decision 3: kill_process_tree 按 flag 门控组杀
**问题：** `own_process_group=false` 时子进程 pgid=调用方 pgid，`kill(-pid)` 会误杀调用方进程组。
**决定：** `kill_process_tree(child, own_process_group)`——仅 `own_process_group` 时 `kill(-pid)`，
否则只 `child.kill()`。交互式转发（无 timeout）永不走此路径，但防御性正确、消除 footgun。

## Implementation Notes
- `own_process_group` 从 `wait_with_optional_timeout` 透传到 `kill_process_tree`。
- `Process._ownProcessGroup` 默认 true；`ShareProcessGroup()` 置 false；`Run()` 作第 15 实参传 `ProcessNative.Run`。Spawn 不传（不读该字段）。
- launcher `_forwardRepl` 在 `.Stdin(Inherit).Stdout(Inherit).Stderr(Inherit)` 后链 `.ShareProcessGroup()`。

## Testing Strategy
- Rust 单测（`process_tests.rs`，`#[cfg(unix)]`）：子进程 `ps -o pgid= -p $$` 报告 pgid，
  与 `libc::getpgrp()` 比对——own=true→异（独立组）；own=false→同（共享调用方组）；缺 arg 14→按 own=true。
- e2e（本地 pty，全 z42 链路）：新 z42vm + 重建 z42.core/z42.io + 模拟 `_forwardRepl` 的 harness，
  A/B（`ShareProcessGroup` 开/关）——关 → 复现无 `>>>`/无求值；开 → `>>> 7 / >>> 8` 恢复。
- GREEN：`cargo build` + `cargo test --lib process`（30/30）本地绿；全量 GREEN + 冷启动自举交 CI
  （z42.core/launcher 改动 + 本机 z42vm 退出挂起 → CI 权威）。
