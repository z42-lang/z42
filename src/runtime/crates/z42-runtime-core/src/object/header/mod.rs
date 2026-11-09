//! Managed block headers, mark bits and tagged handles.

pub mod marks;
pub mod obj_ref;
pub mod arr_ref;
pub mod block;
pub mod var_ref;
pub use marks::*;
pub use obj_ref::{ObjRef, WeakObjRef};
pub use arr_ref::{ArrRef, WeakArrRef};
pub use block::*;
pub use var_ref::*;
