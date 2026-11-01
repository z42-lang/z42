//! Object storage and field access primitives.

mod object;
mod object_fields;
mod owned_object;
pub use object::*;
pub use object_fields::FieldWrite;
pub use owned_object::OwnedObject;
