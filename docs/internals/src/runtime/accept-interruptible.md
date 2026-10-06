# socket：Close 怎么打断阻塞中的调用

> SoT：`src/runtime/src/corelib/network/tcp.rs`、`udp.rs`、`corelib/tls.rs`。

监听 socket 的 accept 与已连接 socket 的读写是两套做法：accept 只能非阻塞 + 轮询（下面前几节），
已连接的 socket 可以用 shutdown 唤醒（见[已连接的 socket](#已连接的-sockettcp--tls--udp)）。

## 要解决的问题

`TcpListener.Stop()` / `Dispose()` 必须能让**另一个线程里**阻塞中的 `AcceptTcpClient()` 返回，
否则 `Thread.Join()` 永久挂起。这不是理论问题：整包测试曾因此**挂死 44 小时**。

## 为什么不能直接关 fd

| 手段 | 实际行为 |
|---|---|
| `close(listenfd)` | 在别的线程正 `accept` 时关 fd 属未定义行为（可能挂死，可能更糟） |
| `shutdown(listenfd)` | **Linux 能唤醒** accept；**macOS/BSD 返回 `ENOTCONN`，不唤醒** |
| `SO_RCVTIMEO`（给监听 socket 设读超时） | **macOS 上对 `accept` 不生效** —— 实测：150ms 超时的 accept 跑满 10 分钟没返回 |
| Windows `closesocket()` | **会**让阻塞的 accept 以错误返回（POSIX 没有这个保证） |

也就是说：**Unix 上没有可移植的手段中断阻塞中的 `accept`**，macOS 尤其没有。

## 现行做法

监听槽位是 `ListenerSlot { listener: Arc<TcpListener>, closed: Arc<AtomicBool> }`：

- `accept` **不把 listener 摘出资源表**，只克隆 `Arc`（与子进程管道用的手法相同）。
  然后把 socket 设为非阻塞，在 `NativeParkGuard` 内循环：
  `accept()` → `WouldBlock` → `poll(fd, POLLIN, 100ms)` → 回到循环顶部复查 `closed`。
- `__net_tcp_listener_drop` **先置 `closed`、再摘表**。阻塞中的 accept 下一轮（≤100ms）看到标志，
  返回 `KIND_HANDLE_INVALID` —— z42 侧就是 `SocketClosedException`，三个 serve 循环都在 catch 它。
- **fd 的真正关闭发生在最后一个 `Arc` 放手时**，所以不会在别人用着 fd 的时候关掉它 ——
  这正是 Go netpoll 用引用计数做的那件事。
- accept 收下的连接会继承 `O_NONBLOCK`，而上层读写按阻塞语义写的 ⇒ **显式 `set_nonblocking(false)`**。

代价：每个**监听** socket 每秒 10 次空转唤醒（一个进程通常只有一两个），关闭延迟上限 100ms。

## 其它语言怎么做（以及我们为什么没照抄）

| 运行时 | 做法 |
|---|---|
| **Go** | 全非阻塞 + netpoll；`Close` 先标记、**evict 等在该 fd 上的 goroutine**，引用计数归零才真关 |
| **libuv / Node** | 同构；跨线程唤醒用 eventfd（Linux）/ 自管道 |
| **Java NIO** | `Selector.wakeup()` = 自管道（Linux 后改 eventfd，Windows 用回环 socket 对） |
| **Java 传统 Socket** | JDK 13 起在 NIO 上重写：非阻塞 + poll，`close` 置标志后**给阻塞线程发信号**让 syscall 以 EINTR 返回 |
| **Python** | 阻塞 `accept()` 被别的线程 close 不保证唤醒（官方已知坑）；标准答案是 `settimeout()` 轮询或 selectors |

主流方案的本质是「非阻塞 fd + 中央事件循环 + 显式唤醒通道」。**那等于给 z42 运行时引入事件循环**，
是架构级改动，不该由一个死锁修复顺带完成。在没有事件循环的前提下，带超时的 `poll` 是唯一一条
不用为每个平台各写一套唤醒通道的路（macOS/BSD 用 `EVFILT_USER`、Linux 用 eventfd、
Windows 的 `WSAPoll` 只能 poll socket 所以还得换回环 socket 对 —— 三套实现）。

> **Deferred**：真需要显式唤醒（大量监听 socket / 省电诉求）时，换成 eventfd / `EVFILT_USER` / 自管道。
> 唤醒机制完全封在 `builtin_net_tcp_accept` 里，z42 侧只看到「accept 抛 `SocketClosedException`」，
> 换实现不影响任何调用方。**触发条件：运行时开始需要事件循环时一起做。**

## 已连接的 socket（TCP / TLS / UDP）

资源表里存的是 `Arc`。读写**只克隆 `Arc`、不把 socket 摘出表**：

- **全双工**：一个线程阻塞在 read 时，另一个线程照样能 write 同一个 socket。摘表的做法会让并发的
  write 拿到「句柄无效」。
- **Close 能打断阻塞中的 read**：`drop` 摘表后对 socket 调 `shutdown(Both)`。对**已连接**的 socket，
  shutdown 在 Linux 和 macOS 上都能唤醒阻塞中的 recv（不像监听 socket 的 accept）。read 返回后发现
  槽位已经不在表里，报 `KIND_HANDLE_INVALID`（z42 侧的 `SocketClosedException`）。
- **不泄漏 fd**：摘表的做法在读完后会把 socket 放回表里，已关闭的 socket 就此复活、fd 泄漏。
  现在 fd 在最后一个 `Arc` 放手时关闭。

两处例外：

- **TLS 读写互斥**：rustls 的会话状态读写都要 `&mut`，所以槽位是 `TlsSlot { stream: Mutex<StreamOwned>, sock }`，
  `sock` 是同一 socket 的 dup，用来 shutdown 和设超时（不必等会话锁）。一个线程阻塞在 read 时，另一个线程的
  write 要等它返回。真正的全双工需要把 rustls 状态与阻塞的 socket I/O 分开锁。
- **UDP 没有 shutdown**：关闭不能唤醒阻塞中的 recv，它要等到下一个数据报或读超时才返回；fd 随后关闭。

## 守这条不变量的门

| 门 | 位置 |
|---|---|
| `a_blocked_accept_is_woken_by_dropping_the_listener` | `corelib/network_tests.rs` —— 回退 `closed` 标志即**红**（实测 5.17 秒后报失败） |
| `accept_still_returns_a_real_connection` | 正向对照：别把 bug 修成「accept 不工作」 |
| `a_socket_is_full_duplex_and_close_wakes_a_blocked_read` | `corelib/network_tests.rs` —— 读阻塞期间能写；Close 唤醒读并报句柄已关；槽位不复活 |

> 🔴 那条回归测试**必须用 detached 线程 + 带超时的 channel**，不能用 `thread::scope`：
> scope 退出时会 join worker，于是回退实现时**整个测试进程挂住**（CI 超时），而不是报一条失败。
