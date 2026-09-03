//! `Std.Net` builtins — TCP 套接字选项（nodelay / ttl / keepalive）、带超时连接、带选项监听（非 wasm32）。refactor-split-network（2026-09-03）：自 `network.rs`
//! 内联 `mod imp` 逐行搬出；`KIND_*` / 句柄槽等共享定义仍在 hub（`use super::*`）。

#![allow(unused_imports)]
use super::*;
use std::net::{TcpListener, TcpStream, SocketAddr, ToSocketAddrs};
use std::io::{Read, Write};

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
