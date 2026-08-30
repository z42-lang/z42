# Tasks: fix-repl-launcher-process-group

> 状态：🟢 已完成 | 创建：2026-08-30 | 完成：2026-08-30

## 进度概览
- [x] 阶段 1: runtime（arg 14 + 进程组门控 + kill 门控）
- [x] 阶段 2: stdlib API（ProcessNative / Process.ShareProcessGroup）
- [x] 阶段 3: launcher 接线 + 测试 + 文档

## 阶段 1: runtime
- [x] 1.1 `process.rs` `builtin_process_run` 读可选 arg 14 `own_process_group`（缺省 true）
- [x] 1.2 `cmd.process_group(0)` 改 `if own_process_group`（`#[cfg(unix)]`）
- [x] 1.3 `wait_with_optional_timeout` / `kill_process_tree` 透传 flag，`kill(-pid)` 按 flag 门控
- [x] 1.4 `process_tests.rs` 3 个回归测试（own/share/absent-defaults）

## 阶段 2: stdlib API
- [x] 2.1 `ProcessNative.z42` `Run` extern 加 `bool ownProcessGroup`
- [x] 2.2 `Process.z42` `_ownProcessGroup` 字段（默认 true）+ `ShareProcessGroup()` + Run 传参

## 阶段 3: 接线 + 验证
- [x] 3.1 launcher `_forwardRepl` 链 `.ShareProcessGroup()`
- [x] 3.2 `cargo build` + `cargo test --lib process`（30/30 本地绿）
- [x] 3.3 e2e pty A/B（全 z42 链路）验证 `>>>` + 求值恢复
- [x] 3.4 文档同步：z42.io README（Process 功能索引）+ repl.md（转发进程组要求）
- [ ] 3.5 全量 GREEN + 冷启动自举 → CI 权威（z42.core/launcher 改动本地不可全验）

## 备注
- arg 14 可选读取 = 无两-nightly（Decision 2）。零格式 bump。
