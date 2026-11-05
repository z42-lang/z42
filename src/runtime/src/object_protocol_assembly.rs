//! Facade bindings for execution-owned object protocol capabilities.
use crate::vm::exec::object_protocol::ObjectProtocol;

pub(crate) fn services() -> ObjectProtocol {
    ObjectProtocol {
        get_type: crate::corelib::object::obj_get_type,
        default_to_string: crate::corelib::object::obj_to_str,
    }
}
