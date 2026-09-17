# 监听 socket：为什么 accept 是非阻塞 + 轮询

> SoT：`src/runtime/src/corelib/network/tcp.rs`。
> 由 `fix-accept-not-interruptible`（2026-09-17）确立。

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

- `accept` **不再把 listener 摘出资源表**，只克隆 `Arc`（同 #648 给子进程管道用的手法）。
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

## 守这条不变量的门

| 门 | 位置 |
|---|---|
| `a_blocked_accept_is_woken_by_dropping_the_listener` | `corelib/network_tests.rs` —— 回退 `closed` 标志即**红**（实测 5.17 秒后报失败） |
| `accept_still_returns_a_real_connection` | 正向对照：别把 bug 修成「accept 不工作」 |

> 🔴 那条回归测试**必须用 detached 线程 + 带超时的 channel**，不能用 `thread::scope`：
> scope 退出时会 join worker，于是回退实现时**整个测试进程挂住**（CI 超时），而不是报一条失败。
> 写这条测试时先踩了这个坑。
