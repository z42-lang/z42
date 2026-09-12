# Tasks: 阻塞 socket 调用不让出 GC safepoint → 死锁

> 状态：🟢 已完成 | 完成：2026-09-12
> 变更类型：`fix`（最小化模式）

**变更说明：** 线程阻塞在 socket 的 `recv` / `accept` / `connect` / `send` 期间**永远到不了字节码
safepoint**；此时另一个线程发起 GC（`request_gc_pause`）会等「全世界停下」，而这个线程停不下来
⇒ **整个进程死锁**。

**这不是测试的问题，是 VM 的**：任何多线程 z42 程序，只要一边阻塞读、另一边触发 GC 就会挂。

**现场（`sample` 抓的两个线程栈）：**
```
主线程    z42::corelib::network::tcp::builtin_net_tcp_socket_read → __recvfrom   ← 永不返回
服务线程  z42::interp::exec_function_body
            → z42::gc::safepoint::check_safepoint_slow
              → z42::gc::safepoint::request_gc_pause
                → parking_lot::condvar::wait                                      ← 等全世界停下
```

**修法：** 机制**早就有了** —— `NativeParkGuard`（`gc/safepoint.rs`，add-repl-prewarm 2026-07-29
为 REPL 的 `readline` 加的，同 JVM `_thread_in_native` / Go `entersyscall`）。它让阻塞期间的线程
计入 `parked_count`、其根被冻结可被安全扫描。**网络模块从来没用上它** —— 七个阻塞点逐个包上：

| 文件 | 阻塞点 |
|---|---|
| `network/tcp.rs` | `TcpStream::connect` / `listener.accept` / `stream.read` / `stream.write_all` |
| `network/tcp_options.rs` | `TcpStream::connect_timeout` |
| `network/udp.rs` | `sock.send_to` / `sock.recv_from` |

**怎么发现的：** `xtask-forward-tests-to-z42b` 的转发让 z42b **in-process 编译完整个父包**再跑测试
⇒ 堆很大 ⇒ 测试期间几乎必然触发 GC。旧路径只加载一个小产物、GC 压力小，所以这个坑一直没被踩到。
症状是 `z42.net` 的 `http_digest_md5` 在 4 路并行全量跑里**停滞三小时**；**单独跑却通过**
（无 GC 触发）—— 这种「只在负载下出现」的形态正是它长期隐形的原因。

- [x] 1.1 七个网络阻塞点包上 `NativeParkGuard`
- [x] 1.2 验证：`z42.net` 全套 279 用例经 z42b in-process 路径跑通、无停滞
- [x] 1.3 验证（原始触发条件）：转发 + `--jobs 4` 全量 stdlib —— 修复前卡死三小时，
      修复后 **193s / 332 全过**
- [x] 1.4 GREEN：`xtask test` 全 13 stage 绿；`cargo test --lib` 1251 + 21 全过

## 后续（未做，独立变更）

其余可能阻塞的 native 调用尚未审计（文件 I/O、`Thread.Join`、`Sleep`、进程 `Wait`）。
判据是同一条：**任何可能长时间不返回的 native 调用都必须包 `NativeParkGuard`**。
