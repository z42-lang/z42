//! Reflection builtins — read-only type introspection backing `Std.Type` and
//! `Std.Reflection.{FieldInfo,MethodInfo,ParameterInfo}` (add-reflection-mvp,
//! 2026-06-08).
//!
//! Design (see docs/spec/.../add-reflection-mvp/design.md):
//!   - `Std.Type` objects carry the real `Arc<TypeDesc>` in
//!     `NativeData::TypeHandle` (set by `__obj_get_type`). Reflection builtins
//!     read it to enumerate members.
//!   - Member/Type objects are populated EAGERLY: each builtin allocates the
//!     real z42 class (`Std.Reflection.FieldInfo`, …) via `try_lookup_type` and
//!     fills slots by name through `field_index`.
//!   - All builtins take the reflected object as `args[0]` and are LENIENT:
//!     a synthetic Type (primitive/array, no handle) yields empty arrays / null,
//!     never `bail!` (mirrors C# returning empty results).
//!   - Method signatures (params/return/static) are read on demand from the
//!     method's `Function` via `ctx.try_lookup_function` — no persisted
//!     per-type method table, no wire-format change.
//! Split into concern submodules (refactor-reflection-split): the single
//! `reflection.rs` (2840 lines) exceeded the 500-line hard cap. mod.rs is a
//! thin hub — imports + shared consts + a private re-glob of each concern
//! module (so siblings see each other via `use super::*`) + `pub use` to keep
//! the public API (`reflection::builtin_*` / `make_type_*` / `read_obj_slot`).
//!

use crate::interp::{exec_function, exec_function_with_type_args, ExecOutcome};
use crate::metadata::{well_known_names, NativeData, TypeDesc, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

const STD_OBJECT: &str = "Std.Object";
const STD_REFLECTION_FIELDINFO: &str = "Std.Reflection.FieldInfo";
const STD_REFLECTION_METHODINFO: &str = "Std.Reflection.MethodInfo";
const STD_REFLECTION_CONSTRUCTORINFO: &str = "Std.Reflection.ConstructorInfo";
const STD_REFLECTION_PARAMINFO: &str = "Std.Reflection.ParameterInfo";
const STD_REFLECTION_PROPERTYINFO: &str = "Std.Reflection.PropertyInfo";

mod type_object;
mod fields;
mod attributes;
mod methods;
mod properties;
mod generics;
mod enums;
mod type_query;
mod invoke;
mod accessors;
mod module_load;

pub use self::type_object::*;
pub use self::fields::*;
pub use self::attributes::*;
pub use self::methods::*;
pub use self::properties::*;
pub use self::generics::*;
pub use self::enums::*;
pub use self::type_query::*;
pub use self::invoke::*;
pub use self::accessors::*;
pub use self::module_load::*;

#[cfg(test)]
#[path = "reflection_tests.rs"]
mod reflection_tests;
