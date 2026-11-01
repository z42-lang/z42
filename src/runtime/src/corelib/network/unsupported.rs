//! Builds without sockets: the ABI v2 network / TLS builtins throw
//! `NetUnsupportedException : PlatformNotSupportedException` (closing is a no-op, as
//! there is never anything open). Same names and arities as `network/{tcp,udp}.rs` /
//! `tls.rs` — the stdlib declarations are shared.
//!
//! `net` stands in for `tcp` / `udp` when the `net` feature is off; `tls` stands in
//! for `corelib/tls.rs` when the `tls` feature is off (`tls` implies `net`, so a build
//! can have sockets without TLS but not the reverse). The wasm preset has neither.

#[cfg(not(feature = "net"))]
#[z42_macros::builtins]
pub mod net {
    use crate::corelib::abi::BuiltinError;
    use crate::corelib::network::error::unsupported;
    use crate::object::Value;

    #[builtin(name = "__tcp_connect")]
    pub fn tcp_connect(_a0: &Value, _a1: &Value, _a2: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_listen")]
    pub fn tcp_listen(_a0: &Value, _a1: &Value, _a2: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_listener_port")]
    pub fn tcp_listener_port(_a0: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_accept")]
    pub fn tcp_accept(_a0: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_read")]
    pub fn tcp_read(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_write")]
    pub fn tcp_write(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_close")]
    pub fn tcp_close(_a0: &Value) {}
    #[builtin(name = "__tcp_listener_close")]
    pub fn tcp_listener_close(_a0: &Value) {}
    #[builtin(name = "__tcp_set_read_timeout")]
    pub fn tcp_set_read_timeout(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_set_write_timeout")]
    pub fn tcp_set_write_timeout(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_set_nodelay")]
    pub fn tcp_set_nodelay(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_set_ttl")]
    pub fn tcp_set_ttl(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_listener_set_ttl")]
    pub fn tcp_listener_set_ttl(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_set_keepalive")]
    pub fn tcp_set_keepalive(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tcp_set_keepalive_tuned")]
    pub fn tcp_set_keepalive_tuned(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value, _a4: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_bind")]
    pub fn udp_bind(_a0: &Value, _a1: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_local_port")]
    pub fn udp_local_port(_a0: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_send")]
    pub fn udp_send(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value, _a4: &Value, _a5: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_recv")]
    pub fn udp_recv(_a0: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_recv_into")]
    pub fn udp_recv_into(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_close")]
    pub fn udp_close(_a0: &Value) {}
    #[builtin(name = "__udp_join_multicast")]
    pub fn udp_join_multicast(_a0: &Value, _a1: &Value, _a2: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_leave_multicast")]
    pub fn udp_leave_multicast(_a0: &Value, _a1: &Value, _a2: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_set_multicast_loop")]
    pub fn udp_set_multicast_loop(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_set_ttl")]
    pub fn udp_set_ttl(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_set_read_timeout")]
    pub fn udp_set_read_timeout(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__udp_set_write_timeout")]
    pub fn udp_set_write_timeout(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__dns_lookup")]
    pub fn dns_lookup(_a0: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
}

#[z42_macros::builtins]
pub mod tls {
    use crate::corelib::abi::BuiltinError;
    use crate::corelib::network::error::unsupported;
    use crate::object::Value;

    #[builtin(name = "__tls_connect")]
    pub fn tls_connect(_a0: &Value, _a1: &Value, _a2: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tls_read")]
    pub fn tls_read(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value) -> Result<Value, BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tls_write")]
    pub fn tls_write(_a0: &Value, _a1: &Value, _a2: &Value, _a3: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tls_close")]
    pub fn tls_close(_a0: &Value) {}
    #[builtin(name = "__tls_set_read_timeout")]
    pub fn tls_set_read_timeout(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
    #[builtin(name = "__tls_set_write_timeout")]
    pub fn tls_set_write_timeout(_a0: &Value, _a1: &Value) -> Result<(), BuiltinError> { Err(unsupported()) }
}
