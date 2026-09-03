//! wasm32 桩：全部返回 `KIND_UNSUPPORTED`（refactor-split-network，自 `network.rs` 内联 `mod imp` 搬出）。

#![allow(unused_imports)]
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
