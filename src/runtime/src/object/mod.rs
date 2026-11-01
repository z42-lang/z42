//! Values, object storage, layouts and managed handles.

pub mod value;
pub mod types;
pub mod header;
pub mod object;
pub mod array;
pub mod string;

pub use value::*;
pub use types::*;
pub use header::{ArrRef, ObjRef, VarGcRef, WeakArrRef, WeakObjRef};
pub use object::*;
pub use array::*;
pub use string::Str;

#[cfg(test)]
mod types_tests;
