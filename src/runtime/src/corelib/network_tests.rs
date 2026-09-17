use super::*;
use crate::vm_context::VmContext;
use crate::metadata::Value;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

fn arr(values: Vec<Value>, ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(values)
}

fn kind_of(v: &Value) -> Option<i64> {
    match v {
        Value::Array(rc) => match rc.borrow().first() {
            Some(Value::I64(k)) => Some(k),
            _ => None,
        },
        _ => None,
    }
}

fn ok_slot(v: &Value) -> i64 {
    match v {
        Value::Array(rc) => {
            let b = rc.borrow();
            assert_eq!(b.len(), 2, "ok-value tuple has 2 elements");
            match (&b.get_boxed(0), &b.get_boxed(1)) {
                (Value::I64(0), Value::I64(s)) => *s,
                _ => panic!("not an ok-slot tuple: {:?}", b),
            }
        }
        _ => panic!("not an Array: {:?}", v),
    }
}

fn ok_listen(v: &Value) -> (i64, i64) {
    match v {
        Value::Array(rc) => {
            let b = rc.borrow();
            assert_eq!(b.len(), 3, "ok-listen tuple has 3 elements");
            match (&b.get_boxed(0), &b.get_boxed(1), &b.get_boxed(2)) {
                (Value::I64(0), Value::I64(s), Value::I64(p)) => (*s, *p),
                _ => panic!("not an ok-listen tuple: {:?}", b),
            }
        }
        _ => panic!("not an Array: {:?}", v),
    }
}

// ── Slot allocator ──────────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn slot_id_monotonic_increasing() {
    let ctx = ctx();
    let args1 = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(0)];
    let r1 = builtin_net_tcp_listen(&ctx, &args1).expect("listen 1 ok");
    let r2 = builtin_net_tcp_listen(&ctx, &args1).expect("listen 2 ok");
    let (s1, _p1) = ok_listen(&r1);
    let (s2, _p2) = ok_listen(&r2);
    assert!(s2 > s1, "slot ids should be monotonic ({} → {})", s1, s2);
}

// ── Connect failures ────────────────────────────────────────────────────

// Note: a "connect to bad host" test would seem natural here but local DNS
// resolvers / ISP captive portals frequently hijack NXDOMAIN, and TEST-NET-1
// (192.0.2.0/24) is not reliably unroutable on all dev networks either —
// connect would succeed-via-hijack or hang-then-timeout, not return
// ConnectionRefused. The `connect_to_unbound_port_returns_socket_err` below
// is the reliable error-path test (localhost:1 = guaranteed-refused).

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn connect_to_unbound_port_returns_socket_err() {
    let ctx = ctx();
    let args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(1)];
    let r = builtin_net_tcp_connect(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_SOCKET_ERR), "got {:?}", r);
}

/// **fix-alloc-inside-native-park (2026-09-14)**: both connect builtins used to build their
/// result tuple while still inside their `NativeParkGuard`, so a collection on another
/// thread could free it before the caller read it (`Z42NetHttpServerThreadedTests`, 3 runs
/// in 40: `got Null`). In a debug build `gc::safepoint::debug_assert_not_native_parked`
/// turns that into a panic here — the refused-connect test above and these two cover every
/// return shape of both builtins.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn connect_success_builds_its_result_outside_the_park() {
    let ctx = ctx();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port() as i64;
    let args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(port)];
    let slot = ok_slot(&builtin_net_tcp_connect(&ctx, &args).expect("call ok"));
    let _ = builtin_net_tcp_socket_drop(&ctx, &[Value::I64(slot)]).expect("drop sock");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn connect_with_timeout_builds_both_results_outside_the_park() {
    let ctx = ctx();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port() as i64;
    let ok_args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(port), Value::I64(2000)];
    let slot = ok_slot(&builtin_net_tcp_connect_with_timeout(&ctx, &ok_args).expect("call ok"));
    let _ = builtin_net_tcp_socket_drop(&ctx, &[Value::I64(slot)]).expect("drop sock");

    let refused = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(1), Value::I64(2000)];
    let r = builtin_net_tcp_connect_with_timeout(&ctx, &refused).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_SOCKET_ERR), "got {:?}", r);
}

/// **fix-park-blocking-natives (2026-09-14)**: `__net_dns_lookup` now resolves parked, so
/// its string array must be built after the park ends (debug tripwire).
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn dns_lookup_builds_its_result_outside_the_park() {
    let ctx = ctx();
    let r = builtin_net_dns_lookup(&ctx, &[Value::Str("localhost".to_string().into())]).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_OK), "got {:?}", r);
}

// ── Slot lookups on unknown ids ─────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn read_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let buf = arr(vec![Value::I64(0); 16], &ctx);
    let args = vec![Value::I64(999_999), buf, Value::I64(0), Value::I64(16)];
    let r = builtin_net_tcp_socket_read(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn write_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let buf = arr(vec![Value::I64(b'x' as i64)], &ctx);
    let args = vec![Value::I64(999_999), buf, Value::I64(0), Value::I64(1)];
    let r = builtin_net_tcp_socket_write(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn accept_on_unknown_listener_returns_handle_invalid() {
    let ctx = ctx();
    let args = vec![Value::I64(999_999)];
    let r = builtin_net_tcp_accept(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn drop_unknown_slot_is_silent_null() {
    let ctx = ctx();
    let args = vec![Value::I64(999_999)];
    let r = builtin_net_tcp_socket_drop(&ctx, &args).expect("call ok");
    assert!(matches!(r, Value::Null));
    let r2 = builtin_net_tcp_listener_drop(&ctx, &args).expect("call ok");
    assert!(matches!(r2, Value::Null));
}

// ── End-to-end loopback ─────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn loopback_listener_accepts_and_round_trips_bytes() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let ctx = ctx();
    let listen_args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(0)];
    let listen_result = builtin_net_tcp_listen(&ctx, &listen_args).expect("listen ok");
    let (listener_slot, actual_port) = ok_listen(&listen_result);
    assert!(actual_port > 0, "OS should assign a real port");

    // Connect client in a separate thread (host side).
    let client_thread = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", actual_port as u16)).expect("connect");
        stream.write_all(b"hello").expect("write");
        let mut reply = [0u8; 5];
        stream.read_exact(&mut reply).expect("read reply");
        assert_eq!(&reply, b"world");
    });

    // Server side via builtins.
    let accept_args = vec![Value::I64(listener_slot)];
    let accept_result = builtin_net_tcp_accept(&ctx, &accept_args).expect("accept ok");
    let sock_slot = ok_slot(&accept_result);

    // Read "hello".
    let read_buf = arr(vec![Value::I64(0); 5], &ctx);
    let read_args = vec![Value::I64(sock_slot), read_buf.clone(), Value::I64(0), Value::I64(5)];
    let read_result = builtin_net_tcp_socket_read(&ctx, &read_args).expect("read ok");
    let nread = ok_slot(&read_result);
    assert_eq!(nread, 5, "should read 5 bytes");
    if let Value::Array(rc) = &read_buf {
        let b = rc.borrow();
        let bytes: Vec<u8> = b.iter_boxed().map(|v| match v {
            Value::I64(n) => n as u8,
            _ => panic!("non-i64 byte"),
        }).collect();
        assert_eq!(&bytes, b"hello");
    }

    // Write "world" back.
    let write_buf = arr(b"world".iter().map(|b| Value::I64(*b as i64)).collect(), &ctx);
    let write_args = vec![Value::I64(sock_slot), write_buf, Value::I64(0), Value::I64(5)];
    let write_result = builtin_net_tcp_socket_write(&ctx, &write_args).expect("write ok");
    assert_eq!(ok_slot(&write_result), 5);

    client_thread.join().expect("client thread");

    // Cleanup.
    let _ = builtin_net_tcp_socket_drop(&ctx, &[Value::I64(sock_slot)]).expect("drop sock");
    let _ = builtin_net_tcp_listener_drop(&ctx, &[Value::I64(listener_slot)]).expect("drop listener");
    assert_eq!(ctx.tcp_socket_slot_count(), 0);
    assert_eq!(ctx.tcp_listener_slot_count(), 0);
}

// ── wasm32 unsupported gating ───────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[test]
fn wasm32_returns_unsupported_tuple() {
    let ctx = ctx();
    let args = vec![Value::Str("127.0.0.1".to_string()), Value::I64(80)];
    let r = builtin_net_tcp_connect(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_UNSUPPORTED));
}

// ── UDP (add-z42-net-udp K2, 2026-05-25) ────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn ok_udp_bind(v: &Value) -> (i64, i64) {
    // Same shape as ok_listen: [0, slot, port]
    ok_listen(v)
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn udp_slot_id_monotonic() {
    let ctx = ctx();
    let args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(0)];
    let r1 = builtin_net_udp_bind(&ctx, &args).expect("bind 1");
    let r2 = builtin_net_udp_bind(&ctx, &args).expect("bind 2");
    let (s1, _) = ok_udp_bind(&r1);
    let (s2, _) = ok_udp_bind(&r2);
    assert!(s2 > s1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn udp_send_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let buf = arr(vec![Value::I64(0xAB)], &ctx);
    let args = vec![
        Value::I64(999_999), buf, Value::I64(0), Value::I64(1),
        Value::Str("127.0.0.1".to_string().into()), Value::I64(1),
    ];
    let r = builtin_net_udp_send(&ctx, &args).expect("call");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn udp_recv_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let args = vec![Value::I64(999_999)];
    let r = builtin_net_udp_recv(&ctx, &args).expect("call");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn udp_drop_unknown_slot_is_silent_null() {
    let ctx = ctx();
    let args = vec![Value::I64(999_999)];
    let r = builtin_net_udp_drop(&ctx, &args).expect("call");
    assert!(matches!(r, Value::Null));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn udp_loopback_send_recv_round_trip() {
    let ctx = ctx();
    // Bind two sockets on loopback with OS-assigned ports.
    let bind_args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(0)];
    let (slot_a, port_a) = ok_udp_bind(&builtin_net_udp_bind(&ctx, &bind_args).expect("bind A"));
    let (slot_b, port_b) = ok_udp_bind(&builtin_net_udp_bind(&ctx, &bind_args).expect("bind B"));
    assert!(port_a > 0 && port_b > 0);

    // B sends "hi" to A.
    let payload = arr(b"hi".iter().map(|b| Value::I64(*b as i64)).collect(), &ctx);
    let send_args = vec![
        Value::I64(slot_b), payload, Value::I64(0), Value::I64(2),
        Value::Str("127.0.0.1".to_string().into()), Value::I64(port_a),
    ];
    let send_result = builtin_net_udp_send(&ctx, &send_args).expect("send");
    assert_eq!(ok_slot(&send_result), 2);  // 2 bytes sent

    // A receives.
    let recv_result = builtin_net_udp_recv(&ctx, &[Value::I64(slot_a)]).expect("recv");
    match &recv_result {
        Value::Array(rc) => {
            let b = rc.borrow();
            assert_eq!(b.len(), 4, "recv ok tuple has 4 elements");
            assert_eq!(b.get_boxed(0), Value::I64(0));
            match &b.get_boxed(1) {
                Value::Array(buf) => {
                    let bb = buf.borrow();
                    assert_eq!(bb.len(), 2);
                    assert_eq!(bb.get_boxed(0), Value::I64(b'h' as i64));
                    assert_eq!(bb.get_boxed(1), Value::I64(b'i' as i64));
                }
                other => panic!("expected byte[] buffer, got {:?}", other),
            }
            assert!(matches!(&b.get_boxed(2), Value::Str(s) if **s == *"127.0.0.1"));
            assert_eq!(b.get_boxed(3), Value::I64(port_b));
        }
        other => panic!("expected ok-tuple Array, got {:?}", other),
    }

    // Cleanup.
    let _ = builtin_net_udp_drop(&ctx, &[Value::I64(slot_a)]).expect("drop A");
    let _ = builtin_net_udp_drop(&ctx, &[Value::I64(slot_b)]).expect("drop B");
    assert_eq!(ctx.udp_socket_slot_count(), 0);
}

// ── fix-accept-not-interruptible (2026-09-17) ────────────────────────────────

/// 阻塞在 `accept` 里的线程，必须能被另一个线程的 `listener_drop` 唤醒。
///
/// 修复前这条会挂死：`accept` 在阻塞前把 listener 从表里摘走，`listener_drop`
/// 因此什么也没关（macOS 上即便关了 fd 也唤不醒 accept）。`HttpServer` 只好靠一次性
/// 自连探针唤醒，探针一漏就永久死锁——实测整包测试挂过 44 小时。
///
/// 🔴 **worker 必须是 detached 线程 + 带超时的 channel**，不能用 `thread::scope`：
/// scope 退出时会 join worker，于是回退实现时**整个测试进程挂住**（CI 超时），
/// 而不是报一条失败。写这条测试时我先踩了这个坑。
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_blocked_accept_is_woken_by_dropping_the_listener() {
    use std::sync::mpsc;

    let ctx = ctx();
    let listen = builtin_net_tcp_listen(&ctx, &[Value::Str("127.0.0.1".into()), Value::I64(0)])
        .expect("listen");
    let (slot, _port) = ok_listen(&listen);

    let (tx, rx) = mpsc::channel();
    let core = std::sync::Arc::clone(&ctx.core);
    std::thread::spawn(move || {
        let worker_ctx = VmContext::new_with_core(core);
        let r = builtin_net_tcp_accept(&worker_ctx, &[Value::I64(slot)]).expect("accept");
        // 关掉的 listener ⇒ KIND_HANDLE_INVALID（z42 侧的 SocketClosedException）。
        let _ = tx.send(kind_of(&r));
    });

    // 让 worker 真正进到 accept 里再关。
    std::thread::sleep(std::time::Duration::from_millis(150));
    builtin_net_tcp_listener_drop(&ctx, &[Value::I64(slot)]).expect("drop");

    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(kind) => assert_eq!(kind, Some(2), "被关掉的 listener 应返回 KIND_HANDLE_INVALID"),
        Err(_) => panic!(
            "阻塞中的 accept 在 listener 关闭后 5 秒仍未返回 —— \
             accept 又变成不可中断了（本 PR 修的就是这个）"
        ),
    }
}

/// 正向对照：listener 还开着时，accept 照常收连接（别把 bug 修成「accept 直接不工作」）。
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn accept_still_returns_a_real_connection() {
    let ctx = ctx();
    let listen = builtin_net_tcp_listen(&ctx, &[Value::Str("127.0.0.1".into()), Value::I64(0)])
        .expect("listen");
    let (slot, port) = ok_listen(&listen);

    std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = std::net::TcpStream::connect(("127.0.0.1", port as u16)).expect("connect");
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        let r = builtin_net_tcp_accept(&ctx, &[Value::I64(slot)]).expect("accept");
        assert_eq!(kind_of(&r), Some(0), "应收到连接（KIND_OK）");
    });

    builtin_net_tcp_listener_drop(&ctx, &[Value::I64(slot)]).expect("drop");
}
