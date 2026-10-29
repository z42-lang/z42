//! `Std.Net` TLS client builtins (builtin ABI v2) — rustls-backed blocking TLS streams:
//! `TlsSocketNative` in z42.core, used by z42.net's `TlsClient` / `TlsStream`.
//!
//! Same handle model as the TCP builtins (`network/tcp.rs`): a slot id into
//! `VmCore.tls_sockets`; the table holds an `Arc<TlsSlot>` that calls clone. The rustls
//! session needs `&mut` for reads and writes alike, so it sits behind a lock, with a dup
//! of the TCP socket beside it for shutdown and timeouts. Read and write are therefore
//! serialized: while one thread is blocked in a read, another thread's write waits for
//! it. Certificate verification is always on (bundled Mozilla roots, `webpki-roots`;
//! rustls' `ring` provider — no OpenSSL / C toolchain).
//!
//! **Failures throw from Rust, as .NET's `TcpClient` + `SslStream` do:**
//!
//! | failure | exception |
//! |---|---|
//! | name resolution / TCP connect | `SocketException` (`network/error.rs`) |
//! | the handshake: certificate rejected (unknown issuer, name mismatch, expired), protocol mismatch, the peer is not speaking TLS | `Std.Security.Authentication.AuthenticationException` |
//! | the transport fails during the handshake (peer hung up, reset, timed out) | `IOException` (`Received an unexpected EOF or 0 bytes from the transport stream.` / `Unable to read data from the transport connection: …`) |
//! | read / write on the established stream (I/O error, timeout, corrupt record) | `IOException` (`Unable to read / write data to the transport connection: …`) |
//! | a host name that cannot be a TLS server name | `ArgumentException` |
//! | a closed / unknown handle (also: closed while blocked on it) | `SocketClosedException : ObjectDisposedException` |
//!
//! A peer that closes the connection without a TLS `close_notify` reads as end of stream
//! (0), as `SslStream` does.
//!
//! Connect (resolution + connect + handshake), read and write run parked
//! (`Blocking::new(..).then(..)`); the results are built after unparking.
//!
/// One TLS connection. `stream` holds the TLS session plus the socket; `sock` is a dup
/// of the same socket, used to shut it down on close — waking a read blocked inside
/// `stream`'s lock — and to set timeouts without waiting for that lock.
pub(crate) struct TlsSlot {
    pub(crate) stream: parking_lot::Mutex<rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>>,
    pub(crate) sock: std::net::TcpStream,
}

/// Process-wide rustls client config (root store + ring provider), built once.
pub(crate) fn client_config() -> Result<std::sync::Arc<rustls::ClientConfig>, crate::corelib::abi::BuiltinError> {
    use std::sync::{Arc, OnceLock};
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    if let Some(c) = CONFIG.get() {
        return Ok(c.clone());
    }
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| crate::corelib::abi::BuiltinError::Internal(format!("tls config: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    // First writer wins; a concurrent racer just discards its identical copy.
    Ok(CONFIG.get_or_init(|| Arc::new(config)).clone())
}

mod imp {
    use super::TlsSlot;
    use crate::corelib::abi::BuiltinError;
    use crate::corelib::network::error::{closed, describe, slot_id, timeout_message};
    use crate::vm_context::VmContext;
    use std::sync::Arc;

    pub(super) fn slot(ctx: &VmContext, slot: i64) -> Result<Arc<TlsSlot>, BuiltinError> {
        ctx.core.tls_sockets.get_cloned(slot_id(slot)?).ok_or_else(closed)
    }

    pub(super) fn still_open(ctx: &VmContext, id: u64) -> Result<(), BuiltinError> {
        if ctx.core.tls_sockets.lock().contains_key(&id) { Ok(()) } else { Err(closed()) }
    }

    /// The rustls error inside an I/O error (rustls reports protocol failures as
    /// `InvalidData` wrapping its own error).
    fn tls_cause(e: &std::io::Error) -> Option<&rustls::Error> {
        e.get_ref().and_then(|inner| inner.downcast_ref::<rustls::Error>())
    }

    fn transport_text(e: &std::io::Error) -> String {
        match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => timeout_message(),
            _ => describe(e),
        }
    }

    /// A failed handshake (`complete_io`): rustls' own verdict → `AuthenticationException`;
    /// the transport giving out → `IOException`.
    pub(super) fn handshake_error(e: &std::io::Error) -> BuiltinError {
        if let Some(tls) = tls_cause(e) {
            let message = match tls {
                rustls::Error::InvalidCertificate(_) | rustls::Error::NoCertificatesPresented => format!(
                    "The remote certificate is invalid according to the validation procedure: {tls}."),
                other => format!("Authentication failed: {other}."),
            };
            return BuiltinError::Exception { class: "Std.Security.Authentication.AuthenticationException", message };
        }
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            return BuiltinError::Io("Received an unexpected EOF or 0 bytes from the transport stream.".into());
        }
        BuiltinError::Io(format!("Unable to read data from the transport connection: {}.", transport_text(e)))
    }

    /// A failed read / write on the established stream → `IOException`.
    pub(super) fn stream_error(e: &std::io::Error, op: &str) -> BuiltinError {
        let text = match tls_cause(e) {
            Some(tls) => tls.to_string(),
            None => transport_text(e),
        };
        BuiltinError::Io(format!("Unable to {op} data {} the transport connection: {text}.",
            if op == "read" { "from" } else { "to" }))
    }
}

#[z42_macros::builtins]
pub mod typed {
    use super::imp::{handshake_error, slot, still_open, stream_error};
    use super::{client_config, TlsSlot};
    use crate::corelib::abi::{Blocking, BuiltinError, Then};
    use crate::corelib::fs::{byte_array, check_buffer_range, read_byte_range, write_byte_range};
    use crate::corelib::network::error::{check_port, resolve, slot_id, socket_error, timeout};
    use crate::metadata::Value;
    use crate::vm_context::VmContext;
    use rustls::pki_types::ServerName;
    use rustls::{ClientConnection, StreamOwned};
    use std::io::{Read, Write};
    use std::net::TcpStream;

    /// `Connect(host, port, timeoutMs) -> slot`: TCP connect, then the handshake (SNI =
    /// `host`), forced here so certificate / protocol errors surface at connect.
    /// `timeoutMs > 0` bounds the connect and the handshake each; the deadline is cleared
    /// afterwards (stream timeouts are set separately).
    #[builtin(name = "__tls_connect")]
    pub fn tls_connect(host: String, port: i64, timeout_ms: i64) -> Result<Then<i64>, BuiltinError> {
        let port = check_port(port)?;
        let limit = timeout(timeout_ms);
        let name = ServerName::try_from(host.clone()).map_err(|_| BuiltinError::Argument(format!(
            "The target host name is not valid for TLS: '{host}'. (Parameter 'host')")))?;
        let conn = ClientConnection::new(client_config()?, name)
            .map_err(|e| BuiltinError::Internal(format!("tls client init: {e}")))?;
        Ok(Blocking::new(move || {
            let addrs = resolve(&host, port)?;
            let mut last = None;
            let mut tcp = None;
            for addr in &addrs {
                let r = match limit {
                    Some(d) => TcpStream::connect_timeout(addr, d),
                    None => TcpStream::connect(addr),
                };
                match r {
                    Ok(s) => { tcp = Some(s); break; }
                    Err(e) => last = Some(socket_error(&e, Some(addr))),
                }
            }
            let Some(tcp) = tcp else {
                return Err(last.unwrap_or_else(|| BuiltinError::Internal("connect: no address".into())));
            };
            if limit.is_some() {
                let _ = tcp.set_read_timeout(limit);
                let _ = tcp.set_write_timeout(limit);
            }
            let mut stream = StreamOwned::new(conn, tcp);
            stream.conn.complete_io(&mut stream.sock).map_err(|e| handshake_error(&e))?;
            if limit.is_some() {
                let _ = stream.sock.set_read_timeout(None);
                let _ = stream.sock.set_write_timeout(None);
            }
            let sock = stream.sock.try_clone().map_err(|e| socket_error(&e, None))?;
            Ok(TlsSlot { stream: parking_lot::Mutex::new(stream), sock })
        }).then(|ctx: &VmContext, s: TlsSlot| Ok(ctx.alloc_tls_socket_slot(s) as i64)))
    }

    /// `Read(slot, buffer, offset, count) -> n` decrypted bytes (0 = end of stream).
    #[builtin(name = "__tls_read")]
    pub fn tls_read(ctx: &VmContext, slot_arg: i64, buffer: Value, offset: i64, count: i64) -> Result<Then<i64>, BuiltinError> {
        let arr = byte_array(&buffer, "buffer")?;
        let (off, n) = check_buffer_range(offset, count, arr.borrow().len())?;
        let id = slot_id(slot_arg)?;
        let s = slot(ctx, slot_arg)?;
        let read = if n == 0 {
            Blocking::ready(Ok(Vec::new()))
        } else {
            Blocking::new(move || {
                let mut tmp = vec![0u8; n];
                let r = s.stream.lock().read(&mut tmp);
                Ok(match r {
                    Ok(got) => { tmp.truncate(got); Ok(tmp) }
                    // No close_notify: end of stream, as SslStream reads it.
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(Vec::new()),
                    Err(e) => Err(e),
                })
            })
        };
        Ok(read.then(move |ctx: &VmContext, r: std::io::Result<Vec<u8>>| {
            still_open(ctx, id)?;
            let bytes = r.map_err(|e| stream_error(&e, "read"))?;
            write_byte_range(&arr.borrow(), off, &bytes);
            Ok(bytes.len() as i64)
        }))
    }

    /// `Write(slot, buffer, offset, count)` — encrypts and writes all of it, then flushes.
    #[builtin(name = "__tls_write")]
    pub fn tls_write(ctx: &VmContext, slot_arg: i64, buffer: &Value, offset: i64, count: i64) -> Result<Then<()>, BuiltinError> {
        let arr = byte_array(buffer, "buffer")?;
        let bytes = {
            let a = arr.borrow();
            let (off, n) = check_buffer_range(offset, count, a.len())?;
            read_byte_range(&a, off, n)?
        };
        let id = slot_id(slot_arg)?;
        let s = slot(ctx, slot_arg)?;
        let write = if bytes.is_empty() {
            Blocking::ready(Ok(()))
        } else {
            Blocking::new(move || {
                let mut st = s.stream.lock();
                Ok(st.write_all(&bytes).and_then(|_| st.flush()))
            })
        };
        Ok(write.then(move |ctx: &VmContext, r: std::io::Result<()>| {
            still_open(ctx, id)?;
            r.map_err(|e| stream_error(&e, "write"))
        }))
    }

    /// Idempotent. Shuts the socket down so a read blocked on another thread wakes up.
    #[builtin(name = "__tls_close")]
    pub fn tls_close(ctx: &VmContext, slot_arg: i64) {
        if let Ok(id) = u64::try_from(slot_arg) {
            if let Some(s) = ctx.core.tls_sockets.take(id) {
                let _ = s.sock.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    /// `millis <= 0` clears the timeout. Goes through the socket dup: does not wait for a
    /// read / write that holds the session lock.
    #[builtin(name = "__tls_set_read_timeout")]
    pub fn tls_set_read_timeout(ctx: &VmContext, slot_arg: i64, millis: i64) -> Result<(), BuiltinError> {
        slot(ctx, slot_arg)?.sock.set_read_timeout(timeout(millis)).map_err(|e| socket_error(&e, None))
    }

    #[builtin(name = "__tls_set_write_timeout")]
    pub fn tls_set_write_timeout(ctx: &VmContext, slot_arg: i64, millis: i64) -> Result<(), BuiltinError> {
        slot(ctx, slot_arg)?.sock.set_write_timeout(timeout(millis)).map_err(|e| socket_error(&e, None))
    }
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tls_tests;
