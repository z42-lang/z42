//! Object protocol providers selected by the facade, with caller-owned materialization.
use crate::object::Value;
use crate::context::VmContext;
use super::call::builtin_abi::BuiltinError;

/// Immutable per-core capabilities; function pointers retain no VM or managed values.
/// Keep the default string as Rust data: the execution caller owns its GC allocation
/// and error mapping, while `get_type` materializes reflection through its provider.
#[derive(Clone, Copy)]
pub struct ObjectProtocol {
    pub get_type: fn(&VmContext, &Value) -> Result<Value, BuiltinError>,
    pub default_to_string: fn(&Value) -> Result<String, BuiltinError>,
    pub make_type_from_name: fn(&VmContext, &str) -> Value,
    pub make_constructed_type: fn(&VmContext, &str, &[String]) -> Value,
}
