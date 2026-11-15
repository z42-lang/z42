//! Values, object storage, layouts and managed handles.
//!
//! ```
//! use z42::object::ObjectProtocol;
//! let _: Option<ObjectProtocol<(), ()>> = None;
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

mod protocol;
pub use protocol::ObjectProtocol;
pub mod value;
pub mod types;
pub mod header;
mod storage;
mod fields;
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
pub use owned_object::OwnedObject;
pub use monitor::Monitor;
pub use reflected_type::{ReflectedTypeHandle, ReflectedTypeShape, ReflectedTypeKey};
pub use array::*;
pub use string::Str;

#[cfg(all(test, z42_test_facade))]
mod tests;
