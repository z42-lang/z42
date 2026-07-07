# Tasks: fix-signal-test-stale-version

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-07 | 类型：fix（单行 bugfix）

**变更说明：** `src/runtime/tests/signal_handler_e2e.rs:76` 的 build-banner 断言硬编码
`"z42vm 0.1.0"`，而 runtime crate 已 bump 到 0.3.0（banner 用 `env!("CARGO_PKG_VERSION")`
写 `z42vm 0.3.0`）→ 断言失配。改为 `format!("z42vm {}", env!("CARGO_PKG_VERSION"))`，
随 crate 版本自动跟随，永不因 bump 再失配。
**原因：** 这 7 个 `#[cfg(unix)]` 信号测试历来只在跳过它们的 Windows 腿"跑"；今日
redesign-xtask-test + fix-process-run-timeout-orphan 把 `test runtime` 放开到全腿后，首次在
CI unix 真跑 → 暴露此 pre-existing 陈旧版本硬编码（与 fix-process-run-timeout-orphan 修的
run-timeout bug 同源：都是"全腿放开后新暴露"）。CI run 28863304715 三条 test-host 腿因此红。

## 诊断证据
- CI stderr dump（run 28863304715, test-host linux-x64）实际 banner = `z42vm 0.3.0 (debug,
  linux/x86_64)`，且 `=== z42 call stack` / `VmCore` 标记均在场——**版本断言是唯一失败点**
  （panic 全在 line 75/76），故只改这一行即修复全 7 个测试（5 signal-marker + 2 crash-dir）。
- 本地 macOS 信号受限沙箱下这些测试 hang（文档已知），无法本地 green；但修复确定性：
  banner 与断言同 crate 同 `env!("CARGO_PKG_VERSION")` → 恒等匹配。`cargo test --no-run` 编译通过。

## 任务
- [x] 1.1 line 76 `"z42vm 0.1.0"` → `&format!("z42vm {}", env!("CARGO_PKG_VERSION"))`
- [x] 1.2 全 src/runtime grep 确认无其它 `0.1.0` 版本硬编码（仅此一处）
- [x] 1.3 `cargo test --no-run --test signal_handler_e2e` 编译通过
- [x] 1.4 归档（GREEN 以 CI test-host 三腿为准——本地沙箱不可跑信号测试）
