use std::io;
use std::net::TcpListener;

#[cfg(feature = "net")]
pub(crate) use super::super::common::net::set_tcp_keepalive;

pub(crate) fn winsock_code(_: i32) -> Option<i32> {
    None
}

pub(crate) fn timed_out_raw() -> Option<i32> {
    None
}

pub(crate) fn wait_readable(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    let _ = (listener, timeout_ms);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "socket readiness wait: not supported on this platform",
    ))
}
