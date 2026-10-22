//! 监听槽位与可中断的 accept 等待，供 `network/tcp.rs`（ABI v2）的 accept 使用。
//! 等待函数只碰纯 Rust 数据，可以在 park 状态下运行。

/// fix-accept-not-interruptible (2026-09-17)：监听槽位 = 共享的 listener + 一个关闭标志。
///
/// accept 若在阻塞前把 `TcpListener` **从表里摘走**，drop 就在表里找不到它 ⇒
/// `TcpListener.Stop()` 退化成空操作，连 fd 都没关；而 macOS 上即便关了 fd 也唤不醒
/// 阻塞中的 `accept`。两者叠加的结果是
/// `HttpServer` 只能靠一次性自连探针唤醒 accept，探针一漏就**永久死锁**
/// （实测：整包工作台挂过 44 小时）。
///
/// 现在 accept **不摘表**、只克隆 `Arc`（与 #648 给子进程管道用的手法同构）；
/// `drop` 先置 `closed` 再摘表，真正的 fd 关闭发生在最后一个 `Arc` 放手时
/// —— 也就是 Go 的 netpoll 用引用计数做的那件事：**不在别人用着 fd 的时候关它**。
#[derive(Clone)]
pub(crate) struct ListenerSlot {
    pub(crate) listener: std::sync::Arc<std::net::TcpListener>,
    pub(crate) closed:   std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ListenerSlot {
    pub(crate) fn new(listener: std::net::TcpListener) -> Self {
        Self {
            listener: std::sync::Arc::new(listener),
            closed:   std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// 等这个 socket 变得「可读」（对监听 socket 而言 = 有连接在队列里），或超时。
///
/// `Ok(true)` = 就绪；`Ok(false)` = 超时；`Err` = 真错误。`EINTR` 按超时处理
/// （由上层循环重试并顺带复查 `closed`）。
///
/// **为什么必须轮询**：Unix 上没有可移植的手段中断阻塞中的 `accept`——
/// `shutdown()` 在 Linux 能唤醒、**macOS 返回 ENOTCONN 且不唤醒**；
/// 关 fd 属未定义行为；`SO_RCVTIMEO` 实测在 macOS 上**对 accept 不生效**
/// （150ms 超时的 accept 跑满 10 分钟没返回）。
/// Go / libuv 的做法是「非阻塞 fd + 中央事件循环 + 显式唤醒」，那等于给运行时引入事件循环；
/// 在没有事件循环的前提下，带超时的 `poll` 是唯一一条不用为每个平台写一套唤醒通道的路。
/// 代价是每个**监听** socket 每秒 10 次空转唤醒（一个进程通常只有一两个监听 socket）。
/// Deferred：将来运行时真需要事件循环时，换成 eventfd / `EVFILT_USER` / 自管道显式唤醒。
#[cfg(unix)]
pub(crate) fn wait_readable(sock: &std::net::TcpListener, timeout_ms: i32) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut pfd = libc::pollfd { fd: sock.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // SAFETY: 单个合法 pollfd，fd 由 `sock` 借用保活。
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    match rc {
        n if n > 0 => Ok(true),
        0          => Ok(false),
        _          => {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted { Ok(false) } else { Err(e) }
        }
    }
}

#[cfg(windows)]
#[repr(C)]
struct WsaPollFd { fd: usize, events: i16, revents: i16 }

#[cfg(windows)]
extern "system" {
    fn WSAPoll(fdarray: *mut WsaPollFd, fds: u32, timeout: i32) -> i32;
}

#[cfg(windows)]
pub(crate) fn wait_readable(sock: &std::net::TcpListener, timeout_ms: i32) -> std::io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    const POLLRDNORM: i16 = 0x0100;
    let mut pfd = WsaPollFd { fd: sock.as_raw_socket() as usize, events: POLLRDNORM, revents: 0 };
    // SAFETY: 单个合法 WSAPOLLFD，socket 由 `sock` 借用保活。
    let rc = unsafe { WSAPoll(&mut pfd, 1, timeout_ms) };
    match rc {
        n if n > 0 => Ok(true),
        0          => Ok(false),
        _          => Err(std::io::Error::last_os_error()),
    }
}

/// accept 的轮询周期。关闭延迟上限 = 这个值；空闲监听 socket 每秒醒 1000/这个值 次。
pub(crate) const ACCEPT_POLL_MS: i32 = 100;

/// accept 等待的结束原因 —— 区分「listener 被关了」与真 I/O 错误。
pub(crate) enum AcceptWait {
    Closed,
    Io(std::io::Error),
}

/// 等一个连接进来：把监听 socket 设成非阻塞，循环 `accept` + [`wait_readable`]，每轮先看
/// 关闭标志（drop 先置标志再摘表，≤ [`ACCEPT_POLL_MS`] 内返回 `Closed`）。纯 Rust，park 中调用。
/// 接来的连接已恢复成阻塞模式（它在 macOS / Linux 上继承监听 socket 的 `O_NONBLOCK`，而读写按阻塞语义写）。
pub(crate) fn accept_wait(slot: &ListenerSlot) -> Result<(std::net::TcpStream, std::net::SocketAddr), AcceptWait> {
    slot.listener.set_nonblocking(true).map_err(AcceptWait::Io)?;
    loop {
        if slot.is_closed() {
            return Err(AcceptWait::Closed);
        }
        match slot.listener.accept() {
            Ok((stream, peer)) => {
                stream.set_nonblocking(false).map_err(AcceptWait::Io)?;
                return Ok((stream, peer));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 就绪或超时，都回到循环顶部复查 closed
                wait_readable(&slot.listener, ACCEPT_POLL_MS).map_err(AcceptWait::Io)?;
            }
            Err(e) => return Err(AcceptWait::Io(e)),
        }
    }
}
