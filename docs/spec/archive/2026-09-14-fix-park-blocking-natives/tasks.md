# Tasks: fix-park-blocking-natives

> 状态：🟢 已完成 | 创建：2026-09-14 | 完成：2026-09-14 | 类型：fix（最小化模式）

**变更说明：** 给剩下没 park 的阻塞 native 调用补 `NativeParkGuard`（TLS / 子进程管道 / DNS / 并行 golden 的 join），
并把子进程管道 I/O 移出进程表锁。
**原因：** 阻塞在系统调用里的线程到不了字节码 safepoint；不 park ⇒ 任何线程发起的 GC 都等它到天荒地老（#598 同一判据）。
这也是「新上下文不得在 Marking 期注册」那一步的前置障碍：那一步会让注册者等 GC，若某个已注册线程卡在未 park 的调用里就死锁。
**文档影响：** `.claude/rules/runtime-rust.md` 新增「阻塞的 native 调用」一节（park / park 内不分配 / 不在共享锁下阻塞）。

## 实施
- [x] 1.1 `corelib/tls.rs`：DNS + connect + 握手整段 park（错误先收成 Rust `String`，出 park 再造元组）；read / write+flush 各自 park
- [x] 1.2 `corelib/process.rs`：`ProcessSlot` 三根管道改 `Arc<Mutex<_>>`（每管道一把锁）。流式读 / 写 stdin 先在进程表锁下克隆 `Arc`、放锁，再 park 着锁管道做阻塞 I/O
      —— 原先阻塞在**全局进程表锁里**，别的线程任何 process builtin 都会**不 park 地**排在这把锁上，只 park 读者本身不够
- [x] 1.3 `corelib/process.rs`：`Run` 的一次性 stdin 写入、`TryWait` 退出后的 drain 补 park
- [x] 1.4 DNS：`tcp.rs` listen、`tcp_options.rs` connect_with_timeout / listen_with_options、`udp.rs` bind / `__net_dns_lookup`
- [x] 1.5 `reflection/module_load.rs`：并行 golden 的 `join` park。顺序分支**不** park —— 它在本线程上跑嵌套 VM 并分配，
      而 park 绊线（`debug_assert_not_native_parked`）按线程计数，会误报
- [x] 1.6 文档：`.claude/rules/runtime-rust.md`

## 测试
- [x] `process_tests::blocked_stdout_read_is_parked_and_leaves_the_process_table_free`（含「另一线程的 process builtin 不排在阻塞读后面」探针）
- [x] `process_tests::stdin_write_into_a_full_pipe_is_parked`
- [x] `tls_tests::stalled_handshake_is_parked_for_gc`（对端 accept 后不应答；挂断后错误元组须在 park 外构造）
- [x] `network_tests::dns_lookup_builds_its_result_outside_the_park`
- [x] 阴性对照：只回退实现（保留测试）⇒ 前三条全红（parked_count 始终为 0）；恢复后全绿

## 验证
- [x] `cargo test --lib`（debug）1298 passed；`--test cross_thread_smoke` 9 passed
- [x] `xtask test` 全绿

## 备注
- 不在本变更内（发现未修）：`__process_run` 先写 stdin payload、后起 stdout/stderr 读线程 —— 子进程若先写满 stdout
  再读 stdin，两边互等，与 GC 无关的纯 I/O 死锁。另立变更。
