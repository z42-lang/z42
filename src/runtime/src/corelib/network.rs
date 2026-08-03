//! `Std.Net.Sockets` builtins — sync blocking TCP sockets.
//!
//! add-z42-net (K1, 2026-05-24): pattern mirrors `process.rs` (slot-id
//! handle + per-builtin slot lookup). All cross-platform differences
//! (BSD vs Winsock vs iOS/Android) are delegated to Rust
//! `std::net::{TcpStream, TcpListener}`.
//!
//! ## Return shape
//!
//! All builtins (except `*_drop`, which return `Value::Null`) return a
//! discriminated `Value::Array` tuple. The first element is always a
//! `KIND_*` tag so z42 decoding is uniform:
//!
//! ```text
//!   [I64(0), I64(slot)]                       // KIND_OK — connect / accept
//!   [I64(0), I64(slot), I64(actual_port)]     // KIND_OK — listen
//!   [I64(0), I64(nbytes)]                     // KIND_OK — read / write (0 = EOF)
//!   [I64(1), Str(message)]                    // KIND_SOCKET_ERR — io fail
//!   [I64(2)]                                  // KIND_HANDLE_INVALID — slot missing
//!   [I64(3)]                                  // KIND_UNSUPPORTED — wasm32
//! ```
//!
//! Drops always return `Value::Null` (idempotent; no error path needed).

use super::convert::{arg_i64, arg_str};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

pub(crate) const KIND_OK:              i64 = 0;
pub(crate) const KIND_SOCKET_ERR:      i64 = 1;
pub(crate) const KIND_HANDLE_INVALID:  i64 = 2;
#[cfg(target_arch = "wasm32")]
pub(crate) const KIND_UNSUPPORTED:     i64 = 3;

fn ok_value(ctx: &VmContext, v: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK), Value::I64(v)])
}

fn ok_two(ctx: &VmContext, a: i64, b: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK), Value::I64(a), Value::I64(b)])
}

/// add-z42-net-udp-multicast (2026-05-27): success tuple without payload
/// for void-shaped ops (multicast join/leave/set_loop).
fn ok_unit(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK)])
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
    use std::net::{TcpListener, TcpStream, SocketAddr, ToSocketAddrs};
    use std::io::{Read, Write};

    pub fn builtin_net_tcp_connect(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_connect";
        let host = arg_str(args, 0, NAME)?.to_string();
        let port = require_port(args, 1, NAME)?;

        let addr = format!("{}:{}", host, port);
        match TcpStream::connect(&addr) {
            Ok(stream) => {
                let slot_id = ctx.alloc_tcp_socket_slot(stream);
                Ok(ok_value(ctx, slot_id as i64))
            }
            Err(e) => Ok(socket_err(ctx, format!("connect to {}: {}", addr, e))),
        }
    }

    pub fn builtin_net_tcp_listen(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_listen";
        let host = arg_str(args, 0, NAME)?.to_string();
        let port = require_port(args, 1, NAME)?;

        let bind_target = format!("{}:{}", host, port);
        let bind_result = bind_target.to_socket_addrs()
            .and_then(|mut iter| iter.next()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no addresses")))
            .and_then(|addr: SocketAddr| TcpListener::bind(addr));

        match bind_result {
            Ok(listener) => {
                let actual_port = listener.local_addr()
                    .map(|a| a.port())
                    .unwrap_or(port);
                let slot_id = ctx.alloc_tcp_listener_slot(listener);
                Ok(ok_two(ctx, slot_id as i64, actual_port as i64))
            }
            Err(e) => Ok(socket_err(ctx, format!("bind {}: {}", bind_target, e))),
        }
    }

    pub fn builtin_net_tcp_accept(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_accept";
        let slot_id = require_slot_id(args, 0, NAME)?;

        // Take the listener out so `.accept()` can block without holding
        // the global listener table lock.
        let listener = {
            let mut map = ctx.core.tcp_listeners.lock();
            map.remove(&slot_id)
        };
        let Some(listener) = listener else {
            return Ok(handle_invalid(ctx));
        };

        let accept_result = listener.accept();
        // Put listener back so subsequent Accept calls work.
        ctx.core.tcp_listeners.lock().insert(slot_id, listener);

        match accept_result {
            Ok((stream, _peer)) => {
                let sock_slot = ctx.alloc_tcp_socket_slot(stream);
                Ok(ok_value(ctx, sock_slot as i64))
            }
            Err(e) => Ok(socket_err(ctx, format!("accept: {}", e))),
        }
    }

    pub fn builtin_net_tcp_socket_read(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_read";
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
            let mut map = ctx.core.tcp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(mut stream) = stream else {
            return Ok(handle_invalid(ctx));
        };

        let mut tmp = vec![0u8; count];
        let read_result = stream.read(&mut tmp);

        ctx.core.tcp_sockets.lock().insert(slot_id, stream);

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

    pub fn builtin_net_tcp_socket_write(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_write";
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
            let mut map = ctx.core.tcp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(mut stream) = stream else {
            return Ok(handle_invalid(ctx));
        };

        let write_result = stream.write_all(&tmp).map(|_| count);

        ctx.core.tcp_sockets.lock().insert(slot_id, stream);

        match write_result {
            Ok(n) => Ok(ok_value(ctx, n as i64)),
            Err(e) => Ok(socket_err(ctx, format!("write: {}", e))),
        }
    }

    pub fn builtin_net_tcp_socket_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_drop";
        let slot_id = require_slot_id(args, 0, NAME)?;
        ctx.core.tcp_sockets.lock().remove(&slot_id);
        Ok(Value::Null)
    }

    // add-httpclient-timeout (2026-05-27): apply read / write deadlines so
    // a misbehaving peer can't hang the script. `millis <= 0` clears the
    // timeout (blocking I/O). On error returns socket_err; on missing slot
    // returns handle_invalid (caller treats as already-disposed).

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
        let stream = {
            let map = ctx.core.tcp_sockets.lock();
            match map.get(&slot_id) {
                Some(s) => s.try_clone(),
                None => return Ok(handle_invalid(ctx)),
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return Ok(socket_err(ctx, format!("{}: try_clone: {}", which, e))),
        };
        let result = if which == "set_read_timeout" {
            stream.set_read_timeout(dur)
        } else {
            stream.set_write_timeout(dur)
        };
        match result {
            Ok(()) => Ok(ok_value(ctx, 0)),
            Err(e) => Ok(socket_err(ctx, format!("{}: {}", which, e))),
        }
    }

    pub fn builtin_net_tcp_socket_set_read_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_set_read_timeout";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let millis = arg_i64(args, 1, NAME)?;
        apply_timeout(ctx, slot_id, millis, "set_read_timeout")
    }

    pub fn builtin_net_tcp_socket_set_write_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_set_write_timeout";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let millis = arg_i64(args, 1, NAME)?;
        apply_timeout(ctx, slot_id, millis, "set_write_timeout")
    }

    pub fn builtin_net_tcp_listener_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_listener_drop";
        let slot_id = require_slot_id(args, 0, NAME)?;
        ctx.core.tcp_listeners.lock().remove(&slot_id);
        Ok(Value::Null)
    }

    // ── add-z42-net-socket-options (2026-05-27) ──────────────────────────

    /// `__net_tcp_socket_set_nodelay(slot, enable) -> [0] | err | invalid`
    /// Toggle TCP_NODELAY (disable Nagle's algorithm) on a connected socket.
    pub fn builtin_net_tcp_socket_set_nodelay(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_set_nodelay";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let enable = match args.get(1) {
            Some(Value::Bool(b)) => *b,
            Some(Value::I64(n))  => *n != 0,
            other => bail!("{}: arg 1 expected bool, got {:?}", NAME, other),
        };
        let stream = {
            let map = ctx.core.tcp_sockets.lock();
            match map.get(&slot_id) {
                Some(s) => s.try_clone(),
                None => return Ok(handle_invalid(ctx)),
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return Ok(socket_err(ctx, format!("set_nodelay: try_clone: {}", e))),
        };
        match stream.set_nodelay(enable) {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_nodelay: {}", e))),
        }
    }

    /// `__net_tcp_socket_set_ttl(slot, ttl) -> [0] | err | invalid`
    /// IP_TTL on the connected socket. 0 < ttl ≤ 255 typical.
    pub fn builtin_net_tcp_socket_set_ttl(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_set_ttl";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let ttl = arg_i64(args, 1, NAME)?;
        if ttl < 0 || ttl > u32::MAX as i64 {
            return Ok(socket_err(ctx, format!("set_ttl: value {} out of range", ttl)));
        }
        let stream = {
            let map = ctx.core.tcp_sockets.lock();
            match map.get(&slot_id) {
                Some(s) => s.try_clone(),
                None => return Ok(handle_invalid(ctx)),
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return Ok(socket_err(ctx, format!("set_ttl: try_clone: {}", e))),
        };
        match stream.set_ttl(ttl as u32) {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_ttl: {}", e))),
        }
    }

    /// `__net_tcp_listener_set_ttl(slot, ttl) -> [0] | err | invalid`
    pub fn builtin_net_tcp_listener_set_ttl(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_listener_set_ttl";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let ttl = arg_i64(args, 1, NAME)?;
        if ttl < 0 || ttl > u32::MAX as i64 {
            return Ok(socket_err(ctx, format!("set_ttl: value {} out of range", ttl)));
        }
        let map = ctx.core.tcp_listeners.lock();
        let listener = match map.get(&slot_id) {
            Some(l) => l,
            None => return Ok(handle_invalid(ctx)),
        };
        match listener.set_ttl(ttl as u32) {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_ttl: {}", e))),
        }
    }

    /// `__net_udp_set_ttl(slot, ttl) -> [0] | err | invalid` — unicast TTL.
    pub fn builtin_net_udp_set_ttl(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_set_ttl";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let ttl = arg_i64(args, 1, NAME)?;
        if ttl < 0 || ttl > u32::MAX as i64 {
            return Ok(socket_err(ctx, format!("set_ttl: value {} out of range", ttl)));
        }
        let map = ctx.core.udp_sockets.lock();
        let sock = match map.get(&slot_id) {
            Some(s) => s,
            None => return Ok(handle_invalid(ctx)),
        };
        match sock.set_ttl(ttl as u32) {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_ttl: {}", e))),
        }
    }

    // ── add-net-socket-options-extended (2026-05-30) ──────────────────────
    // Connect-with-timeout + SO_KEEPALIVE + SO_REUSEADDR. The first uses
    // std::net::TcpStream::connect_timeout; the latter two need cross-
    // platform setsockopt via the `socket2` crate (libc on Unix; Winsock
    // bindings on Windows).

    /// `__net_tcp_connect_with_timeout(host, port, millis) -> [0, slot] | err`
    /// `millis <= 0` is treated as "no preset" — fall back to a very long
    /// duration (matches BCL semantics where 0 means "infinite"). The
    /// stdlib wrapper only routes through this builtin when the user
    /// explicitly called `SetConnectTimeout(>0)`, so the long-fallback
    /// branch is a defensive guard, not a normal path.
    pub fn builtin_net_tcp_connect_with_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_connect_with_timeout";
        let host = arg_str(args, 0, NAME)?.to_string();
        let port = require_port(args, 1, NAME)?;
        let millis = arg_i64(args, 2, NAME)?;
        let dur = if millis > 0 {
            std::time::Duration::from_millis(millis as u64)
        } else {
            std::time::Duration::from_secs(u32::MAX as u64)
        };

        let addr = format!("{}:{}", host, port);
        let socket_addr = match addr.to_socket_addrs().and_then(|mut it| {
            it.next().ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable, "no addresses"))
        }) {
            Ok(a) => a,
            Err(e) => return Ok(socket_err(ctx, format!("connect to {}: {}", addr, e))),
        };
        match TcpStream::connect_timeout(&socket_addr, dur) {
            Ok(stream) => {
                let slot_id = ctx.alloc_tcp_socket_slot(stream);
                Ok(ok_value(ctx, slot_id as i64))
            }
            Err(e) => Ok(socket_err(ctx, format!(
                "connect to {} (timeout {}ms): {}", addr, millis, e))),
        }
    }

    /// `__net_tcp_socket_set_keepalive(slot, enable) -> [0] | err | invalid`
    /// Toggle SO_KEEPALIVE on a connected socket. OS default idle / interval
    /// / probe counts apply. For fine-grained tuning, see
    /// `__net_tcp_socket_set_keepalive_tuned`.
    pub fn builtin_net_tcp_socket_set_keepalive(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_set_keepalive";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let enable = match args.get(1) {
            Some(Value::Bool(b)) => *b,
            Some(Value::I64(n))  => *n != 0,
            other => bail!("{}: arg 1 expected bool, got {:?}", NAME, other),
        };
        let stream = {
            let map = ctx.core.tcp_sockets.lock();
            match map.get(&slot_id) {
                Some(s) => s.try_clone(),
                None => return Ok(handle_invalid(ctx)),
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return Ok(socket_err(ctx, format!("set_keepalive: try_clone: {}", e))),
        };
        let sref = socket2::SockRef::from(&stream);
        match sref.set_keepalive(enable) {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_keepalive: {}", e))),
        }
    }

    /// `__net_tcp_socket_set_keepalive_tuned(slot, enable, idle_secs,
    /// interval_secs, probes) -> [0] | err | invalid`
    ///
    /// Enable SO_KEEPALIVE and set the per-OS tuning parameters. Per
    /// socket2's `TcpKeepalive` (built with `feature = "all"`):
    ///   - `idle_secs`    — time before first keepalive probe (all
    ///                       Unix + Windows)
    ///   - `interval_secs` — time between successive probes (all Unix
    ///                       + Windows via WSAIoctl)
    ///   - `probes`        — number of failed probes before close.
    ///                       Available on Android / DragonFly / FreeBSD
    ///                       / Fuchsia / illumos / iOS / Linux / macOS
    ///                       / NetBSD / tvOS / visionOS / watchOS /
    ///                       Cygwin; silently ignored on Windows + the
    ///                       few platforms where socket2 omits the option.
    /// `enable = false` falls back to plain `set_keepalive(false)` and
    /// ignores the three tuning args. Caller passes the values in
    /// seconds (`>= 1`); zero / negative values throw.
    /// add-net-keepalive-tuning (2026-06-03).
    pub fn builtin_net_tcp_socket_set_keepalive_tuned(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_socket_set_keepalive_tuned";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let enable = match args.get(1) {
            Some(Value::Bool(b)) => *b,
            Some(Value::I64(n))  => *n != 0,
            other => bail!("{}: arg 1 expected bool, got {:?}", NAME, other),
        };
        let idle_secs = match args.get(2) {
            Some(Value::I64(n)) => *n,
            other => bail!("{}: arg 2 expected i64 idle_secs, got {:?}", NAME, other),
        };
        let interval_secs = match args.get(3) {
            Some(Value::I64(n)) => *n,
            other => bail!("{}: arg 3 expected i64 interval_secs, got {:?}", NAME, other),
        };
        let probes = match args.get(4) {
            Some(Value::I64(n)) => *n,
            other => bail!("{}: arg 4 expected i64 probes, got {:?}", NAME, other),
        };
        let stream = {
            let map = ctx.core.tcp_sockets.lock();
            match map.get(&slot_id) {
                Some(s) => s.try_clone(),
                None => return Ok(handle_invalid(ctx)),
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return Ok(socket_err(ctx, format!("set_keepalive_tuned: try_clone: {}", e))),
        };
        let sref = socket2::SockRef::from(&stream);
        if !enable {
            return match sref.set_keepalive(false) {
                Ok(()) => Ok(ok_unit(ctx)),
                Err(e) => Ok(socket_err(ctx, format!("set_keepalive_tuned: {}", e))),
            };
        }
        if idle_secs < 1 || interval_secs < 1 || probes < 1 {
            return Ok(socket_err(ctx, format!(
                "set_keepalive_tuned: idle_secs / interval_secs / probes must each be >= 1 (got {} / {} / {})",
                idle_secs, interval_secs, probes
            )));
        }
        let ka = socket2::TcpKeepalive::new()
            .with_time(std::time::Duration::from_secs(idle_secs as u64));
        // `with_interval` is supported on every Unix that socket2 exposes
        // (Linux/macOS/iOS/Android/*BSD/Solaris) and is a no-op call on
        // Windows since socket2 0.5 onward via WSAIoctl.
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
        let ka = ka.with_interval(std::time::Duration::from_secs(interval_secs as u64));
        // socket2 0.5 exposes `with_retries` on this set when feature = "all";
        // mirrors that gate so we don't accidentally drop on platforms where
        // the kernel actually supports TCP_KEEPCNT.
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
        let ka = ka.with_retries(probes as u32);
        let _ = interval_secs;
        let _ = probes;
        match sref.set_tcp_keepalive(&ka) {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_keepalive_tuned: {}", e))),
        }
    }

    /// `__net_tcp_listen_with_options(host, port, reuse_addr) -> [0, slot, actual_port] | err`
    /// Create a TcpListener with SO_REUSEADDR optionally set BEFORE bind
    /// (POSIX requires the option be applied pre-bind). When `reuse_addr`
    /// is false the result is observationally equivalent to plain
    /// `__net_tcp_listen` — the stdlib wrapper only routes through here
    /// when the user opts in with `SetReuseAddress(true)`.
    pub fn builtin_net_tcp_listen_with_options(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_tcp_listen_with_options";
        let host = arg_str(args, 0, NAME)?.to_string();
        let port = require_port(args, 1, NAME)?;
        let reuse_addr = match args.get(2) {
            Some(Value::Bool(b)) => *b,
            Some(Value::I64(n))  => *n != 0,
            other => bail!("{}: arg 2 expected bool, got {:?}", NAME, other),
        };

        let bind_target = format!("{}:{}", host, port);
        let socket_addr: SocketAddr = match bind_target.to_socket_addrs()
            .and_then(|mut it| it.next().ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable, "no addresses"))) {
            Ok(a) => a,
            Err(e) => return Ok(socket_err(ctx, format!("bind {}: {}", bind_target, e))),
        };

        let domain = if socket_addr.is_ipv4() {
            socket2::Domain::IPV4
        } else {
            socket2::Domain::IPV6
        };
        let sock = match socket2::Socket::new(domain, socket2::Type::STREAM, None) {
            Ok(s) => s,
            Err(e) => return Ok(socket_err(ctx, format!("bind {}: socket: {}", bind_target, e))),
        };
        if reuse_addr {
            if let Err(e) = sock.set_reuse_address(true) {
                return Ok(socket_err(ctx, format!("bind {}: set_reuse_address: {}", bind_target, e)));
            }
        }
        if let Err(e) = sock.bind(&socket_addr.into()) {
            return Ok(socket_err(ctx, format!("bind {}: {}", bind_target, e)));
        }
        if let Err(e) = sock.listen(128) {
            return Ok(socket_err(ctx, format!("bind {}: listen: {}", bind_target, e)));
        }
        let listener: TcpListener = sock.into();
        let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
        let slot_id = ctx.alloc_tcp_listener_slot(listener);
        Ok(ok_two(ctx, slot_id as i64, actual_port as i64))
    }

    // ── UDP builtins (add-z42-net-udp K2, 2026-05-25) ────────────────────
    use std::net::UdpSocket;

    /// `__net_udp_bind(host, port) -> [0, slot, actual_port] | err`
    pub fn builtin_net_udp_bind(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_bind";
        let host = arg_str(args, 0, NAME)?.to_string();
        let port = require_port(args, 1, NAME)?;
        let bind_target = format!("{}:{}", host, port);
        let bind_result = bind_target.to_socket_addrs()
            .and_then(|mut iter| iter.next()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no addresses")))
            .and_then(|addr: SocketAddr| UdpSocket::bind(addr));
        match bind_result {
            Ok(sock) => {
                let actual_port = sock.local_addr().map(|a| a.port()).unwrap_or(port);
                let slot_id = ctx.alloc_udp_socket_slot(sock);
                Ok(ok_two(ctx, slot_id as i64, actual_port as i64))
            }
            Err(e) => Ok(socket_err(ctx, format!("udp bind {}: {}", bind_target, e))),
        }
    }

    /// `__net_udp_send(slot, buf, offset, count, host, port) -> [0, n] | err | invalid`
    pub fn builtin_net_udp_send(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_send";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let buf_arr = match args.get(1) {
            Some(Value::Array(rc)) => rc.clone(),
            other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
        };
        let offset = arg_i64(args, 2, NAME)? as usize;
        let count  = arg_i64(args, 3, NAME)? as usize;
        let host   = arg_str(args, 4, NAME)?.to_string();
        let port   = require_port(args, 5, NAME)?;

        let buf_len = buf_arr.borrow().len();
        if offset + count > buf_len {
            bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
        }

        // Copy datagram bytes into a contiguous Vec<u8>.
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

        // Take socket out for the blocking call; restore after.
        let sock_opt = {
            let mut map = ctx.core.udp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(sock) = sock_opt else {
            return Ok(handle_invalid(ctx));
        };

        let send_result = sock.send_to(&tmp, format!("{}:{}", host, port).as_str());
        ctx.core.udp_sockets.lock().insert(slot_id, sock);

        match send_result {
            Ok(n) => Ok(ok_value(ctx, n as i64)),
            Err(e) => Ok(socket_err(ctx, format!("udp send: {}", e))),
        }
    }

    /// `__net_udp_recv(slot) -> [0, byte[] buf, remote_host_str, remote_port_i64] | err | invalid`
    pub fn builtin_net_udp_recv(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_recv";
        let slot_id = require_slot_id(args, 0, NAME)?;

        let sock_opt = {
            let mut map = ctx.core.udp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(sock) = sock_opt else {
            return Ok(handle_invalid(ctx));
        };

        // 65536 is large enough for any normal UDP datagram (incl IPv6 jumbo
        // up to 65507 payload + room for headers conceptually).
        let mut tmp = vec![0u8; 65536];
        let recv_result = sock.recv_from(&mut tmp);
        ctx.core.udp_sockets.lock().insert(slot_id, sock);

        match recv_result {
            Ok((n, peer)) => {
                // Build z42 byte[] sized to actual datagram length.
                let mut byte_vals: Vec<Value> = Vec::with_capacity(n);
                for i in 0..n {
                    byte_vals.push(Value::I64(tmp[i] as i64));
                }
                let buf_array = ctx.heap().alloc_array(byte_vals);
                let host_str = peer.ip().to_string();
                let port_i64 = peer.port() as i64;
                Ok(ctx.heap().alloc_array(vec![
                    Value::I64(KIND_OK),
                    buf_array,
                    Value::Str(host_str.into()),
                    Value::I64(port_i64),
                ]))
            }
            Err(e) => Ok(socket_err(ctx, format!("udp recv: {}", e))),
        }
    }

    /// `__net_udp_drop(slot) -> Null` — idempotent.
    pub fn builtin_net_udp_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_drop";
        let slot_id = require_slot_id(args, 0, NAME)?;
        ctx.core.udp_sockets.lock().remove(&slot_id);
        Ok(Value::Null)
    }

    // ── add-z42-net-udp-multicast (2026-05-27) ───────────────────────────
    use std::net::Ipv4Addr;

    /// Parse a dotted-quad string into Ipv4Addr.
    fn parse_ipv4(s: &str) -> std::result::Result<Ipv4Addr, String> {
        s.parse::<Ipv4Addr>().map_err(|e| format!("invalid IPv4 '{}': {}", s, e))
    }

    /// `__net_udp_join_multicast(slot, group_ip, iface_ip) -> Null | err | invalid`
    /// IPv4 only in v0. `iface_ip` of "0.0.0.0" lets the OS pick the default
    /// outgoing interface. Returns Null on success; KIND_ERR tuple on
    /// failure; KIND_INVALID when slot is already dropped.
    pub fn builtin_net_udp_join_multicast(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_join_multicast";
        let slot_id   = require_slot_id(args, 0, NAME)?;
        let group_str = arg_str(args, 1, NAME)?.to_string();
        let iface_str = arg_str(args, 2, NAME)?.to_string();

        let group_ip = match parse_ipv4(&group_str) {
            Ok(ip) => ip,
            Err(e) => return Ok(socket_err(ctx, e)),
        };
        if !group_ip.is_multicast() {
            return Ok(socket_err(ctx, format!("{} is not a multicast address", group_str)));
        }
        let iface_ip = match parse_ipv4(&iface_str) {
            Ok(ip) => ip,
            Err(e) => return Ok(socket_err(ctx, e)),
        };

        let sock_opt = {
            let mut map = ctx.core.udp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(sock) = sock_opt else {
            return Ok(handle_invalid(ctx));
        };

        let result = sock.join_multicast_v4(&group_ip, &iface_ip);
        ctx.core.udp_sockets.lock().insert(slot_id, sock);
        match result {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("join_multicast_v4 {}: {}", group_str, e))),
        }
    }

    /// `__net_udp_leave_multicast(slot, group_ip, iface_ip) -> Null | err | invalid`
    pub fn builtin_net_udp_leave_multicast(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_leave_multicast";
        let slot_id   = require_slot_id(args, 0, NAME)?;
        let group_str = arg_str(args, 1, NAME)?.to_string();
        let iface_str = arg_str(args, 2, NAME)?.to_string();

        let group_ip = match parse_ipv4(&group_str) {
            Ok(ip) => ip,
            Err(e) => return Ok(socket_err(ctx, e)),
        };
        let iface_ip = match parse_ipv4(&iface_str) {
            Ok(ip) => ip,
            Err(e) => return Ok(socket_err(ctx, e)),
        };

        let sock_opt = {
            let mut map = ctx.core.udp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(sock) = sock_opt else {
            return Ok(handle_invalid(ctx));
        };

        let result = sock.leave_multicast_v4(&group_ip, &iface_ip);
        ctx.core.udp_sockets.lock().insert(slot_id, sock);
        match result {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("leave_multicast_v4 {}: {}", group_str, e))),
        }
    }

    /// `__net_udp_set_multicast_loop(slot, enable) -> Null | err | invalid`
    /// When true (default), multicasts loop back to the same host on the
    /// joined group. Disabling can save a recv per send for senders that
    /// don't need to consume their own output.
    pub fn builtin_net_udp_set_multicast_loop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_set_multicast_loop";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let enable  = match args.get(1) {
            Some(Value::Bool(b)) => *b,
            Some(Value::I64(n))  => *n != 0,
            other => bail!("{}: arg 1 expected bool, got {:?}", NAME, other),
        };

        let sock_opt = {
            let mut map = ctx.core.udp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(sock) = sock_opt else {
            return Ok(handle_invalid(ctx));
        };

        let result = sock.set_multicast_loop_v4(enable);
        ctx.core.udp_sockets.lock().insert(slot_id, sock);
        match result {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("set_multicast_loop_v4: {}", e))),
        }
    }

    // ── add-z42-net-dns (2026-05-27) — synchronous DNS resolution ────────

    /// `__net_dns_lookup(host) -> [0, string[]] | err`
    /// Resolve `host` to a sorted-by-libc array of textual IP addresses
    /// (v4 + v6 mixed in whatever order the OS resolver returns; caller
    /// can filter by parsing each with IPAddress.Parse). Synchronous —
    /// blocks the calling thread for the duration of getaddrinfo.
    pub fn builtin_net_dns_lookup(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_dns_lookup";
        let host = arg_str(args, 0, NAME)?.to_string();
        // `to_socket_addrs` requires a port — append `:0`. The port in the
        // resulting addresses is ignored; we only emit the IP string.
        let probe = format!("{}:0", host);
        match probe.to_socket_addrs() {
            Ok(iter) => {
                let mut ip_strs: Vec<Value> = Vec::new();
                let mut seen: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                for addr in iter {
                    let s = addr.ip().to_string();
                    if seen.insert(s.clone()) {
                        ip_strs.push(Value::Str(s.into()));
                    }
                }
                let arr = ctx.heap().alloc_array(ip_strs);
                Ok(ctx.heap().alloc_array(vec![Value::I64(KIND_OK), arr]))
            }
            Err(e) => Ok(socket_err(ctx, format!("dns lookup {}: {}", host, e))),
        }
    }

    /// add-z42-net-udp-recv-into (2026-05-27)
    /// `__net_udp_recv_into(slot, buf, offset, count) -> [0, n, host, port] | err | invalid`
    /// Receive a datagram directly into the caller's pre-allocated byte[].
    /// Avoids the per-call array allocation that plain `__net_udp_recv` does.
    /// Truncates the datagram silently if it exceeds `count` (matches BCL
    /// `UdpClient.Receive(byte[])` semantics — caller decides buffer size).
    pub fn builtin_net_udp_recv_into(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_recv_into";
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

        let sock_opt = {
            let mut map = ctx.core.udp_sockets.lock();
            map.remove(&slot_id)
        };
        let Some(sock) = sock_opt else {
            return Ok(handle_invalid(ctx));
        };

        // Recv into scratch buffer then copy into the caller's array — the
        // borrow_mut on the Rc-wrapped Vec<Value> doesn't escape so this
        // never aliases.
        let mut tmp = vec![0u8; count];
        let recv_result = sock.recv_from(&mut tmp);
        ctx.core.udp_sockets.lock().insert(slot_id, sock);

        match recv_result {
            Ok((n, peer)) => {
                // Copy received bytes (clamped to count, matching socket
                // truncation behaviour) into the caller's array.
                let mut borrowed = buf_arr.borrow_mut();
                let written = n.min(count);
                for i in 0..written {
                    borrowed.set_boxed(offset + i, Value::I64(tmp[i] as i64));
                }
                let host_str = peer.ip().to_string();
                let port_i64 = peer.port() as i64;
                Ok(ctx.heap().alloc_array(vec![
                    Value::I64(KIND_OK),
                    Value::I64(written as i64),
                    Value::Str(host_str.into()),
                    Value::I64(port_i64),
                ]))
            }
            Err(e) => Ok(socket_err(ctx, format!("udp recv_into: {}", e))),
        }
    }

    // ── add-net-socket-options-extended (2026-05-30) ──────────────────────
    // UDP read / write timeout mirrors the TCP `apply_timeout` shape but
    // operates on the udp_sockets slot table. std::net::UdpSocket exposes
    // `set_read_timeout` / `set_write_timeout` natively.

    fn apply_udp_timeout(
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
        let map = ctx.core.udp_sockets.lock();
        let sock = match map.get(&slot_id) {
            Some(s) => s,
            None => return Ok(handle_invalid(ctx)),
        };
        let result = if which == "set_read_timeout" {
            sock.set_read_timeout(dur)
        } else {
            sock.set_write_timeout(dur)
        };
        match result {
            Ok(()) => Ok(ok_unit(ctx)),
            Err(e) => Ok(socket_err(ctx, format!("{}: {}", which, e))),
        }
    }

    pub fn builtin_net_udp_set_read_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_set_read_timeout";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let millis = arg_i64(args, 1, NAME)?;
        apply_udp_timeout(ctx, slot_id, millis, "set_read_timeout")
    }

    pub fn builtin_net_udp_set_write_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
        const NAME: &str = "__net_udp_set_write_timeout";
        let slot_id = require_slot_id(args, 0, NAME)?;
        let millis = arg_i64(args, 1, NAME)?;
        apply_udp_timeout(ctx, slot_id, millis, "set_write_timeout")
    }
}

// ── wasm32: all builtins return KIND_UNSUPPORTED tuple ────────────────────

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;

    pub fn builtin_net_tcp_connect(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_listen(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_accept(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_read(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_write(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_drop(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(Value::Null)
    }
    pub fn builtin_net_tcp_listener_drop(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(Value::Null)
    }

    // UDP wasm32 fallbacks
    pub fn builtin_net_udp_bind(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_send(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_recv(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_drop(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(Value::Null)
    }
    pub fn builtin_net_udp_recv_into(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_join_multicast(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_leave_multicast(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_set_multicast_loop(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_dns_lookup(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_set_nodelay(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_set_ttl(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_listener_set_ttl(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_set_ttl(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_set_read_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_set_write_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }

    // add-net-socket-options-extended (2026-05-30) wasm32 stubs
    pub fn builtin_net_tcp_connect_with_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_set_keepalive(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_socket_set_keepalive_tuned(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_tcp_listen_with_options(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_set_read_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
    pub fn builtin_net_udp_set_write_timeout(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(unsupported(ctx))
    }
}

pub use imp::*;

#[cfg(test)]
#[path = "network_tests.rs"]
mod network_tests;
