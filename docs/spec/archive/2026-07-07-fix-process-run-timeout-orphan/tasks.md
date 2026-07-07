# Tasks: 修 process run-timeout 孤儿孙进程阻塞

> 状态：🟢 已完成 | 创建：2026-07-07 | 类型：fix（runtime）
> 占用子系统：`runtime`（ACTIVE.md 已登记）

**变更说明：** `builtin_process_run` 超时只 kill 直接子进程，不 kill 进程组。`sh -c
"sleep 5"` 若 fork `sleep` 孙进程，kill 子进程后孤儿孙进程仍持有继承的 stdout/stderr
管道写端 → reader 线程 `read_to_end` 阻塞到孙进程自然退出（5s）。超时被正确检测
（result=KIND_TIMEOUT）但函数迟 ~5s 返回。
**原因：** redesign-xtask-test 把 `test runtime`（cargo test）放开到全腿后，CI linux/macos
首次运行 `#[cfg(unix)]` 的 `run_timeout_fires_for_long_running_child` 即暴露此 bug
（该 unix-only 测试历来只在跳过它的 Windows 腿"运行"，从未真跑过）。
**文档影响：** roadmap Deferred 条目（bug 修复 → 全腿解锁）；ci.yml 注释。

- [x] 1.1 `src/runtime/src/corelib/process.rs`：`builtin_process_run` 生子进程组
  （unix `cmd.process_group(0)`）
- [x] 1.2 `wait_with_optional_timeout` 超时改 `kill_process_tree`（unix `kill(-pid,
  SIGKILL)` 灭整组 + `child.kill()` 兜底）
- [x] 1.3 本地验证：`run_timeout_fires_for_long_running_child` 0.20s 通过（原 5.0018s）
- [x] 1.4 重开全腿 `test runtime`：`.github/workflows/ci.yml` 的 step `if` 从 windows-only 放开
- [x] 1.5 roadmap Deferred 条目更新（bug 已修，全腿已开）
- [x] 1.6 GREEN 验证（本地 gate 无回归）+ commit + push + 盯 CI（全腿 runtime 转绿）

## 备注
- kill 进程组是 unix-specific（`#[cfg(unix)]` + libc）；Windows 保持 `child.kill()`。
  该测试本身 `#[cfg(unix)]`，非 unix 无此路径。
