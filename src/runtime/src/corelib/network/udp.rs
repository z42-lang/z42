//! `Std.Net.Sockets` UDP + DNS builtins (builtin ABI v2): `UdpSocketNative` /
//! `DnsNative.Resolve` in z42.core, used by z42.net's `UdpClient` / `Dns`.
//!
//! Same shape as `network/tcp.rs`: slot ids into `VmCore.udp_sockets`, the `Arc` cloned
//! out of the table, failures thrown from Rust (`network/error.rs`), every call that can
//! block (resolution, send, receive) parked, results built after unparking.

use super::error::{check_port, check_ttl, closed, resolve, slot_id, socket_error, socket_exception, timeout};
use super::error::code;
use crate::corelib::abi::BuiltinError;
use crate::vm_context::VmContext;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;

/// Receive buffer for `Receive()`: room for any datagram (IPv4 / IPv6 payloads stay below 64 KiB).
const MAX_DATAGRAM: usize = 65_536;

fn socket(ctx: &VmContext, slot: i64) -> Result<Arc<UdpSocket>, BuiltinError> {
    ctx.core.udp_sockets.get_cloned(slot_id(slot)?).ok_or_else(closed)
}

/// `[n-or-bytes, remoteHost, remotePort]` — what both receive builtins return.
fn received(ctx: &VmContext, first: crate::object::Value, peer: SocketAddr) -> crate::object::Value {
    use crate::object::Value;
    ctx.heap().alloc_array(vec![first, Value::Str(peer.ip().to_string().into()), Value::I64(peer.port() as i64)])
}

/// A dotted-quad multicast group / interface argument. Not an IPv4 address →
/// `FormatException` (.NET's `IPAddress.Parse` text).
fn ipv4(s: &str) -> Result<Ipv4Addr, BuiltinError> {
    s.parse::<Ipv4Addr>().map_err(|_| BuiltinError::Format("An invalid IP address was specified.".into()))
}

fn multicast_group(group: &str) -> Result<Ipv4Addr, BuiltinError> {
    let ip = ipv4(group)?;
    if !ip.is_multicast() {
        return Err(socket_exception(code::INVALID_ARGUMENT, format!(
            "An invalid argument was supplied: {group} is not a multicast address.")));
    }
    Ok(ip)
}

#[z42_macros::builtins]
pub mod typed {
    use super::*;
    use crate::corelib::abi::{Blocking, Then};
    use crate::corelib::fs::{byte_array, check_buffer_range, read_byte_range, write_byte_range};
    use crate::object::Value;

    /// `Bind(host, port) -> slot` (port 0 = the OS picks).
    #[builtin(name = "__udp_bind")]
    pub fn udp_bind(host: String, port: i64) -> Result<Then<i64>, BuiltinError> {
        let port = check_port(port)?;
        Ok(Blocking::new(move || {
            let addr = resolve(&host, port)?[0];
            UdpSocket::bind(addr).map_err(|e| socket_error(&e, None))
        }).then(|ctx: &VmContext, s: UdpSocket| Ok(ctx.alloc_udp_socket_slot(s) as i64)))
    }

    /// The port the socket is bound to.
    #[builtin(name = "__udp_local_port")]
    pub fn udp_local_port(ctx: &VmContext, slot: i64) -> Result<i64, BuiltinError> {
        socket(ctx, slot)?.local_addr().map(|a| a.port() as i64).map_err(|e| socket_error(&e, None))
    }

    /// `Send(slot, buf, offset, count, host, port) -> bytes sent`.
    #[builtin(name = "__udp_send")]
    pub fn udp_send(
        ctx: &VmContext, slot: i64, buffer: &Value, offset: i64, count: i64, host: String, port: i64,
    ) -> Result<Blocking<i64>, BuiltinError> {
        let arr = byte_array(buffer, "buffer")?;
        let bytes = {
            let a = arr.borrow();
            let (off, n) = check_buffer_range(offset, count, a.len())?;
            read_byte_range(&a, off, n)?
        };
        let port = check_port(port)?;
        let s = socket(ctx, slot)?;
        Ok(Blocking::new(move || {
            let to = resolve(&host, port)?[0];
            s.send_to(&bytes, to).map(|n| n as i64).map_err(|e| socket_error(&e, None))
        }))
    }

    /// `Receive(slot) -> [byte[] datagram, remoteHost, remotePort]`.
    #[builtin(name = "__udp_recv")]
    pub fn udp_recv(ctx: &VmContext, slot: i64) -> Result<Then<Value>, BuiltinError> {
        let s = socket(ctx, slot)?;
        Ok(Blocking::new(move || {
            let mut tmp = vec![0u8; MAX_DATAGRAM];
            let (n, peer) = s.recv_from(&mut tmp).map_err(|e| socket_error(&e, None))?;
            tmp.truncate(n);
            Ok((tmp, peer))
        }).then(|ctx: &VmContext, (bytes, peer): (Vec<u8>, SocketAddr)| {
            let data = ctx.heap().alloc_bytes(bytes);
            Ok(received(ctx, data, peer))
        }))
    }

    /// `ReceiveInto(slot, buf, offset, count) -> [n, remoteHost, remotePort]`; a datagram
    /// longer than `count` is truncated to it. The OS receive always gets a full-datagram
    /// buffer: into a short one, Unix truncates silently but Windows fails the receive
    /// with `WSAEMSGSIZE` and drops the peer address.
    #[builtin(name = "__udp_recv_into")]
    pub fn udp_recv_into(ctx: &VmContext, slot: i64, buffer: Value, offset: i64, count: i64) -> Result<Then<Value>, BuiltinError> {
        thread_local! {
            static SCRATCH: std::cell::RefCell<Vec<u8>> = std::cell::RefCell::new(vec![0u8; MAX_DATAGRAM]);
        }
        let arr = byte_array(&buffer, "buffer")?;
        let (off, n) = check_buffer_range(offset, count, arr.borrow().len())?;
        let s = socket(ctx, slot)?;
        Ok(Blocking::new(move || {
            SCRATCH.with_borrow_mut(|tmp| {
                let (got, peer) = s.recv_from(tmp).map_err(|e| socket_error(&e, None))?;
                Ok((tmp[..got.min(n)].to_vec(), peer))
            })
        }).then(move |ctx: &VmContext, (bytes, peer): (Vec<u8>, SocketAddr)| {
            write_byte_range(&arr.borrow(), off, &bytes);
            Ok(received(ctx, Value::I64(bytes.len() as i64), peer))
        }))
    }

    /// Idempotent.
    #[builtin(name = "__udp_close")]
    pub fn udp_close(ctx: &VmContext, slot: i64) {
        if let Ok(id) = u64::try_from(slot) {
            ctx.core.udp_sockets.lock().remove(&id);
        }
    }

    /// IPv4 multicast membership; `iface` `0.0.0.0` = the OS's default interface.
    #[builtin(name = "__udp_join_multicast")]
    pub fn udp_join_multicast(ctx: &VmContext, slot: i64, group: &str, iface: &str) -> Result<(), BuiltinError> {
        let (g, i) = (multicast_group(group)?, ipv4(iface)?);
        socket(ctx, slot)?.join_multicast_v4(&g, &i).map_err(|e| socket_error(&e, None))
    }

    #[builtin(name = "__udp_leave_multicast")]
    pub fn udp_leave_multicast(ctx: &VmContext, slot: i64, group: &str, iface: &str) -> Result<(), BuiltinError> {
        let (g, i) = (ipv4(group)?, ipv4(iface)?);
        socket(ctx, slot)?.leave_multicast_v4(&g, &i).map_err(|e| socket_error(&e, None))
    }

    /// `IP_MULTICAST_LOOP`.
    #[builtin(name = "__udp_set_multicast_loop")]
    pub fn udp_set_multicast_loop(ctx: &VmContext, slot: i64, enable: bool) -> Result<(), BuiltinError> {
        socket(ctx, slot)?.set_multicast_loop_v4(enable).map_err(|e| socket_error(&e, None))
    }

    /// Unicast `IP_TTL` (0..=255).
    #[builtin(name = "__udp_set_ttl")]
    pub fn udp_set_ttl(ctx: &VmContext, slot: i64, ttl: i64) -> Result<(), BuiltinError> {
        let ttl = check_ttl(ttl)?;
        socket(ctx, slot)?.set_ttl(ttl).map_err(|e| socket_error(&e, None))
    }

    /// `millis <= 0` clears the timeout.
    #[builtin(name = "__udp_set_read_timeout")]
    pub fn udp_set_read_timeout(ctx: &VmContext, slot: i64, millis: i64) -> Result<(), BuiltinError> {
        socket(ctx, slot)?.set_read_timeout(timeout(millis)).map_err(|e| socket_error(&e, None))
    }

    #[builtin(name = "__udp_set_write_timeout")]
    pub fn udp_set_write_timeout(ctx: &VmContext, slot: i64, millis: i64) -> Result<(), BuiltinError> {
        socket(ctx, slot)?.set_write_timeout(timeout(millis)).map_err(|e| socket_error(&e, None))
    }

    /// `Dns.GetHostAddresses(host)`: the addresses as text, in resolver order, without
    /// duplicates. Failure → `SocketException(HostNotFound)`.
    #[builtin(name = "__dns_lookup", blocking)]
    pub fn dns_lookup(host: String) -> Result<Vec<String>, BuiltinError> {
        let mut seen = std::collections::HashSet::new();
        Ok(resolve(&host, 0)?.into_iter()
            .map(|a| a.ip().to_string())
            .filter(|s| seen.insert(s.clone()))
            .collect())
    }
}
