//! Type shapes, field tags and pure layout operations.

mod field;
mod type_desc;
mod layout;
mod elem_type;
mod type_args;
pub use field::*;
pub use type_desc::*;
pub use layout::*;
pub use elem_type::*;
pub use type_args::{TypeArgs, TypeArgsInfo};
