//! ABI v2 TCP / UDP / DNS builtins (`network/tcp.rs`, `network/udp.rs`): the .NET exception
//! mapping, and parking — a collection completes while a thread is blocked in accept /
//! read / receive. Loopback only.

use super::error::code;
use crate::corelib::abi::BuiltinError;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string().into())
}

/// Call a builtin by name; the error comes back as the typed `BuiltinError`.
pub(crate) fn call(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Option<Value>, BuiltinError> {
    crate::corelib::exec_builtin(ctx, name, args).map_err(|e| match e.downcast::<BuiltinError>() {
        Ok(b) => b,
        Err(e) => panic!("`{name}` failed with a non-typed error: {e}"),
    })
}

fn val(ctx: &VmContext, name: &str, args: &[Value]) -> Value {
    match call(ctx, name, args) {
        Ok(Some(v)) => v,
        other => panic!("`{name}`: expected a value, got {other:?}"),
    }
}

fn int(ctx: &VmContext, name: &str, args: &[Value]) -> i64 {
    match val(ctx, name, args) {
        Value::I64(n) => n,
        other => panic!("`{name}`: expected an int, got {other:?}"),
    }
}

/// The class a `Detailed` error is thrown as (first choice) and its `SocketErrorCode`.
pub(crate) fn detailed(e: &BuiltinError) -> (&'static str, Option<i64>) {
    match e {
        BuiltinError::Detailed(d) => (d.classes[0], d.fields.iter().find_map(|(n, v)| match (n, v) {
            (&"SocketErrorCode", crate::corelib::abi::FieldInit::I64(c)) => Some(*c),
            _ => None,
        })),
        other => panic!("expected a Detailed exception, got {other:?}"),
    }
}

fn err(ctx: &VmContext, name: &str, args: &[Value]) -> BuiltinError {
    match call(ctx, name, args) {
        Err(e) => e,
        Ok(v) => panic!("`{name}`: expected a failure, got {v:?}"),
    }
}

fn bytes(ctx: &VmContext, n: usize) -> Value {
    ctx.heap().alloc_bytes(vec![0u8; n])
}

fn parked(ctx: &VmContext) -> usize {
    ctx.core.parked_count.load(std::sync::atomic::Ordering::Acquire) as usize
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(5) {
        if cond() { return true; }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

/// With one other thread parked, this thread becomes the collector and a full
/// collection finishes promptly (it does not wait for the parked thread).
pub(crate) fn collect_while_parked(ctx: &VmContext) {
    assert!(wait_until(|| parked(ctx) == 1), "the blocked thread is parked");
    let t0 = Instant::now();
    {
        let pause = crate::gc::safepoint::request_gc_pause(ctx);
        assert!(pause.is_some(), "this thread becomes the collector");
        ctx.heap().collect_full();
    }
    assert!(t0.elapsed() < Duration::from_secs(1), "the collection waited for the blocked thread ({:?})", t0.elapsed());
}

/// `(listener slot, port)` on 127.0.0.1.
fn listen(ctx: &VmContext) -> (i64, i64) {
    let l = int(ctx, "__tcp_listen", &[s("127.0.0.1"), Value::I64(0), Value::Bool(false)]);
    (l, int(ctx, "__tcp_listener_port", &[Value::I64(l)]))
}

/// A connected pair `(client, server)`.
fn pair(ctx: &VmContext) -> (i64, i64, i64) {
    let (l, port) = listen(ctx);
    let c = int(ctx, "__tcp_connect", &[s("127.0.0.1"), Value::I64(port), Value::I64(0)]);
    let a = int(ctx, "__tcp_accept", &[Value::I64(l)]);
    (l, c, a)
}

#[test]
fn connect_refused_is_a_socket_exception_with_the_endpoint() {
    let ctx = ctx();
    let (l, port) = listen(&ctx);
    call(&ctx, "__tcp_listener_close", &[Value::I64(l)]).unwrap();
    let e = err(&ctx, "__tcp_connect", &[s("127.0.0.1"), Value::I64(port), Value::I64(0)]);
    assert_eq!(detailed(&e), ("Std.Net.Sockets.SocketException", Some(code::CONNECTION_REFUSED)), "{e}");
    assert!(e.message().ends_with(&format!(" 127.0.0.1:{port}")), "{}", e.message());
    assert!(!e.message().contains("os error"), "{}", e.message());
}

#[test]
fn bad_port_and_ttl_are_argument_out_of_range() {
    let ctx = ctx();
    let e = err(&ctx, "__tcp_connect", &[s("127.0.0.1"), Value::I64(70_000), Value::I64(0)]);
    assert!(matches!(e, BuiltinError::ArgumentOutOfRange(ref m) if m.contains("'port'")), "{e}");
    let (_l, c, _a) = pair(&ctx);
    let e = err(&ctx, "__tcp_set_ttl", &[Value::I64(c), Value::I64(256)]);
    assert!(matches!(e, BuiltinError::ArgumentOutOfRange(_)), "{e}");
    let e = err(&ctx, "__tcp_set_keepalive_tuned",
        &[Value::I64(c), Value::Bool(true), Value::I64(0), Value::I64(1), Value::I64(1)]);
    assert!(matches!(e, BuiltinError::ArgumentOutOfRange(ref m) if m.contains("idleSecs")), "{e}");
}

#[test]
fn an_unknown_handle_is_socket_closed() {
    let ctx = ctx();
    let buf = bytes(&ctx, 4);
    for (name, args) in [
        ("__tcp_read", vec![Value::I64(999_999), buf, Value::I64(0), Value::I64(4)]),
        ("__tcp_write", vec![Value::I64(-1), buf, Value::I64(0), Value::I64(4)]),
        ("__tcp_accept", vec![Value::I64(999_999)]),
        ("__tcp_set_nodelay", vec![Value::I64(999_999), Value::Bool(true)]),
        ("__udp_local_port", vec![Value::I64(999_999)]),
    ] {
        let e = err(&ctx, name, &args);
        assert_eq!(detailed(&e).0, "Std.Net.Sockets.SocketClosedException", "{name}: {e}");
        assert!(e.classes().contains(&"Std.ObjectDisposedException"), "{name}");
    }
    // Closing what is not open is a no-op.
    call(&ctx, "__tcp_close", &[Value::I64(999_999)]).unwrap();
    call(&ctx, "__udp_close", &[Value::I64(-3)]).unwrap();
}

#[test]
fn bind_in_use_is_address_already_in_use() {
    let ctx = ctx();
    let (_l, port) = listen(&ctx);
    let e = err(&ctx, "__tcp_listen", &[s("127.0.0.1"), Value::I64(port), Value::Bool(false)]);
    assert_eq!(detailed(&e), ("Std.Net.Sockets.SocketException", Some(code::ADDRESS_ALREADY_IN_USE)), "{e}");
}

#[test]
fn dns_failure_is_host_not_found_and_numeric_hosts_resolve_offline() {
    let ctx = ctx();
    // `.invalid` never resolves (RFC 2606) — unless a local proxy answers every name
    // (fake-IP DNS); then there is no failure to look at.
    match call(&ctx, "__dns_lookup", &[s("no-such-host.invalid")]) {
        Err(e) => {
            let (class, code) = detailed(&e);
            assert_eq!(class, "Std.Net.Sockets.SocketException");
            assert!((code::HOST_NOT_FOUND..=11004).contains(&code.unwrap()), "{e}");
            assert!(!e.message().starts_with("failed to lookup"), "{}", e.message());
        }
        Ok(_) => eprintln!("skipped: this host's resolver answers `.invalid` names"),
    }
    let e = super::error::host_not_found("h", Some(&std::io::Error::other(
        "failed to lookup address information: nodename nor servname provided, or not known")));
    assert_eq!(detailed(&e), ("Std.Net.Sockets.SocketException", Some(code::HOST_NOT_FOUND)));
    assert_eq!(e.message(), "nodename nor servname provided, or not known");
    match val(&ctx, "__dns_lookup", &[s("127.0.0.1")]) {
        Value::Array(a) => {
            let a = a.borrow();
            assert_eq!(a.len(), 1);
            assert!(matches!(a.get_boxed(0), Value::Str(t) if t.as_str() == "127.0.0.1"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn loopback_round_trip_and_read_timeout() {
    let ctx = ctx();
    let (_l, c, a) = pair(&ctx);
    let out = ctx.heap().alloc_bytes(b"hello".to_vec());
    assert!(call(&ctx, "__tcp_write", &[Value::I64(c), out, Value::I64(0), Value::I64(5)]).unwrap().is_none());
    let buf = bytes(&ctx, 16);
    let n = int(&ctx, "__tcp_read", &[Value::I64(a), buf, Value::I64(2), Value::I64(10)]);
    assert_eq!(n, 5);
    if let Value::Array(arr) = buf {
        assert_eq!(arr.borrow().byte_range(2, 5).unwrap(), b"hello");
    }
    // Out-of-range buffer window: .NET's argument check, before any I/O.
    let e = err(&ctx, "__tcp_read", &[Value::I64(a), buf, Value::I64(10), Value::I64(10)]);
    assert!(matches!(e, BuiltinError::Argument(_)), "{e}");
    // An expired SO_RCVTIMEO: SocketException(TimedOut), with the timeout's own text.
    call(&ctx, "__tcp_set_read_timeout", &[Value::I64(a), Value::I64(50)]).unwrap();
    let e = err(&ctx, "__tcp_read", &[Value::I64(a), buf, Value::I64(0), Value::I64(4)]);
    assert_eq!(detailed(&e), ("Std.Net.Sockets.SocketException", Some(code::TIMED_OUT)), "{e}");
    assert!(!e.message().contains("temporarily unavailable"), "{}", e.message());
    // The peer closing reads as 0.
    call(&ctx, "__tcp_set_read_timeout", &[Value::I64(a), Value::I64(0)]).unwrap();
    call(&ctx, "__tcp_close", &[Value::I64(c)]).unwrap();
    assert_eq!(int(&ctx, "__tcp_read", &[Value::I64(a), buf, Value::I64(0), Value::I64(4)]), 0);
}

#[test]
fn a_collection_completes_while_a_thread_is_blocked_in_accept_and_closing_wakes_it() {
    let ctx = ctx();
    let (l, _port) = listen(&ctx);
    let core = Arc::clone(&ctx.core);
    let t = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        call(&w, "__tcp_accept", &[Value::I64(l)]).err().map(|e| detailed(&e).0)
    });
    collect_while_parked(&ctx);
    call(&ctx, "__tcp_listener_close", &[Value::I64(l)]).unwrap();
    assert_eq!(t.join().unwrap(), Some("Std.Net.Sockets.SocketClosedException"));
    assert_eq!(parked(&ctx), 0);
}

#[test]
fn a_collection_completes_while_a_thread_is_blocked_in_read_and_closing_wakes_it() {
    let ctx = ctx();
    let (_l, c, a) = pair(&ctx);
    let core = Arc::clone(&ctx.core);
    let t = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        // In real code the buffer is in the caller's registers (a root); here, pin it.
        let buf = w.heap().alloc_bytes(vec![0u8; 8]);
        let root = w.heap().pin_root(buf);
        let r = call(&w, "__tcp_read", &[Value::I64(a), buf, Value::I64(0), Value::I64(8)]).err().map(|e| detailed(&e).0);
        w.heap().unpin_root(root);
        r
    });
    collect_while_parked(&ctx);
    // Full duplex: the other direction still works while the read blocks.
    let out = ctx.heap().alloc_bytes(vec![1, 2, 3]);
    call(&ctx, "__tcp_write", &[Value::I64(a), out, Value::I64(0), Value::I64(3)]).unwrap();
    let buf = bytes(&ctx, 3);
    assert_eq!(int(&ctx, "__tcp_read", &[Value::I64(c), buf, Value::I64(0), Value::I64(3)]), 3);
    // Closing wakes the blocked read on Unix (`shutdown`); Windows' shutdown does not wake
    // a blocked `recv`, so there the peer's close ends it. Either way the read reports the
    // handle as closed.
    call(&ctx, "__tcp_close", &[Value::I64(a)]).unwrap();
    #[cfg(not(unix))]
    call(&ctx, "__tcp_close", &[Value::I64(c)]).unwrap();
    assert_eq!(t.join().unwrap(), Some("Std.Net.Sockets.SocketClosedException"));
    assert_eq!(parked(&ctx), 0);
}

#[test]
fn a_collection_completes_while_a_thread_is_blocked_in_connect() {
    // TEST-NET-1 (RFC 5737) is never routed: the SYN goes unanswered until the timeout —
    // unless this host has no route at all, which fails at once, or a local proxy accepts
    // every connection (then there is nothing to observe). The TLS tests cover the same
    // parked connect deterministically (a silent loopback peer).
    let ctx = ctx();
    let core = Arc::clone(&ctx.core);
    let t = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        call(&w, "__tcp_connect", &[s("192.0.2.1"), Value::I64(9), Value::I64(1500)]).err().map(|e| detailed(&e))
    });
    let t0 = Instant::now();
    while parked(&ctx) == 0 && !t.is_finished() && t0.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(2));
    }
    if parked(&ctx) == 1 {
        collect_while_parked(&ctx);
    }
    match t.join().unwrap() {
        Some((class, _code)) => assert_eq!(class, "Std.Net.Sockets.SocketException"),
        None => eprintln!("skipped: a local proxy accepted the connection to TEST-NET-1"),
    }
    assert_eq!(parked(&ctx), 0);
}

#[test]
fn udp_round_trip_timeout_and_a_collection_while_blocked_in_receive() {
    let ctx = ctx();
    let rx = int(&ctx, "__udp_bind", &[s("127.0.0.1"), Value::I64(0)]);
    let port = int(&ctx, "__udp_local_port", &[Value::I64(rx)]);
    let tx = int(&ctx, "__udp_bind", &[s("127.0.0.1"), Value::I64(0)]);
    let out = ctx.heap().alloc_bytes(b"ping".to_vec());
    let _out_root = ctx.heap().pin_root(out);   // used again after the collection below
    let sent = int(&ctx, "__udp_send", &[Value::I64(tx), out, Value::I64(0), Value::I64(4), s("127.0.0.1"), Value::I64(port)]);
    assert_eq!(sent, 4);
    match val(&ctx, "__udp_recv", &[Value::I64(rx)]) {
        Value::Array(a) => {
            let a = a.borrow();
            match a.get_boxed(0) {
                Value::Array(b) => assert_eq!(b.borrow().byte_range(0, 4).unwrap(), b"ping"),
                other => panic!("{other:?}"),
            }
            assert!(matches!(a.get_boxed(1), Value::Str(h) if h.as_str() == "127.0.0.1"));
        }
        other => panic!("{other:?}"),
    }
    call(&ctx, "__udp_set_read_timeout", &[Value::I64(rx), Value::I64(50)]).unwrap();
    let e = err(&ctx, "__udp_recv", &[Value::I64(rx)]);
    assert_eq!(detailed(&e), ("Std.Net.Sockets.SocketException", Some(code::TIMED_OUT)), "{e}");
    call(&ctx, "__udp_set_read_timeout", &[Value::I64(rx), Value::I64(0)]).unwrap();

    let core = Arc::clone(&ctx.core);
    let t = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        let buf = w.heap().alloc_bytes(vec![0u8; 2]);
        let root = w.heap().pin_root(buf);
        let ok = match call(&w, "__udp_recv_into", &[Value::I64(rx), buf, Value::I64(0), Value::I64(2)]) {
            Ok(Some(Value::Array(a))) => matches!(a.borrow().get_boxed(0), Value::I64(2)),
            other => panic!("{other:?}"),
        };
        w.heap().unpin_root(root);
        ok
    });
    collect_while_parked(&ctx);
    // A longer datagram is truncated to the buffer window.
    call(&ctx, "__udp_send", &[Value::I64(tx), out, Value::I64(0), Value::I64(4), s("127.0.0.1"), Value::I64(port)]).unwrap();
    assert!(t.join().unwrap(), "recv_into reports the truncated length");
    assert_eq!(parked(&ctx), 0);
}

#[test]
fn multicast_arguments_follow_dotnet() {
    let ctx = ctx();
    let u = int(&ctx, "__udp_bind", &[s("0.0.0.0"), Value::I64(0)]);
    let e = err(&ctx, "__udp_join_multicast", &[Value::I64(u), s("not-an-ip"), s("0.0.0.0")]);
    assert!(matches!(e, BuiltinError::Format(_)), "{e}");
    let e = err(&ctx, "__udp_join_multicast", &[Value::I64(u), s("10.0.0.1"), s("0.0.0.0")]);
    assert_eq!(detailed(&e), ("Std.Net.Sockets.SocketException", Some(code::INVALID_ARGUMENT)), "{e}");
}

#[test]
fn the_unsupported_platform_error_falls_back_through_dotnet_classes() {
    let e = super::error::unsupported();
    assert_eq!(e.classes(), ["Std.Net.NetUnsupportedException", "Std.PlatformNotSupportedException", "Std.NotSupportedException"]);
}

#[test]
fn os_errors_map_to_dotnet_socket_error_codes() {
    use super::error::socket_error_code;
    use std::io::{Error, ErrorKind};
    assert_eq!(socket_error_code(&Error::from(ErrorKind::ConnectionRefused)), code::CONNECTION_REFUSED);
    assert_eq!(socket_error_code(&Error::from(ErrorKind::WouldBlock)), code::TIMED_OUT);
    assert_eq!(socket_error_code(&Error::from(ErrorKind::BrokenPipe)), code::SHUTDOWN);
    assert_eq!(socket_error_code(&Error::other("x")), code::SOCKET_ERROR);
    #[cfg(unix)]
    {
        assert_eq!(socket_error_code(&Error::from_raw_os_error(libc::ECONNRESET)), code::CONNECTION_RESET);
        assert_eq!(socket_error_code(&Error::from_raw_os_error(libc::EAGAIN)), code::TIMED_OUT);
        assert_eq!(socket_error_code(&Error::from_raw_os_error(libc::EHOSTUNREACH)), code::HOST_UNREACHABLE);
    }
    #[cfg(windows)]
    assert_eq!(socket_error_code(&Error::from_raw_os_error(10054)), code::CONNECTION_RESET);
}
