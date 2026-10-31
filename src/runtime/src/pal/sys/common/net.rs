use std::io;

#[cfg(feature = "net")]
pub(crate) fn set_tcp_keepalive(
    stream: &std::net::TcpStream,
    idle: std::time::Duration,
    interval: std::time::Duration,
    retries: u32,
) -> io::Result<()> {
    let ka = socket2::TcpKeepalive::new().with_time(idle);
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "illumos",
        target_os = "solaris",
        target_os = "aix",
        target_os = "windows",
    ))]
    let ka = ka.with_interval(interval);
    #[cfg(any(
        target_os = "android",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "fuchsia",
        target_os = "illumos",
        target_os = "ios",
        target_os = "visionos",
        target_os = "linux",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "cygwin",
    ))]
    let ka = ka.with_retries(retries);
    let _ = (interval, retries);
    socket2::SockRef::from(stream).set_tcp_keepalive(&ka)
}
