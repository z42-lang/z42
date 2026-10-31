use std::io;
use std::net::TcpListener;

#[repr(C)]
struct WsaPollFd {
    fd: usize,
    events: i16,
    revents: i16,
}
extern "system" {
    fn WSAPoll(fdarray: *mut WsaPollFd, fds: u32, timeout: i32) -> i32;
}

#[cfg(feature = "net")]
pub(crate) use super::super::common::net::set_tcp_keepalive;

pub(crate) fn winsock_code(code: i32) -> Option<i32> {
    const WSAEWOULDBLOCK: i32 = 10035;
    const WSAETIMEDOUT: i32 = 10060;
    match code {
        WSAEWOULDBLOCK => Some(WSAETIMEDOUT),
        995 | 997 | 10004..=11004 => Some(code),
        _ => None,
    }
}

pub(crate) fn timed_out_raw() -> Option<i32> {
    Some(10060) // WSAETIMEDOUT
}

pub(crate) fn wait_readable(listener: &TcpListener, timeout_ms: i32) -> io::Result<bool> {
    use std::os::windows::io::AsRawSocket;
    const POLLRDNORM: i16 = 0x0100;
    let mut pfd = WsaPollFd {
        fd: listener.as_raw_socket() as usize,
        events: POLLRDNORM,
        revents: 0,
    };
    // SAFETY: one valid WSAPOLLFD; the socket is kept alive by the `listener` borrow.
    let rc = unsafe { WSAPoll(&mut pfd, 1, timeout_ms) };
    match rc {
        n if n > 0 => Ok(true),
        0 => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}
