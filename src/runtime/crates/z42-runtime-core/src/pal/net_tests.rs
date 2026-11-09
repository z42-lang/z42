use super::*;
use std::time::Instant;

#[test]
fn unknown_os_errors_have_no_winsock_code() {
    assert_eq!(winsock_code(0), None);
    assert_eq!(winsock_code(-1), None);
}

#[cfg(unix)]
#[test]
fn unix_errnos_map_to_winsock_codes() {
    assert_eq!(winsock_code(libc::ECONNRESET), Some(10054));
    assert_eq!(winsock_code(libc::ECONNREFUSED), Some(10061));
    assert_eq!(winsock_code(libc::EHOSTUNREACH), Some(10065));
    assert_eq!(winsock_code(libc::EADDRINUSE), Some(10048));
    // Blocking sockets only see EAGAIN from an expired SO_RCVTIMEO / SO_SNDTIMEO.
    assert_eq!(winsock_code(libc::EAGAIN), Some(10060));
    assert_eq!(winsock_code(libc::EPIPE), winsock_code(libc::ESHUTDOWN));
    assert_eq!(winsock_code(libc::ENOENT), None);
}

#[cfg(windows)]
#[test]
fn winsock_codes_pass_through() {
    assert_eq!(winsock_code(10054), Some(10054));
    assert_eq!(winsock_code(11001), Some(11001));
    assert_eq!(winsock_code(10035), Some(10060)); // WSAEWOULDBLOCK → WSAETIMEDOUT
    assert_eq!(winsock_code(2), None);
}

#[test]
fn timed_out_error_is_the_os_timeout() {
    let e = timed_out_error().expect("desktop targets have an OS timeout error");
    assert_eq!(e.kind(), io::ErrorKind::TimedOut);
    assert_eq!(winsock_code(e.raw_os_error().unwrap()), Some(10060));
}

#[test]
fn wait_readable_times_out_then_sees_a_queued_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let start = Instant::now();
    assert!(!wait_readable(&listener, 20).unwrap(), "nothing queued: times out");
    assert!(start.elapsed().as_millis() >= 10);

    let _client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    assert!(wait_readable(&listener, 5_000).unwrap(), "a queued connection makes it readable");
    listener.accept().unwrap();
}

#[cfg(feature = "net")]
#[test]
fn set_tcp_keepalive_enables_keepalive() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let secs = std::time::Duration::from_secs;
    set_tcp_keepalive(&stream, secs(30), secs(5), 3).unwrap();
    assert!(socket2::SockRef::from(&stream).keepalive().unwrap());
}
