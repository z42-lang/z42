//! `Std.Net` TLS client builtins — rustls-backed blocking TLS streams.
//!
//! add-z42-net-tls (2026-06-03): mirrors `network.rs` exactly (slot-id
//! handle + per-builtin slot lookup, remove-on-IO to avoid holding the
//! map lock across a blocking read/write). The only difference from the
//! raw TCP builtins is that the stored handle is a `rustls::StreamOwned`
//! (TCP socket + TLS session state) instead of a bare `TcpStream`.
//!
//! Certificate verification is always on: roots come from the bundled
//! Mozilla set (`webpki-roots`), so no host trust-store wiring is needed.
//! The crypto backend is rustls' `ring` provider (pure-Rust build, no
//! OpenSSL / aws-lc-rs C toolchain).
//!
//! ## Return shape
//!
//! Identical to `network.rs` — a discriminated `Value::Array` tuple whose
//! first element is a `KIND_*` tag:
//!
//! ```text
//!   [I64(0), I64(slot)]      // KIND_OK — connect
//!   [I64(0), I64(nbytes)]    // KIND_OK — read / write (0 = EOF / set-timeout)
//!   [I64(1), Str(message)]   // KIND_SOCKET_ERR — io / handshake fail
//!   [I64(2)]                 // KIND_HANDLE_INVALID — slot missing
//!   [I64(3)]                 // KIND_UNSUPPORTED — wasm32
//! ```
//!
//! Drops always return `Value::Null` (idempotent).

use super::convert::{arg_i64, arg_str};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

const KIND_OK:             i64 = 0;
const KIND_SOCKET_ERR:     i64 = 1;
const KIND_HANDLE_INVALID: i64 = 2;
#[cfg(target_arch = "wasm32")]
const KIND_UNSUPPORTED:    i64 = 3;

fn ok_value(ctx: &VmContext, v: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK), Value::I64(v)])
}

fn socket_err(ctx: &VmContext, msg: String) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_SOCKET_ERR), Value::Str(msg.into())])
}

fn handle_invalid(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_HANDLE_INVALID)])
}

#[cfg(target_arch = "wasm32")]
fn unsupported(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_UNSUPPORTED)])
}

fn require_slot_id(args: &[Value], idx: usize, name: &str) -> Result<u64> {
    let n = arg_i64(args, idx, name)?;
    if n < 0 {
        bail!("{}: slot id must be non-negative, got {}", name, n);
    }
    Ok(n as u64)
}

#[cfg(not(target_arch = "wasm32"))]
fn require_port(args: &[Value], idx: usize, name: &str) -> Result<u16> {
    let p = arg_i64(args, idx, name)?;
    if !(0..=65535).contains(&p) {
        bail!("{}: port out of range [0, 65535]: {}", name, p);
    }
    Ok(p as u16)
}

// ── desktop / mobile (non-wasm32) implementations ─────────────────────────

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;
    use rustls::pki_types::ServerName;
    use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

    /// Process-wide rustls client config (root store + ring provider). Built
    /// once and shared via `Arc` — parsing the Mozilla root set on every
    /// connect would be wasteful, and the config is immutable.
    fn client_config() -> Result<Arc<ClientConfig>> {
        static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
        if let Some(c) = CONFIG.get() {
            return Ok(c.clone());
        }
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| anyhow::anyhow!("tls config: {}", e))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let arc = Arc::new(config);
        // First writer wins; a concurrent racer just discards its identical copy.
        let _ = CONFIG.set(arc.clone());
        Ok(CONFIG.get().cloned().unwrap_or(arc))
    }

    /// `__net_tls_connect(host, port, timeout_ms) -> [0, slot] | err`
    /// TCP-connects to `host:port`, performs a TLS handshake with SNI =
    /// `host` and certificate verification against the bundled roots, then
    /// stores the live stream and returns its slot id. The handshake is
    /// forced here (`complete_io`) so cert / protocol errors surface at
    /// connect rather than on the first read.
    ///
    /// `timeout_ms > 0` bounds both the TCP connect and the handshake (a
    /// stalled peer can't hang the script). The deadline is cleared once the
    /// handshake completes — subsequent read/write timeouts are set
    /// explicitly via `__net_tls_socket_set_*_timeout`. `timeout_ms <= 0`
    /// means blocking (no deadline), matching the raw-TCP builtins.
    pub fn builtin_net_tls_connect(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tls_connect";
        let host = arg_str(args, 0, NAME)?.to_string();
        let port = require_port(args, 1, NAME)?;
        let millis = arg_i64(args, 2, NAME)?;
        let deadline = if millis > 0 { Some(Duration::from_millis(millis as u64)) } else { None };

        let config = client_config()?;
        let server_name = match ServerName::try_from(host.clone()) {
            Ok(n) => n,
            Err(e) => return Ok(socket_err(ctx, format!("invalid server name {:?}: {}", host, e))),
        };
        let conn = match ClientConnection::new(config, server_name) {
            Ok(c) => c,
            Err(e) => return Ok(socket_err(ctx, format!("tls client init: {}", e))),
        };

        let addr = format!("{}:{}", host, port);
        let tcp = match deadline {
            Some(dur) => {
                let socket_addr = match addr.to_socket_addrs().and_then(|mut it| {
                    it.next().ok_or_else(|| std::io::Error::new(
                        std::io::ErrorKind::AddrNotAvailable, "no addresses"))
                }) {
                    Ok(a) => a,
                    Err(e) => return Ok(socket_err(ctx, format!("resolve {}: {}", addr, e))),
                };
                match TcpStream::connect_timeout(&socket_addr, dur) {
                    Ok(s) => s,
                    Err(e) => return Ok(socket_err(ctx, format!(
                        "connect to {} (timeout {}ms): {}", addr, millis, e))),
                }
            }
            None => match TcpStream::connect(&addr) {
                Ok(s) => s,
                Err(e) => return Ok(socket_err(ctx, format!("connect to {}: {}", addr, e))),
            },
        };

        // Bound the handshake itself: a half-open peer that completes the TCP
        // SYN but never finishes the TLS exchange would otherwise hang here.
        if let Some(dur) = deadline {
            let _ = tcp.set_read_timeout(Some(dur));
            let _ = tcp.set_write_timeout(Some(dur));
        }
        let mut stream = StreamOwned::new(conn, tcp);
        if let Err(e) = stream.conn.complete_io(&mut stream.sock) {
            return Ok(socket_err(ctx, format!("tls handshake with {}: {}", host, e)));
        }
        // Clear the handshake deadline; per-call read/write timeouts (if any)
        // are applied separately by the z42 TlsClient after connect.
        if deadline.is_some() {
            let _ = stream.get_ref().set_read_timeout(None);
            let _ = stream.get_ref().set_write_timeout(None);
        }
        let slot_id = ctx.alloc_tls_socket_slot(stream);
        Ok(ok_value(ctx, slot_id as i64))
    }

    /// `__net_tls_socket_read(slot, buf, offset, count) -> [0, n] | err | invalid`
    /// Reads up to `count` decrypted bytes into `buf[offset..]`; `n == 0`
    /// signals clean EOF (peer closed the TLS session).
    pub fn builtin_net_tls_socket_read(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tls_socket_read";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let buf_arr = match args.get(1) {
            Some(Value::Array(rc)) => rc.clone(),
            other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
        };
        let offset = arg_i64(args, 2, NAME)? as usize;
        let count  = arg_i64(args, 3, NAME)? as usize;

        let buf_len = buf_arr.borrow().len();
        if offset + count > buf_len {
            bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
        }
        if count == 0 { return Ok(ok_value(ctx, 0)); }

        let stream = {
            let mut map = ctx.core.tls_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(mut stream) = stream else {
            return Ok(handle_invalid(ctx));
        };

        let mut tmp = vec![0u8; count];
        let read_result = stream.read(&mut tmp);

        ctx.core.tls_sockets.lock().insert(slot_id, stream);

        match read_result {
            Ok(n) => {
                let mut borrowed = buf_arr.borrow_mut();
                for i in 0..n {
                    borrowed.set_boxed(offset + i, Value::I64(tmp[i] as i64));
                }
                Ok(ok_value(ctx, n as i64))
            }
            Err(e) => Ok(socket_err(ctx, format!("read: {}", e))),
        }
    }

    /// `__net_tls_socket_write(slot, buf, offset, count) -> [0, n] | err | invalid`
    /// Encrypts and writes all `count` bytes from `buf[offset..]`.
    pub fn builtin_net_tls_socket_write(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tls_socket_write";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let buf_arr = match args.get(1) {
            Some(Value::Array(rc)) => rc.clone(),
            other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
        };
        let offset = arg_i64(args, 2, NAME)? as usize;
        let count  = arg_i64(args, 3, NAME)? as usize;

        let buf_len = buf_arr.borrow().len();
        if offset + count > buf_len {
            bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
        }
        if count == 0 { return Ok(ok_value(ctx, 0)); }

        let mut tmp = vec![0u8; count];
        {
            let borrowed = buf_arr.borrow();
            for i in 0..count {
                match borrowed.get_boxed(offset + i) {
                    Value::I64(v) => tmp[i] = (v & 0xFF) as u8,
                    other => bail!("{}: byte[] elem at {} expected I64, got {:?}", NAME, offset + i, other),
                }
            }
        }

        let stream = {
            let mut map = ctx.core.tls_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(mut stream) = stream else {
            return Ok(handle_invalid(ctx));
        };

        // write_all then flush — rustls buffers cleartext until flushed.
        let write_result = stream.write_all(&tmp).and_then(|_| stream.flush()).map(|_| count);

        ctx.core.tls_sockets.lock().insert(slot_id, stream);

        match write_result {
            Ok(n) => Ok(ok_value(ctx, n as i64)),
            Err(e) => Ok(socket_err(ctx, format!("write: {}", e))),
        }
    }

    /// `__net_tls_socket_drop(slot) -> null` — idempotent; closes the fd.
    pub fn builtin_net_tls_socket_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tls_socket_drop";
        let slot_id = require_slot_id(args, 0, NAME)?;
        ctx.core.tls_sockets.lock().remove(&slot_id);
        Ok(Value::Null)
    }

    // add-z42-net-tls (2026-06-03): read / write deadlines on the underlying
    // TCP socket so a stalled peer can't hang the script. `millis <= 0`
    // clears the timeout (blocking I/O). Setting the timeout is non-blocking,
    // so unlike read/write we hold the map lock briefly instead of removing.
    fn apply_timeout(
        ctx: &VmContext,
        slot_id: u64,
        millis: i64,
        which: &'static str,
    ) -> Result<Value> {
        let dur = if millis > 0 {
            Some(std::time::Duration::from_millis(millis as u64))
        } else {
            None
        };
        let map = ctx.core.tls_sockets.lock();
        let Some(stream) = map.get(&slot_id) else {
            return Ok(handle_invalid(ctx));
        };
        let result = if which == "set_read_timeout" {
            stream.get_ref().set_read_timeout(dur)
        } else {
            stream.get_ref().set_write_timeout(dur)
        };
        match result {
            Ok(()) => Ok(ok_value(ctx, 0)),
            Err(e) => Ok(socket_err(ctx, format!("{}: {}", which, e))),
        }
    }

    /// `__net_tls_socket_set_read_timeout(slot, millis) -> [0,0] | err | invalid`
    pub fn builtin_net_tls_socket_set_read_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tls_socket_set_read_timeout";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let millis = arg_i64(args, 1, NAME)?;
        apply_timeout(ctx, slot_id, millis, "set_read_timeout")
    }

    /// `__net_tls_socket_set_write_timeout(slot, millis) -> [0,0] | err | invalid`
    pub fn builtin_net_tls_socket_set_write_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tls_socket_set_write_timeout";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let millis = arg_i64(args, 1, NAME)?;
        apply_timeout(ctx, slot_id, millis, "set_write_timeout")
    }
}

// ── wasm32: all builtins return KIND_UNSUPPORTED tuple ────────────────────

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;

    pub fn builtin_net_tls_connect(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tls_socket_read(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tls_socket_write(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tls_socket_drop(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(Value::Null)
    }
    pub fn builtin_net_tls_socket_set_read_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tls_socket_set_write_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
}

pub use imp::*;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tls_tests.rs"]
mod tls_tests;
