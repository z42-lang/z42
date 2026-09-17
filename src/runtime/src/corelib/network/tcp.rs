//! `Std.Net` builtins — TCP 连接 / 监听 / 收发 / 超时 / 句柄释放（非 wasm32）。refactor-split-network（2026-09-03）：自 `network.rs`
//! 内联 `mod imp` 逐行搬出；`KIND_*` / 句柄槽等共享定义仍在 hub（`use super::*`）。

#![allow(unused_imports)]
use super::*;
use std::net::{TcpListener, TcpStream, SocketAddr, ToSocketAddrs};
use std::io::{Read, Write};

/// fix-accept-not-interruptible (2026-09-17)：监听槽位 = 共享的 listener + 一个关闭标志。
///
/// 此前 `builtin_net_tcp_accept` 在阻塞前把 `TcpListener` **从表里摘走**，于是
/// `__net_tcp_listener_drop` 在表里找不到它 ⇒ `TcpListener.Stop()` 退化成空操作，
/// 连 fd 都没关；而 macOS 上即便关了 fd 也唤不醒阻塞中的 `accept`。两者叠加的结果是
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
fn wait_readable(sock: &std::net::TcpListener, timeout_ms: i32) -> std::io::Result<bool> {
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
fn wait_readable(sock: &std::net::TcpListener, timeout_ms: i32) -> std::io::Result<bool> {
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
const ACCEPT_POLL_MS: i32 = 100;

pub fn builtin_net_tcp_connect(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_connect";
    let host = arg_str(args, 0, NAME)?.to_string();
    let port = require_port(args, 1, NAME)?;

    let addr = format!("{}:{}", host, port);
    // fix-blocking-io-deadlocks-gc：**阻塞系统调用期间必须让出 GC safepoint**。
    // 线程卡在 recv/accept/connect 里时永远到不了字节码 safepoint；此时另一个线程发起 GC
    // （`request_gc_pause`）会等「全世界停下」——而这个线程停不下来 ⇒ **死锁**。
    // `NativeParkGuard` 就是为此存在的（add-repl-prewarm 给 REPL 的 readline 加的，
    // 同 JVM `_thread_in_native` / Go `entersyscall`），网络这边一直没用上。
    //
    // **fix-alloc-inside-native-park (2026-09-14)**: the park covers the syscall and nothing
    // else. The result tuple below is a GC allocation, and one made while still parked is
    // not a root for any collection running meanwhile — see
    // `gc::safepoint::debug_assert_not_native_parked`.
    let connected = { let _park = crate::gc::NativeParkGuard::enter(ctx); TcpStream::connect(&addr) };
    match connected {
        Ok(stream) => {
            let slot_id = ctx.alloc_tcp_socket_slot(stream);
            Ok(ok_value(ctx, slot_id as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("connect to {}: {}", addr, e))),
    }
}

pub fn builtin_net_tcp_listen(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_listen";
    let host = arg_str(args, 0, NAME)?.to_string();
    let port = require_port(args, 1, NAME)?;

    let bind_target = format!("{}:{}", host, port);
    // fix-park-blocking-natives (2026-09-14): name resolution (getaddrinfo) can block for the
    // resolver timeout ⇒ parked; the result tuple is allocated after the park ends.
    let bind_result = {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        bind_target.to_socket_addrs()
            .and_then(|mut iter| iter.next()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no addresses")))
            .and_then(|addr: SocketAddr| TcpListener::bind(addr))
    };

    match bind_result {
        Ok(listener) => {
            let actual_port = listener.local_addr()
                .map(|a| a.port())
                .unwrap_or(port);
            let slot_id = ctx.alloc_tcp_listener_slot(listener);
            Ok(ok_two(ctx, slot_id as i64, actual_port as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("bind {}: {}", bind_target, e))),
    }
}

pub fn builtin_net_tcp_accept(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_accept";
    let slot_id = require_slot_id(args, 0, NAME)?;

    // fix-accept-not-interruptible (2026-09-17)：**不再把 listener 摘出表**，只克隆 Arc。
    // 摘表是老实现让 `Stop()` 变成空操作的原因（drop 在表里找不到正被 accept 用着的那个）。
    let slot = match ctx.core.tcp_listeners.get_cloned(slot_id) {
        Some(s) => s,
        None => return Ok(handle_invalid(ctx)),
    };

    if let Err(e) = slot.listener.set_nonblocking(true) {
        return Ok(socket_err(ctx, format!("accept: set_nonblocking: {}", e)));
    }

    // fix-blocking-io-deadlocks-gc：**阻塞系统调用期间必须让出 GC safepoint**。
    // 线程卡在 recv/accept/connect 里时永远到不了字节码 safepoint；此时另一个线程发起 GC
    // （`request_gc_pause`）会等「全世界停下」——而这个线程停不下来 ⇒ **死锁**。
    // `NativeParkGuard` 就是为此存在的（同 JVM `_thread_in_native` / Go `entersyscall`）。
    //
    // fix-alloc-inside-native-park (2026-09-14)：park 区间内**不得分配** GC 对象，
    // 所以整个等待循环只碰 Rust 局部量，产出的 socket 槽位在出 park 之后才建。
    let accept_result = {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        loop {
            // 每轮先看关闭标志：`__net_tcp_listener_drop` 先置标志再摘表。
            if slot.is_closed() {
                break Err(AcceptWait::Closed);
            }
            match slot.listener.accept() {
                Ok(pair) => break Ok(pair),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    match wait_readable(&slot.listener, ACCEPT_POLL_MS) {
                        Ok(_)  => continue,   // 就绪或超时，都回到循环顶部复查 closed
                        Err(e) => break Err(AcceptWait::Io(e)),
                    }
                }
                Err(e) => break Err(AcceptWait::Io(e)),
            }
        }
    };

    match accept_result {
        Ok((stream, _peer)) => {
            // 连接是从非阻塞的监听 socket 上接来的；在 macOS/Linux 上它会继承
            // O_NONBLOCK，而上层的读写按阻塞语义写的 ⇒ 显式恢复。
            if let Err(e) = stream.set_nonblocking(false) {
                return Ok(socket_err(ctx, format!("accept: clear nonblocking: {}", e)));
            }
            let sock_slot = ctx.alloc_tcp_socket_slot(stream);
            Ok(ok_value(ctx, sock_slot as i64))
        }
        // listener 被 `Stop()` / `Dispose()` 关掉。`KIND_HANDLE_INVALID` 在 z42 侧就是
        // `SocketClosedException`（`NetTcpDecode.Throw`），而三个 serve 循环
        // （Serve / ServeThreaded / ServeWithPool）已经在 catch 它 —— 语义正好对上：
        // 句柄没了。
        Err(AcceptWait::Closed) => Ok(handle_invalid(ctx)),
        Err(AcceptWait::Io(e))  => Ok(socket_err(ctx, format!("accept: {}", e))),
    }
}

/// accept 等待循环的结束原因 —— 区分「listener 被关了」与真 I/O 错误。
enum AcceptWait {
    Closed,
    Io(std::io::Error),
}

pub fn builtin_net_tcp_socket_read(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_read";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let buf_arr = match args.get(1) {
        Some(Value::Array(rc)) => rc.clone(),
        other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
    };
    let offset = arg_i64(args, 2, NAME)? as usize;
    let count  = arg_i64(args, 3, NAME)? as usize;

    let buf_len = buf_arr.borrow().len();
    if offset + count > buf_len {
        bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
    }
    if count == 0 { return Ok(ok_value(ctx, 0)); }

    let stream = {
        let mut map = ctx.core.tcp_sockets.lock();
        map.remove(&slot_id)
    };
    let Some(mut stream) = stream else {
        return Ok(handle_invalid(ctx));
    };

    let mut tmp = vec![0u8; count];
    // fix-blocking-io-deadlocks-gc：**阻塞系统调用期间必须让出 GC safepoint**。
    // 线程卡在 recv/accept/connect 里时永远到不了字节码 safepoint；此时另一个线程发起 GC
    // （`request_gc_pause`）会等「全世界停下」——而这个线程停不下来 ⇒ **死锁**。
    // `NativeParkGuard` 就是为此存在的（add-repl-prewarm 给 REPL 的 readline 加的，
    // 同 JVM `_thread_in_native` / Go `entersyscall`），网络这边一直没用上。
    let read_result = { let _park = crate::gc::NativeParkGuard::enter(ctx); stream.read(&mut tmp) };

    ctx.core.tcp_sockets.lock().insert(slot_id, stream);

    match read_result {
        Ok(n) => {
            let mut borrowed = buf_arr.borrow_mut();
            for i in 0..n {
                borrowed.set_boxed(offset + i, Value::I64(tmp[i] as i64));
            }
            Ok(ok_value(ctx, n as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("read: {}", e))),
    }
}

pub fn builtin_net_tcp_socket_write(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_write";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let buf_arr = match args.get(1) {
        Some(Value::Array(rc)) => rc.clone(),
        other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
    };
    let offset = arg_i64(args, 2, NAME)? as usize;
    let count  = arg_i64(args, 3, NAME)? as usize;

    let buf_len = buf_arr.borrow().len();
    if offset + count > buf_len {
        bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
    }
    if count == 0 { return Ok(ok_value(ctx, 0)); }

    let mut tmp = vec![0u8; count];
    {
        let borrowed = buf_arr.borrow();
        // packed-primitive-arrays Step 3: packed `Bytes` → slice-copy the send
        // window in one memcpy, no per-byte unbox.
        if let Some(b) = borrowed.as_bytes() {
            tmp.copy_from_slice(&b[offset..offset + count]);
        } else {
            for i in 0..count {
                match borrowed.get_boxed(offset + i) {
                    Value::I64(v) => tmp[i] = (v & 0xFF) as u8,
                    other => bail!("{}: byte[] elem at {} expected I64, got {:?}", NAME, offset + i, other),
                }
            }
        }
    }

    let stream = {
        let mut map = ctx.core.tcp_sockets.lock();
        map.remove(&slot_id)
    };
    let Some(mut stream) = stream else {
        return Ok(handle_invalid(ctx));
    };

    // fix-blocking-io-deadlocks-gc：**阻塞系统调用期间必须让出 GC safepoint**。
    // 线程卡在 recv/accept/connect 里时永远到不了字节码 safepoint；此时另一个线程发起 GC
    // （`request_gc_pause`）会等「全世界停下」——而这个线程停不下来 ⇒ **死锁**。
    // `NativeParkGuard` 就是为此存在的（add-repl-prewarm 给 REPL 的 readline 加的，
    // 同 JVM `_thread_in_native` / Go `entersyscall`），网络这边一直没用上。
    let write_result = { let _park = crate::gc::NativeParkGuard::enter(ctx); stream.write_all(&tmp).map(|_| count) };

    ctx.core.tcp_sockets.lock().insert(slot_id, stream);

    match write_result {
        Ok(n) => Ok(ok_value(ctx, n as i64)),
        Err(e) => Ok(socket_err(ctx, format!("write: {}", e))),
    }
}

pub fn builtin_net_tcp_socket_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_drop";
    let slot_id = require_slot_id(args, 0, NAME)?;
    ctx.core.tcp_sockets.lock().remove(&slot_id);
    Ok(Value::Null)
}

// add-httpclient-timeout (2026-05-27): apply read / write deadlines so
// a misbehaving peer can't hang the script. `millis <= 0` clears the
// timeout (blocking I/O). On error returns socket_err; on missing slot
// returns handle_invalid (caller treats as already-disposed).

fn apply_timeout(
    ctx: &VmContext,
    slot_id: u64,
    millis: i64,
    which: &'static str,
) -> Result<Value> {
    let dur = if millis > 0 {
        Some(std::time::Duration::from_millis(millis as u64))
    } else {
        None
    };
    let stream = {
        let map = ctx.core.tcp_sockets.lock();
        match map.get(&slot_id) {
            Some(s) => s.try_clone(),
            None => return Ok(handle_invalid(ctx)),
        }
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => return Ok(socket_err(ctx, format!("{}: try_clone: {}", which, e))),
    };
    let result = if which == "set_read_timeout" {
        stream.set_read_timeout(dur)
    } else {
        stream.set_write_timeout(dur)
    };
    match result {
        Ok(()) => Ok(ok_value(ctx, 0)),
        Err(e) => Ok(socket_err(ctx, format!("{}: {}", which, e))),
    }
}

pub fn builtin_net_tcp_socket_set_read_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_set_read_timeout";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let millis = arg_i64(args, 1, NAME)?;
    apply_timeout(ctx, slot_id, millis, "set_read_timeout")
}

pub fn builtin_net_tcp_socket_set_write_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_set_write_timeout";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let millis = arg_i64(args, 1, NAME)?;
    apply_timeout(ctx, slot_id, millis, "set_write_timeout")
}

pub fn builtin_net_tcp_listener_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_listener_drop";
    let slot_id = require_slot_id(args, 0, NAME)?;
    // fix-accept-not-interruptible：**先置标志、再摘表**。阻塞在 accept 里的线程持有
    // 同一个 `Arc`，它下一轮轮询（≤ ACCEPT_POLL_MS）看到标志就返回 SocketClosed；
    // fd 的真正关闭发生在最后一个 Arc 放手时，因此不会在别人用着 fd 的时候关掉它。
    if let Some(slot) = ctx.core.tcp_listeners.get_cloned(slot_id) {
        slot.closed.store(true, std::sync::atomic::Ordering::Release);
    }
    ctx.core.tcp_listeners.lock().remove(&slot_id);
    Ok(Value::Null)
}
