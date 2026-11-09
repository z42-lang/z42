//! Object storage and field access primitives.

mod object;
mod monitor;
pub use monitor::Monitor;
mod object_fields;
mod owned_object;
pub use object::*;
pub use object_fields::FieldWrite;
pub use owned_object::OwnedObject;

mod reflected_type;
pub use reflected_type::{ReflectedTypeHandle, ReflectedTypeShape};
pub use reflected_type::ReflectedTypeKey;
