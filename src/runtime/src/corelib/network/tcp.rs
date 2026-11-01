//! `Std.Net.Sockets` TCP builtins (builtin ABI v2): `TcpSocketNative` in z42.core,
//! used by z42.net's `TcpClient` / `TcpListener` / `NetworkStream`.
//!
//! Handles are registry slot ids (`VmCore.tcp_sockets` / `tcp_listeners`). A call clones
//! the `Arc` out under the table lock and does the system call outside it, so a socket
//! is full duplex and a read blocked on one thread does not stall another thread's
//! write or close. Close = take the slot out + `shutdown`, which wakes a blocked read /
//! write; the fd closes when the last `Arc` drops.
//!
//! - **Failures throw from Rust** — the mapping is in `network/error.rs`.
//! - **Everything that can block runs parked** (name resolution, connect, accept, read,
//!   write) — `Blocking::new(..).then(..)`: the parked part sees only Rust data; the
//!   `then` step (register the new slot, copy into the caller's `byte[]`, notice a close
//!   that happened meanwhile) runs unparked. Option setters (`setsockopt`) do not block.

use super::error::{check_port, check_ttl, closed, out_of_range, resolve, slot_id, socket_error, timeout};
use super::listener::{accept_wait, AcceptWait, ListenerSlot};
use crate::corelib::abi::BuiltinError;
use crate::vm_context::VmContext;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;

/// Connect to the first address of `addrs` that accepts (as `TcpStream::connect` does),
/// bounded by `limit` per address. The error is the last address's, with its endpoint.
fn connect_any(addrs: &[SocketAddr], limit: Option<std::time::Duration>) -> Result<TcpStream, BuiltinError> {
    let mut last = None;
    for addr in addrs {
        let r = match limit {
            Some(d) => TcpStream::connect_timeout(addr, d),
            None => TcpStream::connect(addr),
        };
        match r {
            Ok(s) => return Ok(s),
            Err(e) => last = Some(socket_error(&e, Some(addr))),
        }
    }
    Err(last.unwrap_or_else(|| BuiltinError::Internal("connect: no address to try".into())))
}

/// Bind + listen on `addr`. `reuse_addr` sets `SO_REUSEADDR` before the bind (POSIX wants
/// it pre-bind); without it this is std's `TcpListener::bind` (which itself sets the
/// option on Unix, not on Windows).
fn listen_on(addr: SocketAddr, reuse_addr: bool) -> std::io::Result<TcpListener> {
    if !reuse_addr {
        return TcpListener::bind(addr);
    }
    let domain = if addr.is_ipv4() { socket2::Domain::IPV4 } else { socket2::Domain::IPV6 };
    let sock = socket2::Socket::new(domain, socket2::Type::STREAM, None)?;
    sock.set_reuse_address(true)?;
    sock.bind(&addr.into())?;
    sock.listen(128)?;
    Ok(sock.into())
}

pub(crate) fn stream(ctx: &VmContext, slot: i64) -> Result<Arc<TcpStream>, BuiltinError> {
    ctx.core.tcp_sockets.get_cloned(slot_id(slot)?).ok_or_else(closed)
}

fn listener(ctx: &VmContext, slot: i64) -> Result<ListenerSlot, BuiltinError> {
    ctx.core.tcp_listeners.get_cloned(slot_id(slot)?).ok_or_else(closed)
}

/// After a parked read / write: a close that happened meanwhile wins over whatever the
/// woken system call returned (the close's `shutdown` is what woke it).
fn still_open(ctx: &VmContext, id: u64) -> Result<(), BuiltinError> {
    if ctx.core.tcp_sockets.lock().contains_key(&id) { Ok(()) } else { Err(closed()) }
}

#[z42_macros::builtins]
pub mod typed {
    use super::*;
    use crate::corelib::abi::{Blocking, Then};
    use crate::corelib::fs::{byte_array, check_buffer_range, read_byte_range, write_byte_range};
    use crate::object::Value;
    use std::io::{Read, Write};

    /// `Connect(host, port, timeoutMs) -> slot`; `timeoutMs <= 0` = no limit.
    #[builtin(name = "__tcp_connect")]
    pub fn tcp_connect(host: String, port: i64, timeout_ms: i64) -> Result<Then<i64>, BuiltinError> {
        let port = check_port(port)?;
        let limit = timeout(timeout_ms);
        Ok(Blocking::new(move || connect_any(&resolve(&host, port)?, limit))
            .then(|ctx: &VmContext, s: TcpStream| Ok(ctx.alloc_tcp_socket_slot(s) as i64)))
    }

    /// `Listen(host, port, reuseAddress) -> listener slot` (port 0 = the OS picks).
    #[builtin(name = "__tcp_listen")]
    pub fn tcp_listen(host: String, port: i64, reuse_addr: bool) -> Result<Then<i64>, BuiltinError> {
        let port = check_port(port)?;
        Ok(Blocking::new(move || {
            let addr = resolve(&host, port)?[0];
            listen_on(addr, reuse_addr).map_err(|e| socket_error(&e, None))
        }).then(|ctx: &VmContext, l: TcpListener| Ok(ctx.alloc_tcp_listener_slot(l) as i64)))
    }

    /// The port a listener is bound to.
    #[builtin(name = "__tcp_listener_port")]
    pub fn tcp_listener_port(ctx: &VmContext, slot: i64) -> Result<i64, BuiltinError> {
        let l = listener(ctx, slot)?;
        l.listener.local_addr().map(|a| a.port() as i64).map_err(|e| socket_error(&e, None))
    }

    /// `Accept(listener) -> slot`. A listener closed while waiting → `SocketClosedException`
    /// within the accept poll period (`network/listener.rs`).
    #[builtin(name = "__tcp_accept")]
    pub fn tcp_accept(ctx: &VmContext, slot: i64) -> Result<Then<i64>, BuiltinError> {
        let l = listener(ctx, slot)?;
        Ok(Blocking::new(move || match accept_wait(&l) {
            Ok((s, _peer)) => Ok(s),
            Err(AcceptWait::Closed) => Err(closed()),
            Err(AcceptWait::Io(e)) => Err(socket_error(&e, None)),
        }).then(|ctx: &VmContext, s: TcpStream| Ok(ctx.alloc_tcp_socket_slot(s) as i64)))
    }

    /// `Read(slot, buffer, offset, count) -> n` (0 = the peer closed).
    #[builtin(name = "__tcp_read")]
    pub fn tcp_read(ctx: &VmContext, slot: i64, buffer: Value, offset: i64, count: i64) -> Result<Then<i64>, BuiltinError> {
        let arr = byte_array(&buffer, "buffer")?;
        let (off, n) = check_buffer_range(offset, count, arr.borrow().len())?;
        let id = slot_id(slot)?;
        let s = stream(ctx, slot)?;
        let read = if n == 0 {
            Blocking::ready(Ok(Vec::new()))
        } else {
            Blocking::new(move || {
                let mut tmp = vec![0u8; n];
                Ok((&*s).read(&mut tmp).map(|got| { tmp.truncate(got); tmp }))
            })
        };
        Ok(read.then(move |ctx: &VmContext, r: std::io::Result<Vec<u8>>| {
            still_open(ctx, id)?;
            let bytes = r.map_err(|e| socket_error(&e, None))?;
            write_byte_range(&arr.borrow(), off, &bytes);
            Ok(bytes.len() as i64)
        }))
    }

    /// `Write(slot, buffer, offset, count)` — all of it (`write_all`).
    #[builtin(name = "__tcp_write")]
    pub fn tcp_write(ctx: &VmContext, slot: i64, buffer: &Value, offset: i64, count: i64) -> Result<Then<()>, BuiltinError> {
        let arr = byte_array(buffer, "buffer")?;
        let bytes = {
            let a = arr.borrow();
            let (off, n) = check_buffer_range(offset, count, a.len())?;
            read_byte_range(&a, off, n)?
        };
        let id = slot_id(slot)?;
        let s = stream(ctx, slot)?;
        let write = if bytes.is_empty() {
            Blocking::ready(Ok(()))
        } else {
            Blocking::new(move || Ok((&*s).write_all(&bytes)))
        };
        Ok(write.then(move |ctx: &VmContext, r: std::io::Result<()>| {
            still_open(ctx, id)?;
            r.map_err(|e| socket_error(&e, None))
        }))
    }

    /// Idempotent. Shuts the socket down so a blocked read / write wakes up.
    #[builtin(name = "__tcp_close")]
    pub fn tcp_close(ctx: &VmContext, slot: i64) {
        if let Ok(id) = u64::try_from(slot) {
            if let Some(s) = ctx.core.tcp_sockets.take(id) {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    /// Idempotent. Sets the closed flag first (a blocked accept sees it on its next poll),
    /// then takes the slot out; the fd closes with the last `Arc`.
    #[builtin(name = "__tcp_listener_close")]
    pub fn tcp_listener_close(ctx: &VmContext, slot: i64) {
        let Ok(id) = u64::try_from(slot) else { return };
        if let Some(l) = ctx.core.tcp_listeners.get_cloned(id) {
            l.closed.store(true, std::sync::atomic::Ordering::Release);
        }
        ctx.core.tcp_listeners.lock().remove(&id);
    }

    /// `millis <= 0` clears the timeout (blocking again).
    #[builtin(name = "__tcp_set_read_timeout")]
    pub fn tcp_set_read_timeout(ctx: &VmContext, slot: i64, millis: i64) -> Result<(), BuiltinError> {
        stream(ctx, slot)?.set_read_timeout(timeout(millis)).map_err(|e| socket_error(&e, None))
    }

    #[builtin(name = "__tcp_set_write_timeout")]
    pub fn tcp_set_write_timeout(ctx: &VmContext, slot: i64, millis: i64) -> Result<(), BuiltinError> {
        stream(ctx, slot)?.set_write_timeout(timeout(millis)).map_err(|e| socket_error(&e, None))
    }

    /// `TCP_NODELAY` (disable Nagle).
    #[builtin(name = "__tcp_set_nodelay")]
    pub fn tcp_set_nodelay(ctx: &VmContext, slot: i64, enable: bool) -> Result<(), BuiltinError> {
        stream(ctx, slot)?.set_nodelay(enable).map_err(|e| socket_error(&e, None))
    }

    /// `IP_TTL` (0..=255, else `ArgumentOutOfRangeException`, as .NET's `Socket.Ttl`).
    #[builtin(name = "__tcp_set_ttl")]
    pub fn tcp_set_ttl(ctx: &VmContext, slot: i64, ttl: i64) -> Result<(), BuiltinError> {
        let ttl = check_ttl(ttl)?;
        stream(ctx, slot)?.set_ttl(ttl).map_err(|e| socket_error(&e, None))
    }

    #[builtin(name = "__tcp_listener_set_ttl")]
    pub fn tcp_listener_set_ttl(ctx: &VmContext, slot: i64, ttl: i64) -> Result<(), BuiltinError> {
        let ttl = check_ttl(ttl)?;
        listener(ctx, slot)?.listener.set_ttl(ttl).map_err(|e| socket_error(&e, None))
    }

    /// `SO_KEEPALIVE` with the OS's idle / interval / probe defaults.
    #[builtin(name = "__tcp_set_keepalive")]
    pub fn tcp_set_keepalive(ctx: &VmContext, slot: i64, enable: bool) -> Result<(), BuiltinError> {
        let s = stream(ctx, slot)?;
        socket2::SockRef::from(&*s).set_keepalive(enable).map_err(|e| socket_error(&e, None))
    }

    /// `SO_KEEPALIVE` + idle / interval / probe count (seconds, each `>= 1`, else
    /// `ArgumentOutOfRangeException`). The probe count is ignored where the OS has no
    /// `TCP_KEEPCNT` (Windows); `enable = false` ignores the tuning arguments.
    #[builtin(name = "__tcp_set_keepalive_tuned")]
    pub fn tcp_set_keepalive_tuned(
        ctx: &VmContext, slot: i64, enable: bool, idle_secs: i64, interval_secs: i64, probes: i64,
    ) -> Result<(), BuiltinError> {
        let s = stream(ctx, slot)?;
        let sref = socket2::SockRef::from(&*s);
        if !enable {
            return sref.set_keepalive(false).map_err(|e| socket_error(&e, None));
        }
        for (v, name) in [(idle_secs, "idleSecs"), (interval_secs, "intervalSecs"), (probes, "probes")] {
            if v < 1 || v > u32::MAX as i64 {
                return Err(out_of_range(name));
            }
        }
        let secs = std::time::Duration::from_secs;
        crate::pal::net::set_tcp_keepalive(&s, secs(idle_secs as u64), secs(interval_secs as u64), probes as u32)
            .map_err(|e| socket_error(&e, None))
    }
}
