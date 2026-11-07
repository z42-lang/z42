//! Object protocol providers selected by the facade, with caller-owned materialization.
use crate::object::Value;
use crate::vm_context::VmContext;
use super::call::builtin_abi::BuiltinError;

/// Immutable per-core capabilities; function pointers retain no VM or managed values.
/// Keep the default string as Rust data: the execution caller owns its GC allocation
/// and error mapping, while `get_type` materializes reflection through its provider.
#[derive(Clone, Copy)]
pub(crate) struct ObjectProtocol {
    pub(crate) get_type: fn(&VmContext, &Value) -> Result<Value, BuiltinError>,
    pub(crate) default_to_string: fn(&Value) -> Result<String, BuiltinError>,
    pub(crate) make_type_from_name: fn(&VmContext, &str) -> Value,
    pub(crate) make_constructed_type: fn(&VmContext, &str, &[String]) -> Value,
}
