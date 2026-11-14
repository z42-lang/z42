//! Object-domain capabilities; callers supply their own context and error domain.
//!
//! The execution-layer protocol path is not an alternate entry:
//! ```compile_fail
//! use z42::exec::object_protocol::ObjectProtocol;
//! ```
use crate::object::Value;

/// Immutable per-core capabilities; function pointers own no context, error or
/// managed values. The caller owns default-text GC materialization and error mapping.
pub struct ObjectProtocol<Context, Error> {
    pub get_type: fn(&Context, &Value) -> Result<Value, Error>,
    pub default_to_string: fn(&Value) -> Result<String, Error>,
    pub make_type_from_name: fn(&Context, &str) -> Value,
    pub make_constructed_type: fn(&Context, &str, &[String]) -> Value,
}

// Function pointers are Copy independently of their parameter/result types.
impl<Context, Error> Copy for ObjectProtocol<Context, Error> {}
impl<Context, Error> Clone for ObjectProtocol<Context, Error> {
    fn clone(&self) -> Self { *self }
}
