//! ABI v2 TLS builtins (`tls.rs`): the .NET `SslStream` exception mapping and parking.
//! Loopback only — the peers are plain TCP listeners and an in-process rustls server
//! with a self-signed certificate (`tls_test_data/`: P-256, CN / SAN `localhost` +
//! `127.0.0.1`, valid until 2126), which the client must reject: it is not under the
//! bundled roots.

use crate::corelib::abi::BuiltinError;
use crate::corelib::network::network_tests::{call, collect_while_parked, detailed};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::sync::Arc;
use std::time::Duration;

fn s(v: &str) -> Value {
    Value::Str(v.to_string().into())
}

fn connect(ctx: &VmContext, host: &str, port: u16, timeout_ms: i64) -> Result<Option<Value>, BuiltinError> {
    call(ctx, "__tls_connect", &[s(host), Value::I64(port as i64), Value::I64(timeout_ms)])
}

fn listener() -> (std::net::TcpListener, u16) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    (l, port)
}

#[test]
fn a_peer_hanging_up_mid_handshake_is_an_io_exception_and_the_wait_is_parked() {
    let ctx = VmContext::new();
    let (l, port) = listener();
    let core = Arc::clone(&ctx.core);
    let client = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        connect(&w, "127.0.0.1", port, 0).err()
    });
    let (mut peer, _) = l.accept().expect("accept");
    collect_while_parked(&ctx);
    // Read the ClientHello first: closing with unread input would reset (a different
    // IOException text) instead of a clean EOF.
    use std::io::Read;
    let mut hello = [0u8; 4096];
    let _ = peer.read(&mut hello);
    drop(peer);
    let e = client.join().unwrap().expect("the handshake fails");
    assert!(matches!(e, BuiltinError::Io(ref m) if m.contains("unexpected EOF")), "{e}");
}

#[test]
fn a_silent_peer_times_the_handshake_out_as_an_io_exception() {
    let ctx = VmContext::new();
    let (_l, port) = listener();
    let e = connect(&ctx, "127.0.0.1", port, 150).unwrap_err();
    assert!(matches!(e, BuiltinError::Io(ref m) if m.starts_with("Unable to read data from the transport connection")), "{e}");
}

#[test]
fn a_peer_that_does_not_speak_tls_fails_authentication() {
    let ctx = VmContext::new();
    let (l, port) = listener();
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let (mut c, _) = l.accept().expect("accept");
        let mut hello = [0u8; 64];
        let _ = c.read(&mut hello);
        let _ = c.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
        std::thread::sleep(Duration::from_millis(200));
    });
    let e = connect(&ctx, "127.0.0.1", port, 2000).unwrap_err();
    assert!(matches!(e, BuiltinError::Exception { class: "Std.Security.Authentication.AuthenticationException", .. }), "{e}");
    server.join().unwrap();
}

/// A rustls server presenting the self-signed test certificate.
fn self_signed_server() -> (std::thread::JoinHandle<()>, u16) {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let cert = CertificateDer::from(include_bytes!("tls_test_data/cert.der").to_vec());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(include_bytes!("tls_test_data/key.der").to_vec()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions().unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key).expect("test cert");
    let config = Arc::new(config);
    let (l, port) = listener();
    let server = std::thread::spawn(move || {
        let (mut tcp, _) = l.accept().expect("accept");
        let mut conn = rustls::ServerConnection::new(config).unwrap();
        // Runs until the client aborts with its certificate alert.
        let _ = conn.complete_io(&mut tcp);
    });
    (server, port)
}

#[test]
fn an_untrusted_certificate_fails_authentication() {
    let ctx = VmContext::new();
    let (server, port) = self_signed_server();
    let e = connect(&ctx, "localhost", port, 5000).unwrap_err();
    match &e {
        BuiltinError::Exception { class, message } => {
            assert_eq!(*class, "Std.Security.Authentication.AuthenticationException");
            assert!(message.starts_with("The remote certificate is invalid"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    server.join().unwrap();
}

#[test]
fn bad_arguments_and_unknown_handles() {
    let ctx = VmContext::new();
    let e = connect(&ctx, "127.0.0.1", 0, 0).err();
    // Port 0 is a valid number but nobody listens there: the TCP connect fails.
    assert_eq!(detailed(&e.expect("fails")).0, "Std.Net.Sockets.SocketException");
    let e = call(&ctx, "__tls_connect", &[s("127.0.0.1"), Value::I64(-1), Value::I64(0)]).unwrap_err();
    assert!(matches!(e, BuiltinError::ArgumentOutOfRange(_)), "{e}");
    let e = connect(&ctx, "not a host name", 443, 0).unwrap_err();
    assert!(matches!(e, BuiltinError::Argument(_)), "{e}");
    let buf = ctx.heap().alloc_bytes(vec![0u8; 4]);
    let e = call(&ctx, "__tls_read", &[Value::I64(999_999), buf, Value::I64(0), Value::I64(4)]).unwrap_err();
    assert_eq!(detailed(&e).0, "Std.Net.Sockets.SocketClosedException");
    let e = call(&ctx, "__tls_set_read_timeout", &[Value::I64(999_999), Value::I64(10)]).unwrap_err();
    assert_eq!(detailed(&e).0, "Std.Net.Sockets.SocketClosedException");
    call(&ctx, "__tls_close", &[Value::I64(999_999)]).unwrap();
}
