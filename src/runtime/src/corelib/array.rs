// 2026-05-07 add-array-base-class:
// Std.Array native bindings. v1 仅 `__array_clone`（浅拷贝）；元素是引用类型
// 时共享引用，与 C# `System.Array.Clone()` 语义一致。
// 2026-08-22 add-json-serde: reflective array create/get/set/length —— serde 反射建/读写
// T[]（元素类型只以运行期 Type 已知，无法静态 `new T[n]`）。System.Array parity。

use crate::corelib::convert::box_prim_to_heap;
use crate::corelib::reflection::read_obj_slot;
use crate::metadata::types::{default_value_for, ArrayObj};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// Normalize any element-type spelling (short alias / VM tag / FQ wrapper) to the
/// **short element tag** `ArrayObj::pack_backing` keys on. Reference types
/// (`string`, user classes) pass through → a reference (`Boxed`) backing.
fn elem_tag(name: &str) -> &str {
    match name {
        "sbyte" | "i8" | "Std.SByte" => "sbyte",
        "byte" | "u8" | "Std.Byte" => "byte",
        "short" | "i16" | "Std.Int16" => "short",
        "ushort" | "u16" | "Std.UInt16" => "ushort",
        "int" | "i32" | "Std.Int32" => "int",
        "uint" | "u32" | "Std.UInt32" => "uint",
        "long" | "i64" | "Std.Int64" => "long",
        "ulong" | "u64" | "Std.UInt64" => "ulong",
        "float" | "f32" | "Std.Single" => "float",
        "double" | "f64" | "Std.Double" => "double",
        "bool" | "Std.Boolean" => "bool",
        "char" | "Std.Char" => "char",
        other => other,
    }
}

/// The FQ wrapper name for an **integer** element tag (used to box a packed-int
/// element into a `Std.Int32`/… `BoxedStruct` for reflective GetValue). `None` for
/// non-integer tags (float/double/bool/char box to their own `Value` variant; refs
/// are already object-representation).
fn int_wrapper_fqn(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "sbyte" => "Std.SByte",
        "byte" => "Std.Byte",
        "short" => "Std.Int16",
        "ushort" => "Std.UInt16",
        "int" => "Std.Int32",
        "uint" => "Std.UInt32",
        "long" => "Std.Int64",
        "ulong" => "Std.UInt64",
        _ => return None,
    })
}

/// Read the element-type name off a reflective `Std.Type` receiver — `__fullName`
/// (FQ, e.g. "Std.Int32" / "Demo.MyClass") first, then `__name` fallback.
fn type_name_of(v: &Value) -> Option<String> {
    for slot in ["__fullName", "__name"] {
        if let Value::Str(s) = read_obj_slot(v, slot) {
            return Some(s.to_string());
        }
    }
    None
}

/// `__array_create(elemType: Type, n: int) -> object` — allocate an `elemType[]` of
/// length `n`, default-initialised. Primitive element types pack (int→I32 backing,
/// etc.); reference types get a `Boxed` backing. The array carries its short element
/// tag so `GetType().GetElementType()` round-trips. add-json-serde.
pub fn builtin_array_create(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let elem = args.first().cloned().unwrap_or(Value::Null);
    let n = match args.get(1) {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        _ => bail!("Array.CreateInstance: expected (Type, non-negative int)"),
    };
    let name = type_name_of(&elem)
        .ok_or_else(|| anyhow::anyhow!("Array.CreateInstance: element type has no name"))?;
    let tag = elem_tag(&name);
    let default = default_value_for(tag);
    let elems = vec![default; n];
    Ok(ctx.heap().alloc_array_typed(tag, elems))
}

/// `__array_length(arr: object) -> int`. add-json-serde.
pub fn builtin_array_length(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::Array(rc)) => Ok(Value::I64(rc.borrow().len() as i64)),
        Some(Value::Null) => bail!("Array.GetLength: null array reference"),
        other => bail!("Array.GetLength: expected an array, got {:?}", other),
    }
}

/// `__array_get(arr: object, i: int) -> object` — read element `i` as an object.
/// Packed integers are boxed to the matching wrapper (`BoxedStruct`); double / bool /
/// char / reference elements are already object-representation. add-json-serde.
pub fn builtin_array_get(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let rc = match args.first() {
        Some(Value::Array(rc)) => rc.clone(),
        Some(Value::Null) => bail!("Array.GetValue: null array reference"),
        other => bail!("Array.GetValue: expected an array, got {:?}", other),
    };
    let i = match args.get(1) {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        _ => bail!("Array.GetValue: expected a non-negative index"),
    };
    let (raw, tag) = {
        let a = rc.borrow();
        if i >= a.len() {
            bail!("Array.GetValue: index {i} out of bounds (len {})", a.len());
        }
        (a.get_boxed(i), elem_tag(&a.element_type).to_string())
    };
    match (int_wrapper_fqn(&tag), &raw) {
        (Some(fqn), Value::I64(n)) => box_prim_to_heap(ctx, fqn, *n),
        _ => Ok(raw),
    }
}

/// `__array_set(arr: object, i: int, value: object) -> void` — write `value` into
/// element `i`, unboxing a boxed primitive into the packed slot. add-json-serde.
pub fn builtin_array_set(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let rc = match args.first() {
        Some(Value::Array(rc)) => rc.clone(),
        Some(Value::Null) => bail!("Array.SetValue: null array reference"),
        other => bail!("Array.SetValue: expected an array, got {:?}", other),
    };
    let i = match args.get(1) {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        _ => bail!("Array.SetValue: expected a non-negative index"),
    };
    let value = args.get(2).cloned().unwrap_or(Value::Null);
    // Unbox a boxed integer primitive to its raw `I64` so packed backings store it;
    // non-boxed values (F64 / Bool / Char / Str / Object) pass through to `set_boxed`.
    let raw = match value {
        Value::BoxedStruct(s) => match s.borrow().boxed_prim_i64() {
            Some(n) => Value::I64(n),
            None => Value::BoxedStruct(s),
        },
        other => other,
    };
    let mut a = rc.borrow_mut();
    if i >= a.len() {
        bail!("Array.SetValue: index {i} out of bounds (len {})", a.len());
    }
    a.set_boxed(i, raw);
    Ok(Value::Null)
}

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
