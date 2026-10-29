//! Socket platform operations — the OS-specific parts of `Std.Net`.
//!
//! runtime-audit §6.1 L-2b: migrates the unix / windows branches out of
//! `corelib/network/{error,listener,tcp}.rs` (errno → Winsock code table, the
//! interruptible accept wait, the keep-alive knobs each OS exposes). Callers
//! see std types only; the `#[cfg(...)]` split lives entirely in this file.
//!
//! Everything except [`set_tcp_keepalive`] is plain std / libc and compiles on
//! every target (other targets degrade: no mapping, `Unsupported`).
//! [`set_tcp_keepalive`] goes through `socket2` and exists with the `net`
//! feature only.

use std::io;
use std::net::TcpListener;

/// The Winsock (`WSAE*`) code equivalent to the raw OS error `raw` — the numbering
/// .NET's `SocketError` uses on every platform. Unix: .NET's errno table
/// (`SocketErrorPal`), with `EAGAIN` mapped to `WSAETIMEDOUT` (on blocking sockets it
/// only comes from an expired `SO_RCVTIMEO` / `SO_SNDTIMEO`). Windows: the Winsock
/// code itself (`WSAEWOULDBLOCK` → `WSAETIMEDOUT` likewise; 995 / 997 are the
/// overlapped-I/O pair). `None` = no socket-level equivalent (callers fall back to
/// `io::ErrorKind`).
pub fn winsock_code(raw: i32) -> Option<i32> {
    winsock_code_impl(raw)
}

/// The OS's own "connection timed out" error (`ETIMEDOUT` / `WSAETIMEDOUT`), for its
/// message text. `None` where the target has no such OS error.
pub fn timed_out_error() -> Option<io::Error> {
    timed_out_raw().map(io::Error::from_raw_os_error)
}

/// Wait until `listener` is readable (a connection is queued) or `timeout_ms` passes.
/// `Ok(true)` = ready, `Ok(false)` = timed out (an interrupted wait counts as a
/// timeout; the caller loops). Borrowing `listener` keeps its descriptor alive for
/// the whole wait. Pure std data — safe to call while parked.
pub fn wait_readable(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    wait_readable_impl(listener, timeout_ms)
}

/// Enable TCP keep-alive on `stream`: first probe after `idle`, then every `interval`,
/// giving up after `retries` unanswered probes. Each knob is applied where the OS
/// exposes it (`interval` / `retries` are silently skipped elsewhere).
#[cfg(feature = "net")]
pub fn set_tcp_keepalive(
    stream: &std::net::TcpStream,
    idle: std::time::Duration,
    interval: std::time::Duration,
    retries: u32,
) -> io::Result<()> {
    let ka = socket2::TcpKeepalive::new().with_time(idle);
    #[cfg(any(
        target_os = "linux", target_os = "android", target_os = "macos", target_os = "ios",
        target_os = "tvos", target_os = "watchos", target_os = "freebsd", target_os = "dragonfly",
        target_os = "netbsd", target_os = "illumos", target_os = "solaris", target_os = "aix",
        target_os = "windows",
    ))]
    let ka = ka.with_interval(interval);
    #[cfg(any(
        target_os = "android", target_os = "dragonfly", target_os = "freebsd", target_os = "fuchsia",
        target_os = "illumos", target_os = "ios", target_os = "visionos", target_os = "linux",
        target_os = "macos", target_os = "netbsd", target_os = "tvos", target_os = "watchos",
        target_os = "cygwin",
    ))]
    let ka = ka.with_retries(retries);
    let _ = (interval, retries);
    socket2::SockRef::from(stream).set_tcp_keepalive(&ka)
}

/// Winsock codes the unix table maps to by name (`System.Net.Sockets.SocketError`).
#[cfg(unix)]
mod wsa {
    pub const EINTR: i32 = 10004;
    pub const EACCES: i32 = 10013;
    pub const EFAULT: i32 = 10014;
    pub const EINVAL: i32 = 10022;
    pub const EMFILE: i32 = 10024;
    pub const EINPROGRESS: i32 = 10036;
    pub const EALREADY: i32 = 10037;
    pub const ENOTSOCK: i32 = 10038;
    pub const EDESTADDRREQ: i32 = 10039;
    pub const EMSGSIZE: i32 = 10040;
    pub const EPROTOTYPE: i32 = 10041;
    pub const ENOPROTOOPT: i32 = 10042;
    pub const EPROTONOSUPPORT: i32 = 10043;
    pub const EOPNOTSUPP: i32 = 10045;
    pub const EPFNOSUPPORT: i32 = 10046;
    pub const EAFNOSUPPORT: i32 = 10047;
    pub const EADDRINUSE: i32 = 10048;
    pub const EADDRNOTAVAIL: i32 = 10049;
    pub const ENETDOWN: i32 = 10050;
    pub const ENETUNREACH: i32 = 10051;
    pub const ENETRESET: i32 = 10052;
    pub const ECONNABORTED: i32 = 10053;
    pub const ECONNRESET: i32 = 10054;
    pub const ENOBUFS: i32 = 10055;
    pub const EISCONN: i32 = 10056;
    pub const ENOTCONN: i32 = 10057;
    pub const ESHUTDOWN: i32 = 10058;
    pub const ETIMEDOUT: i32 = 10060;
    pub const ECONNREFUSED: i32 = 10061;
    pub const EHOSTDOWN: i32 = 10064;
    pub const EHOSTUNREACH: i32 = 10065;
}

// ── unix impls ───────────────────────────────────────────────────────────────

#[cfg(unix)]
fn winsock_code_impl(errno: i32) -> Option<i32> {
    use libc::*;
    Some(match errno {
        EINTR => wsa::EINTR,
        EACCES | EPERM => wsa::EACCES,
        EFAULT => wsa::EFAULT,
        EINVAL => wsa::EINVAL,
        EMFILE | ENFILE => wsa::EMFILE,
        EAGAIN => wsa::ETIMEDOUT,
        EINPROGRESS => wsa::EINPROGRESS,
        EALREADY => wsa::EALREADY,
        ENOTSOCK => wsa::ENOTSOCK,
        EDESTADDRREQ => wsa::EDESTADDRREQ,
        EMSGSIZE => wsa::EMSGSIZE,
        EPROTOTYPE => wsa::EPROTOTYPE,
        ENOPROTOOPT => wsa::ENOPROTOOPT,
        EPROTONOSUPPORT => wsa::EPROTONOSUPPORT,
        EOPNOTSUPP => wsa::EOPNOTSUPP,
        EPFNOSUPPORT => wsa::EPFNOSUPPORT,
        EAFNOSUPPORT => wsa::EAFNOSUPPORT,
        EADDRINUSE => wsa::EADDRINUSE,
        EADDRNOTAVAIL => wsa::EADDRNOTAVAIL,
        ENETDOWN => wsa::ENETDOWN,
        ENETUNREACH => wsa::ENETUNREACH,
        ENETRESET => wsa::ENETRESET,
        ECONNABORTED => wsa::ECONNABORTED,
        ECONNRESET => wsa::ECONNRESET,
        ENOBUFS => wsa::ENOBUFS,
        EISCONN => wsa::EISCONN,
        ENOTCONN => wsa::ENOTCONN,
        EPIPE | ESHUTDOWN => wsa::ESHUTDOWN,
        ETIMEDOUT => wsa::ETIMEDOUT,
        ECONNREFUSED => wsa::ECONNREFUSED,
        EHOSTDOWN => wsa::EHOSTDOWN,
        EHOSTUNREACH => wsa::EHOSTUNREACH,
        _ => return None,
    })
}

#[cfg(unix)]
fn timed_out_raw() -> Option<i32> {
    Some(libc::ETIMEDOUT)
}

#[cfg(unix)]
fn wait_readable_impl(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut pfd = libc::pollfd { fd: listener.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // SAFETY: one valid pollfd; the fd is kept alive by the `listener` borrow.
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    match rc {
        n if n > 0 => Ok(true),
        0 => Ok(false),
        _ => {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted { Ok(false) } else { Err(e) }
        }
    }
}

// ── windows impls ────────────────────────────────────────────────────────────

#[cfg(windows)]
fn winsock_code_impl(code: i32) -> Option<i32> {
    const WSAEWOULDBLOCK: i32 = 10035;
    const WSAETIMEDOUT: i32 = 10060;
    match code {
        WSAEWOULDBLOCK => Some(WSAETIMEDOUT),
        995 | 997 | 10004..=11004 => Some(code),
        _ => None,
    }
}

#[cfg(windows)]
fn timed_out_raw() -> Option<i32> {
    Some(10060) // WSAETIMEDOUT
}

#[cfg(windows)]
#[repr(C)]
struct WsaPollFd {
    fd: usize,
    events: i16,
    revents: i16,
}

#[cfg(windows)]
extern "system" {
    fn WSAPoll(fdarray: *mut WsaPollFd, fds: u32, timeout: i32) -> i32;
}

#[cfg(windows)]
fn wait_readable_impl(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    const POLLRDNORM: i16 = 0x0100;
    let mut pfd = WsaPollFd { fd: listener.as_raw_socket() as usize, events: POLLRDNORM, revents: 0 };
    // SAFETY: one valid WSAPOLLFD; the socket is kept alive by the `listener` borrow.
    let rc = unsafe { WSAPoll(&mut pfd, 1, timeout_ms) };
    match rc {
        n if n > 0 => Ok(true),
        0 => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

// ── other targets (wasm32 …): no OS sockets ──────────────────────────────────

#[cfg(not(any(unix, windows)))]
fn winsock_code_impl(_: i32) -> Option<i32> {
    None
}

#[cfg(not(any(unix, windows)))]
fn timed_out_raw() -> Option<i32> {
    None
}

#[cfg(not(any(unix, windows)))]
fn wait_readable_impl(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    let _ = (listener, timeout_ms);
    Err(io::Error::new(io::ErrorKind::Unsupported, "socket readiness wait: not supported on this platform"))
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod net_tests;
