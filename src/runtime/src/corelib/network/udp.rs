//! `Std.Net` builtins — UDP 收发 / 组播 / DNS 同步解析 / UDP 超时（非 wasm32）。refactor-split-network（2026-09-03）：自 `network.rs`
//! 内联 `mod imp` 逐行搬出；`KIND_*` / 句柄槽等共享定义仍在 hub（`use super::*`）。

#![allow(unused_imports)]
use super::*;
use std::net::{TcpListener, TcpStream, SocketAddr, ToSocketAddrs};
use std::io::{Read, Write};

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
        // packed-primitive-arrays Step 3: packed `Bytes` → one memcpy.
        if let Some(b) = borrowed.as_bytes() {
            tmp.copy_from_slice(&b[offset..offset + count]);
        } else {
            for i in 0..count {
                match borrowed.get_boxed(offset + i) {
                    Value::I64(v) => tmp[i] = (v & 0xFF) as u8,
                    other => bail!("{}: byte[] elem at {} expected I64, got {:?}", NAME, offset + i, other),
                }
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
