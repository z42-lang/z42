#![cfg_attr(not(feature = "net"), allow(dead_code))]
//! .NET-shaped socket failures for the ABI v2 network builtins (`network/tcp.rs`,
//! `network/udp.rs`, `tls.rs`, the `network/unsupported.rs` stubs).
//!
//! | failure | exception (fallbacks when the class is not loaded) |
//! |---|---|
//! | an OS socket error (connect refused / reset / bind in use / unreachable / …) | `Std.Net.Sockets.SocketException` (`SocketErrorCode` / `ErrorCode` = the Winsock code [`socket_error_code`] maps the OS error to; message = the OS text, plus the endpoint for connect, as .NET 6+) |
//! | a socket read / write / connect timeout (`SO_RCVTIMEO` expiry, `connect_timeout`) | `SocketException`, `SocketErrorCode == TimedOut` — what .NET's `Socket.Receive` / `Connect` throw; only higher layers (HttpClient) turn it into a timeout exception |
//! | host name resolution failed | `SocketException`, `HostNotFound` (11001) — the resolver's text (`gai_strerror` on Unix, `No such host is known.` on Windows) |
//! | a closed / disposed / unknown socket handle, or closed while blocked on it | `Std.Net.Sockets.SocketClosedException : Std.ObjectDisposedException : InvalidOperationException` (`ObjectName` = `Std.Net.Sockets.Socket`) |
//! | port / TTL / keep-alive argument out of range | `ArgumentOutOfRangeException` |
//! | the build has no sockets (`net` / `tls` feature off, e.g. wasm32) | `Std.Net.NetUnsupportedException : Std.PlatformNotSupportedException : NotSupportedException` |

use crate::corelib::abi::{BuiltinError, FieldInit};

/// `SocketException`, then fallbacks (the builtins are only reachable through z42.net,
/// which declares the first one).
pub(crate) const SOCKET_EXCEPTION: &[&str] =
    &["Std.Net.Sockets.SocketException", "Std.Net.NetException", "Std.IO.IOException"];
const SOCKET_CLOSED: &[&str] = &[
    "Std.Net.Sockets.SocketClosedException", "Std.ObjectDisposedException", "Std.InvalidOperationException",
];
#[cfg(any(not(feature = "tls"), test))]
const NET_UNSUPPORTED: &[&str] = &[
    "Std.Net.NetUnsupportedException", "Std.PlatformNotSupportedException", "Std.NotSupportedException",
];

/// `System.Net.Sockets.SocketError` values used by name here (the z42 enum
/// `Std.Net.Sockets.SocketError` has them all).
pub(crate) mod code {
    pub const SOCKET_ERROR: i64 = -1;
    pub const INTERRUPTED: i64 = 10004;
    pub const ACCESS_DENIED: i64 = 10013;
    pub const INVALID_ARGUMENT: i64 = 10022;
    pub const OPERATION_NOT_SUPPORTED: i64 = 10045;
    pub const ADDRESS_ALREADY_IN_USE: i64 = 10048;
    pub const ADDRESS_NOT_AVAILABLE: i64 = 10049;
    pub const NETWORK_DOWN: i64 = 10050;
    pub const NETWORK_UNREACHABLE: i64 = 10051;
    pub const CONNECTION_ABORTED: i64 = 10053;
    pub const CONNECTION_RESET: i64 = 10054;
    pub const NOT_CONNECTED: i64 = 10057;
    pub const SHUTDOWN: i64 = 10058;
    pub const TIMED_OUT: i64 = 10060;
    pub const CONNECTION_REFUSED: i64 = 10061;
    pub const HOST_UNREACHABLE: i64 = 10065;
    pub const HOST_NOT_FOUND: i64 = 11001;
}

/// The `SocketError` code of an OS error — the Winsock code the OS error maps to
/// (`pal::net::winsock_code`: .NET's Unix errno table, the code itself on Windows),
/// else by `io::ErrorKind`.
pub(crate) fn socket_error_code(e: &std::io::Error) -> i64 {
    e.raw_os_error().and_then(from_os).unwrap_or_else(|| from_kind(e.kind()))
}

fn from_os(raw: i32) -> Option<i64> {
    crate::pal::net::winsock_code(raw).map(i64::from)
}

fn from_kind(kind: std::io::ErrorKind) -> i64 {
    use std::io::ErrorKind::*;
    match kind {
        TimedOut | WouldBlock => code::TIMED_OUT,
        ConnectionRefused => code::CONNECTION_REFUSED,
        ConnectionReset => code::CONNECTION_RESET,
        ConnectionAborted => code::CONNECTION_ABORTED,
        NotConnected => code::NOT_CONNECTED,
        AddrInUse => code::ADDRESS_ALREADY_IN_USE,
        AddrNotAvailable => code::ADDRESS_NOT_AVAILABLE,
        BrokenPipe => code::SHUTDOWN,
        PermissionDenied => code::ACCESS_DENIED,
        InvalidInput => code::INVALID_ARGUMENT,
        Interrupted => code::INTERRUPTED,
        HostUnreachable => code::HOST_UNREACHABLE,
        NetworkUnreachable => code::NETWORK_UNREACHABLE,
        NetworkDown => code::NETWORK_DOWN,
        Unsupported => code::OPERATION_NOT_SUPPORTED,
        _ => code::SOCKET_ERROR,
    }
}

/// The OS text of `e` without Rust's ` (os error N)` suffix — what .NET puts in `Message`.
pub(crate) fn describe(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.rfind(" (os error ") {
        Some(i) if s.ends_with(')') => s[..i].to_string(),
        _ => s,
    }
}

/// The text of a timeout: the OS's own `ETIMEDOUT` / `WSAETIMEDOUT` message (an expired
/// `SO_RCVTIMEO` surfaces as `EAGAIN` — "Resource temporarily unavailable" — which is not
/// what happened).
pub(crate) fn timeout_message() -> String {
    crate::pal::net::timed_out_error()
        .map(|e| describe(&e))
        .unwrap_or_else(|| "Connection timed out".to_string())
}

/// A `SocketException` with `code` / `message`.
pub(crate) fn socket_exception(code: i64, message: String) -> BuiltinError {
    BuiltinError::detailed(SOCKET_EXCEPTION, message, vec![
        ("SocketErrorCode", FieldInit::I64(code)),
        ("ErrorCode", FieldInit::I64(code)),
    ])
}

/// `e` as a `SocketException`; `endpoint` (connect) is appended to the message, as .NET does.
pub(crate) fn socket_error(e: &std::io::Error, endpoint: Option<&std::net::SocketAddr>) -> BuiltinError {
    let code = socket_error_code(e);
    let mut msg = if code == code::TIMED_OUT && e.kind() == std::io::ErrorKind::WouldBlock {
        timeout_message()
    } else {
        describe(e)
    };
    if let Some(ep) = endpoint {
        msg.push(' ');
        msg.push_str(&ep.to_string());
    }
    socket_exception(code, msg)
}

/// Name resolution failed: `SocketException(HostNotFound)` (`TryAgain` / `NoRecovery` /
/// `NoData` when the OS says so — Windows), message = the resolver's text.
pub(crate) fn host_not_found(host: &str, e: Option<&std::io::Error>) -> BuiltinError {
    let Some(e) = e else {
        return socket_exception(code::HOST_NOT_FOUND, format!("No such host is known: '{host}'."));
    };
    let code = e.raw_os_error().and_then(from_os)
        .filter(|c| (code::HOST_NOT_FOUND..=11004).contains(c))
        .unwrap_or(code::HOST_NOT_FOUND);
    let msg = describe(e);
    let msg = msg.strip_prefix("failed to lookup address information: ").map(str::to_string).unwrap_or(msg);
    socket_exception(code, msg)
}

/// A closed / disposed / unknown socket handle.
pub(crate) fn closed() -> BuiltinError {
    BuiltinError::detailed(
        SOCKET_CLOSED,
        "Cannot access a disposed object.\nObject name: 'Std.Net.Sockets.Socket'.",
        vec![("ObjectName", FieldInit::Str("Std.Net.Sockets.Socket".into()))],
    )
}

/// No sockets in this build (the `net` / `tls` feature is off, e.g. wasm32).
#[cfg(any(not(feature = "tls"), test))]
pub(crate) fn unsupported() -> BuiltinError {
    BuiltinError::detailed(NET_UNSUPPORTED, "network sockets are not supported on this target (e.g. wasm32)", vec![])
}

/// A handle argument as a registry id (a negative id is a handle that never existed).
pub(crate) fn slot_id(slot: i64) -> Result<u64, BuiltinError> {
    u64::try_from(slot).map_err(|_| closed())
}

/// `ArgumentOutOfRangeException` for `param` (.NET's text).
pub(crate) fn out_of_range(param: &str) -> BuiltinError {
    BuiltinError::ArgumentOutOfRange(format!(
        "Specified argument was out of the range of valid values. (Parameter '{param}')"))
}

/// A port argument (0 = let the OS pick, for bind).
pub(crate) fn check_port(port: i64) -> Result<u16, BuiltinError> {
    u16::try_from(port).map_err(|_| out_of_range("port"))
}

/// An IP TTL argument (0..=255, as .NET's `Socket.Ttl`).
pub(crate) fn check_ttl(ttl: i64) -> Result<u32, BuiltinError> {
    if (0..=255).contains(&ttl) { Ok(ttl as u32) } else { Err(out_of_range("ttl")) }
}

/// A timeout argument: `<= 0` = none (blocking).
pub(crate) fn timeout(millis: i64) -> Option<std::time::Duration> {
    (millis > 0).then(|| std::time::Duration::from_millis(millis as u64))
}

/// Resolve `host:port` (numeric addresses do not touch the network). Blocks for the
/// resolver: callers run it parked. No address → `HostNotFound`.
#[cfg(feature = "net")]
pub(crate) fn resolve(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>, BuiltinError> {
    use std::net::ToSocketAddrs;
    match (host, port).to_socket_addrs() {
        Ok(it) => {
            let addrs: Vec<_> = it.collect();
            if addrs.is_empty() { Err(host_not_found(host, None)) } else { Ok(addrs) }
        }
        Err(e) => Err(host_not_found(host, Some(&e))),
    }
}
