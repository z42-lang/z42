use std::io;
use std::net::TcpListener;

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

#[cfg(feature = "net")]
pub(crate) use super::super::common::net::set_tcp_keepalive;

pub(crate) fn winsock_code(errno: i32) -> Option<i32> {
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

pub(crate) fn timed_out_raw() -> Option<i32> {
    Some(libc::ETIMEDOUT)
}

pub(crate) fn wait_readable(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    let mut pfd = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd; the fd is kept alive by the `listener` borrow.
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    match rc {
        n if n > 0 => Ok(true),
        0 => Ok(false),
        _ => {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                Ok(false)
            } else {
                Err(e)
            }
        }
    }
}
