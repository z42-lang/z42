# DRAFT：阻塞中的 accept 唤不醒 —— HttpServer.Stop() 靠一次性探针，漏了就永久死锁

- **类型**：vm（改运行期网络 builtin 的阻塞语义）+ stdlib
- **状态**：已确认（User 2026-09-17 拍板：方案 A 轮询 + 引用计数；B/C 记 Deferred），已实施
- **日期**：2026-09-17

## 1. 问题

**实测**（`repro/accept_wake.z42`，本 worktree）：一个线程阻塞在 `lst.AcceptTcpClient()`，
另一个线程调用 `lst.Stop()` —— **accept 不返回**，等 2 秒仍卡着。

两条原因叠加：

1. `builtin_net_tcp_accept`（`corelib/network/tcp.rs:63`）**先把 listener 从表里 `remove` 走**再阻塞，
   于是 `builtin_net_tcp_listener_drop`（同文件 :256）在表里**找不到它**，`Stop()` 退化成空操作——
   连「关 fd」都没发生。
2. 即便真关了 fd，macOS 也不保证唤醒阻塞中的 `accept`（代码注释自己写了这点）。

因此 `HttpServer` 只能靠 `Stop()` 里的**自连探针**唤醒 accept。而那个探针
（`HttpServer.z42:282`）**连上就立刻 `probe.Dispose()`**：如果 accept 还没把它从 backlog 取走，
连接被 RST 后会从队列里消失 ⇒ **accept 永不返回 ⇒ `srv.Join()` 永久挂起**。

## 2. 影响（实测数据）

| 现场 | 观测 |
|---|---|
| 整包工作台 9-14 那轮 | 一轮**挂了 44 小时**（`http_server_threaded` 零输出），白等两天 |
| 最新 main 30 轮工作台 | 1 次 HANG |
| 独立复现器 6 进程并行 | **4 个卡死**，`sample` 栈一致：主线程 `pthread_join`、服务线程停在 `accept()` |
| IC A/B 的 ic-on2 组 | 4 次 HANG |

它不止会挂测试，还会**污染任何 flake 统计**（一轮挂住 = 该轮数据作废），也是 CI 偶发超时的一个来源。

## 3. 方案（根因修复）

**让阻塞中的 accept 可被中断**，然后**删掉探针这套 hack**（按 philosophy.md：不留兼容/绕行路径）。

1. **运行期**（`corelib/network/tcp.rs`）：
   - listener 在表里改为 `Arc<TcpListener>` + 一个 `closed: AtomicBool`；accept **不再 `remove`**，
     而是在表锁下 `Arc::clone`（与 #648 给子进程管道用的手法同构）。
   - accept 改为：`set_nonblocking(true)` 一次，然后 `NativeParkGuard` 内
     `libc::poll(fd, POLLIN, 100ms)` 轮询；每次超时回来检查 `closed` ⇒ 置位则返回
     `SocketClosed`（z42 侧已有 `SocketClosedException`，三个 serve 循环**已经**在 catch 它）。
   - `listener_drop` 先置 `closed = true` 再摘表。
   - `socket2` / `libc` 都已是依赖，无需新增。
2. **stdlib**（`z42.net/src/Http/HttpServer.z42`）：`Stop()` 删掉自连探针整段；三个 serve 循环
   （`Serve` / `ServeThreaded` / `ServeWithPool`）靠 `SocketClosedException` 正常退出。
   顺带删掉 `ServeThreaded` 里「探针 peer 要丢弃」的那段注释与分支。

### 备选（不推荐）

纯 stdlib 打补丁：探针改成「重试直到服务循环确认退出、且不提前 Dispose」。**仍是在 hack 上加 hack**，
且解决不了「`Stop()` 对阻塞中的 listener 是空操作」这个根子——任何直接用 `TcpListener` 的用户代码照样会挂。

## 4. 实施记录

- 运行期 `corelib/network/tcp.rs`：新增 `ListenerSlot { listener: Arc<TcpListener>, closed: Arc<AtomicBool> }`；
  `accept` 改为**不摘表**、克隆 Arc + 非阻塞 + `poll(POLLIN, 100ms)` 循环复查 `closed`；
  `listener_drop` 先置标志再摘表；收下的连接显式 `set_nonblocking(false)`（否则继承 O_NONBLOCK）。
  `wait_readable` 按平台分：unix 用 `libc::poll`，windows 用手写 extern 的 `WSAPoll`（都无新依赖）。
- stdlib `HttpServer.Stop()`：**自连探针整段删除**（三个 serve 循环靠既有的 `SocketClosedException` 退出）。
- 机制页 `docs/internals/src/runtime/accept-interruptible.md`（已接 SUMMARY），含各语言做法对照与 Deferred 记录。

### 实测否定结论（省得后人再试）

- **macOS 上 `SO_RCVTIMEO` 对 `accept` 不生效**：给监听 socket 设 150ms 读超时后 `accept` 跑满 10 分钟没返回。
  所以"设读超时 + 循环"这条更省事的路走不通，必须非阻塞 + `poll`。
- `shutdown(listenfd)` 在 Linux 能唤醒 accept，**macOS 返回 ENOTCONN 且不唤醒**。

## 5. 验证计划

- **Rust 单测**（决定性、确定性）：一个线程阻塞 accept，另一线程 drop listener ⇒ accept 必须在 ~1 秒内
  返回 SocketClosed。**阴性对照**：回退实现 ⇒ 该测试挂死/超时。
- **z42 回归**：`repro/accept_wake.z42` 转成 z42.net 的测试单元（服务器空转在 accept → Stop → Join 必须返回）。
- 复现器：改前 6 进程并行 4 个卡死；改后应 0 卡死。
- `xtask test` GREEN + `cargo test` 全量（含网络单测）。
- 关注点：轮询周期 100ms 会给空闲 accept 带来每秒 10 次唤醒——需确认对 bench 无影响
  （accept 本就是慢路径；必要时用 `poll` 的无限超时 + 自管道唤醒替代轮询）。
