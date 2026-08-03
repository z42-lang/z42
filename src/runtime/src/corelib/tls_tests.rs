use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

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

fn ok_val(v: &Value) -> i64 {
    match v {
        Value::Array(rc) => {
            let b = rc.borrow();
            assert_eq!(b.len(), 2, "ok-value tuple has 2 elements: {:?}", b);
            match (&b.get_boxed(0), &b.get_boxed(1)) {
                (Value::I64(0), Value::I64(s)) => *s,
                _ => panic!("not an ok tuple: {:?}", b),
            }
        }
        _ => panic!("not an Array: {:?}", v),
    }
}

// ── Connect failures (no network needed) ─────────────────────────────────

// localhost:1 is guaranteed-refused (same rationale as network_tests). The
// TCP connect fails before any TLS handshake, so we get KIND_SOCKET_ERR.
#[test]
fn connect_to_unbound_port_returns_socket_err() {
    let ctx = ctx();
    let args = vec![Value::Str("127.0.0.1".to_string().into()), Value::I64(1), Value::I64(0)];
    let r = builtin_net_tls_connect(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_SOCKET_ERR), "got {:?}", r);
    assert_eq!(ctx.tls_socket_slot_count(), 0, "failed connect must not leak a slot");
}

// An empty / invalid SNI host is rejected by rustls' ServerName parse before
// any socket is opened.
#[test]
fn connect_with_invalid_server_name_returns_socket_err() {
    let ctx = ctx();
    let args = vec![Value::Str("not a valid host".to_string().into()), Value::I64(443), Value::I64(0)];
    let r = builtin_net_tls_connect(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_SOCKET_ERR), "got {:?}", r);
}

// ── Slot lookups on unknown ids ──────────────────────────────────────────

#[test]
fn read_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let buf = arr(vec![Value::I64(0); 16], &ctx);
    let args = vec![Value::I64(999_999), buf, Value::I64(0), Value::I64(16)];
    let r = builtin_net_tls_socket_read(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[test]
fn write_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let buf = arr(vec![Value::I64(b'x' as i64)], &ctx);
    let args = vec![Value::I64(999_999), buf, Value::I64(0), Value::I64(1)];
    let r = builtin_net_tls_socket_write(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[test]
fn set_read_timeout_on_unknown_slot_returns_handle_invalid() {
    let ctx = ctx();
    let args = vec![Value::I64(999_999), Value::I64(1000)];
    let r = builtin_net_tls_socket_set_read_timeout(&ctx, &args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_HANDLE_INVALID));
}

#[test]
fn drop_unknown_slot_is_silent_null() {
    let ctx = ctx();
    let args = vec![Value::I64(999_999)];
    let r = builtin_net_tls_socket_drop(&ctx, &args).expect("call ok");
    assert!(matches!(r, Value::Null));
}

// ── Real-endpoint handshake (network; run with `--ignored`) ──────────────

// Not part of the default GREEN run: CI sandboxes block outbound 443 and we
// don't want a network flake to fail the suite. Validates the full path
// (TCP connect → TLS handshake against bundled Mozilla roots → HTTP GET →
// decrypt response) against a stable host. Run manually:
//   cargo test -p z42 tls_tests -- --ignored --nocapture
#[test]
#[ignore = "requires outbound network on :443"]
fn real_https_handshake_and_get_round_trip() {
    let ctx = ctx();
    let host = "example.com";
    let connect_args = vec![Value::Str(host.to_string().into()), Value::I64(443), Value::I64(5000)];
    let r = builtin_net_tls_connect(&ctx, &connect_args).expect("call ok");
    assert_eq!(kind_of(&r), Some(KIND_OK), "handshake failed: {:?}", r);
    let slot = ok_val(&r);

    // Minimal HTTP/1.0 request (no keep-alive → server closes after body).
    let req = format!("GET / HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n", host);
    let bytes: Vec<Value> = req.bytes().map(|b| Value::I64(b as i64)).collect();
    let n = bytes.len();
    let write_buf = arr(bytes, &ctx);
    let w = builtin_net_tls_socket_write(
        &ctx,
        &[Value::I64(slot), write_buf, Value::I64(0), Value::I64(n as i64)],
    ).expect("write ok");
    assert_eq!(ok_val(&w), n as i64);

    // Read the first chunk of the response and assert it looks like HTTP.
    let read_buf = arr(vec![Value::I64(0); 64], &ctx);
    let rd = builtin_net_tls_socket_read(
        &ctx,
        &[Value::I64(slot), read_buf.clone(), Value::I64(0), Value::I64(64)],
    ).expect("read ok");
    let got = ok_val(&rd);
    assert!(got > 0, "expected response bytes, got {}", got);
    if let Value::Array(rc) = &read_buf {
        let b = rc.borrow();
        let head: Vec<u8> = b.iter_boxed().take(got as usize).map(|v| match v {
            Value::I64(x) => x as u8,
            _ => 0,
        }).collect();
        assert!(head.starts_with(b"HTTP/"), "response head: {:?}", String::from_utf8_lossy(&head));
    }

    let _ = builtin_net_tls_socket_drop(&ctx, &[Value::I64(slot)]).expect("drop");
    assert_eq!(ctx.tls_socket_slot_count(), 0);
}
