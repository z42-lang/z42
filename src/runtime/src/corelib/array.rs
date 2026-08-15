// 2026-05-07 add-array-base-class:
// Std.Array native bindings. v1 仅 `__array_clone`（浅拷贝）；元素是引用类型
// 时共享引用，与 C# `System.Array.Clone()` 语义一致。

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// `Std.Array.Clone()` — shallow copy of the receiver array. Reference-type
/// elements are shared (the new array's slots reference the same heap objects).
/// Empty arrays return another empty array (not the same reference).
pub fn builtin_array_clone(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        bail!("__array_clone: expected 1 argument (this), got {}", args.len());
    }
    match &args[0] {
        Value::Array(rc) => {
            // unify-gc-heap PR-3: value-semantic array copy — `deep_copy` allocates a
            // fresh element block in the GC heap and clones the elements in (reference
            // elements stay shared: cloning a `Value::Object`/`Array` clones the handle).
            // Region-alloc the new header via the heap (not the leaking `GcRef::new`).
            let copy = rc.borrow().deep_copy(ctx.heap());
            Ok(ctx.heap().alloc_array_obj(copy))
        }
        Value::Null => bail!("__array_clone: null array reference"),
        other => bail!("__array_clone: expected an array, got {:?}", other),
    }
}

#[cfg(test)]
#[path = "array_tests.rs"]
mod array_tests;
