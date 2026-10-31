//! Socket platform operations exposed as std types.
//! OS error mapping, readiness waits and keepalive support live under `pal/sys/`.
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
    super::sys::net::winsock_code(raw)
}

/// The OS's own "connection timed out" error (`ETIMEDOUT` / `WSAETIMEDOUT`), for its
/// message text. `None` where the target has no such OS error.
pub fn timed_out_error() -> Option<io::Error> {
    super::sys::net::timed_out_raw().map(io::Error::from_raw_os_error)
}

/// Wait until `listener` is readable (a connection is queued) or `timeout_ms` passes.
/// `Ok(true)` = ready, `Ok(false)` = timed out (an interrupted wait counts as a
/// timeout; the caller loops). Borrowing `listener` keeps its descriptor alive for
/// the whole wait. Pure std data — safe to call while parked.
pub fn wait_readable(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    super::sys::net::wait_readable(listener, timeout_ms)
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
    super::sys::net::set_tcp_keepalive(stream, idle, interval, retries)
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod net_tests;
