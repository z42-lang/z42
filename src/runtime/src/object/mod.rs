//! Values, object storage, layouts and managed handles.
//!
//! ```
//! use z42::object::ObjectProtocol;
//! let _: Option<ObjectProtocol<(), ()>> = None;
//! ```
//! ```compile_fail
//! use z42::object::header;
//! ```
//! ```compile_fail
//! use z42::object::protocol::ObjectProtocol;
//! ```
//! Storage consumers use the owning object contract:
//! ```
//! use z42::object::{ScriptObject, OwnedObject, NativeData, FieldWrite, Monitor};
//! let _: Option<OwnedObject> = None;
//! let _: Option<&ScriptObject> = None;
//! let _ = NativeData::None;
//! let _ = FieldWrite::NotRef;
//! let _: Option<Monitor> = None;
//! ```
//! ```compile_fail
//! use z42::object::object::ScriptObject;
//! ```
//! ```compile_fail
//! use z42::object::storage::ScriptObject;
//! ```

//! Heap access implementations stay private; consumers use the object contract.
//! ```compile_fail
//! use z42::object::heap_access::store_field_slot;
//! ```
//! ```compile_fail
//! use z42::object::struct_access::box_struct;
//! ```

mod protocol;
pub use protocol::ObjectProtocol;
pub mod identity;
pub mod value;
pub(crate) mod types;
pub(crate) mod header;
mod storage;
mod fields;
mod heap_access;
mod struct_access;
mod owned_object;
mod monitor;
mod reflected_type;
pub mod array;
pub mod string;

pub use value::*;
pub use types::*;
pub use header::{ArrRef, ObjRef, VarGcRef, WeakArrRef, WeakObjRef};
pub use storage::{NativeData, ScriptObject, ObjHeader};
#[cfg(all(feature = "jit", target_pointer_width = "64"))]
pub use storage::obj_jit_layout;
pub use fields::FieldWrite;
pub use heap_access::{store_field_access, store_field_slot, store_struct_leaf, store_struct_element_leaf, store_array_element, try_store_array_element, barrier_array_element, barrier_array_range};
pub use struct_access::{box_struct, snapshot_struct_box, snapshot_struct_element};
pub use owned_object::OwnedObject;
pub use monitor::Monitor;
pub use reflected_type::{ReflectedTypeHandle, ReflectedTypeShape, ReflectedTypeKey};
pub use array::*;
pub use string::Str;

#[cfg(all(test, z42_test_facade))]
mod tests;
