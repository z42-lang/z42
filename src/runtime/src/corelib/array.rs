// 2026-05-07 add-array-base-class:
// Std.Array native bindings. v1 仅 `__array_clone`（浅拷贝）；元素是引用类型
// 时共享引用，与 C# `System.Array.Clone()` 语义一致。
// 2026-08-22 add-json-serde: reflective array create/get/set/length —— serde 反射建/读写
// T[]（元素类型只以运行期 Type 已知，无法静态 `new T[n]`）。System.Array parity。

use crate::corelib::convert::box_prim_to_heap;
use crate::corelib::raise_op;
use crate::corelib::reflection::read_obj_slot;
use crate::metadata::types::{default_value_for, ArrayObj};
use crate::metadata::Value;
use crate::objops;
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
    crate::corelib::expect_args("Array.CreateInstance", args, 2)?;
    let elem = args[0].clone();
    let n = match args.get(1) {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        _ => bail!("Array.CreateInstance: expected (Type, non-negative int)"),
    };
    let name = type_name_of(&elem)
        .ok_or_else(|| anyhow::anyhow!("Array.CreateInstance: element type has no name"))?;
    let tag = elem_tag(&name);
    let default = default_value_for(tag);
    let heap = ctx.heap();
    let et = crate::metadata::types::ElemType::intern(tag);
    // 值 struct 元素类型 → 真正的 struct[]（`StructBytes`，元素零初始化），与 `new P[n]` 同一个闸门。
    if let Some(sb) = objops::array::try_struct_backed(ctx, et, n) {
        return Ok(heap.alloc_array_obj(sb));
    }
    // perf-array-alloc-direct: default-fill straight into the GC block.
    Ok(heap.alloc_array_obj(ArrayObj::typed_filled(heap, et, n, default)))
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
    let tag = {
        let a = rc.borrow();
        if i >= a.len() {
            bail!("Array.GetValue: index {i} out of bounds (len {})", a.len());
        }
        elem_tag(&a.element_type).to_string()
    };
    // 值 struct 数组的元素装箱成快照（`get_boxed` 没有堆，装不了箱）。
    let raw = objops::array_bulk::elem_get_boxed(ctx, &rc, i).map_err(|e| raise_op(ctx, e))?;
    match (int_wrapper_fqn(&tag), &raw) {
        (Some(fqn), Value::I64(n)) => box_prim_to_heap(ctx, fqn, *n),
        _ => Ok(raw),
    }
}

/// `__array_set(arr: object, value: object, i: int) -> void` — write `value` into
/// element `i`, unboxing a boxed primitive into the packed slot. Arg order mirrors
/// C# `Array.SetValue(object value, int index)` (value first) as an instance method
/// (`this`=array at args[0]). add-array-property-reflection-api (was value-last).
pub fn builtin_array_set(ctx: &VmContext, args: &[Value]) -> Result<()> {
    // split-null-sentinel-channels ⑥：显式校验 arity，之后 `args[1]` 直接下标
    // （此前是 `args.get(1).unwrap_or(Value::Null)` ⇒ 少传参数与传 null 无法区分）。
    crate::corelib::expect_args("Array.SetValue", args, 3)?;
    let rc = match args.first() {
        Some(Value::Array(rc)) => rc.clone(),
        Some(Value::Null) => bail!("Array.SetValue: null array reference"),
        other => bail!("Array.SetValue: expected an array, got {:?}", other),
    };
    let value = args[1].clone();
    let i = match args.get(2) {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        _ => bail!("Array.SetValue: expected a non-negative index"),
    };
    // Unbox a boxed integer primitive to its raw `I64` so packed backings store it;
    // non-boxed values (F64 / Bool / Char / Str / Object) pass through to `set_boxed`.
    let raw = match value {
        Value::BoxedStruct(s) => match s.borrow().boxed_prim_i64() {
            Some(n) => Value::I64(n),
            None => Value::BoxedStruct(s),
        },
        other => other,
    };
    {
        let mut a = rc.borrow_mut();
        if i >= a.len() {
            bail!("Array.SetValue: index {i} out of bounds (len {})", a.len());
        }
        // 值不是元素类型（基元不同种 / 值 struct 数组收到别的东西）→ InvalidCastException（同 C#），
        // 数组不动。异常对象要分配 ⇒ 先放锁再抛。
        if let Err(e) = objops::array_bulk::check_untyped_store(&a, &raw) {
            drop(a);
            return Err(raise_op(ctx, e));
        }
        // fix-silent-array-elem-zero：走会报错的那版。此前是 `set_boxed`（release 静默存 0）
        // ⇒ `a.SetValue(objNull, 0)` 把 `int[0]` 从 9 变成 0、不抛、报成功。
        // 形参声明是 `Object` ⇒ 没有编译器站点能先转换，值是**用户**给的
        // （与 IR `ArraySet` 那条路不同，那里 debug-only 的原策略依然正确）。
        a.try_set_boxed(i, raw.clone())
            .map_err(|e| anyhow::anyhow!("Array.SetValue: {e}"))?;
    }
    // fix-missing-array-write-barriers (2026-09-10): storing a reference into an array is a
    // heap write like any other, and an **old** array receiving a **young** element must mark
    // its card or the next minor will not re-root it. The interpreter's `ArraySet` and the
    // JIT's array-store helper both fire this; these `Std.Array` builtins never did.
    if raw.is_heap_ref() {
        ctx.heap().write_barrier_array_elem(&Value::Array(rc), i, &raw);
    }
    Ok(())
}

/// `__array_copy(src, srcIndex, dst, dstIndex, length)` — bulk element move,
/// mirroring `System.Array.Copy(Array, int, Array, int, int)`. perf-bulk-array-copy:
/// the script-side `Array.Copy<T>` was a per-element `for` loop, so every copied
/// element paid an interpreted `ArrayGet` + `ArraySet`. This is the same
/// "one bulk primitive, the algorithm stays in script" shape as `__str_to_chars`
/// / `__str_substring`: it copies a range and nothing else.
///
/// The element semantics — every backing incl. value-struct arrays, boxing / unboxing,
/// type checks (`ArrayTypeMismatchException` / `InvalidCastException`), overlap, write
/// barrier — live in `objops::array_bulk::copy_range`; this only parses the arguments.
pub fn builtin_array_copy(ctx: &VmContext, args: &[Value]) -> Result<()> {
    fn arr(v: Option<&Value>, what: &str) -> Result<crate::gc::GcRef<ArrayObj>> {
        match v {
            Some(Value::Array(rc)) => Ok(rc.clone()),
            Some(Value::Null) => bail!("__array_copy: null {what} array reference"),
            other => bail!("__array_copy: expected an array for {what}, got {other:?}"),
        }
    }
    fn idx(v: Option<&Value>, what: &str) -> Result<usize> {
        match v {
            Some(Value::I64(n)) if *n >= 0 => Ok(*n as usize),
            _ => bail!("__array_copy: {what} must be a non-negative int"),
        }
    }
    let src = arr(args.first(), "source")?;
    let si = idx(args.get(1), "sourceIndex")?;
    let dst = arr(args.get(2), "destination")?;
    let di = idx(args.get(3), "destinationIndex")?;
    let n = idx(args.get(4), "length")?;
    objops::array_bulk::copy_range(ctx, &src, si, &dst, di, n).map_err(|e| raise_op(ctx, e))
}

/// `__array_sort_prims(array, count) -> bool` — stable-sort `array[0, count)` natively
/// when every element there is one primitive kind (int-like / double / char / string),
/// in exactly the order the elements' own `CompareTo` gives (see
/// `ArrayObj::sort_prims_prefix`). `false` = declined, array untouched: the caller's
/// script merge sort runs instead. Only reorders references already in the array, so
/// no write barrier is owed (the card belongs to this same array).
pub fn builtin_array_sort_prims(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let n = match args.get(1) {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        _ => bail!("__array_sort_prims: count must be a non-negative int"),
    };
    match args.first() {
        Some(Value::Array(rc)) => Ok(Value::Bool(rc.borrow_mut().sort_prims_prefix(n))),
        Some(Value::Null) => bail!("__array_sort_prims: null array reference"),
        other => bail!("__array_sort_prims: expected an array, got {other:?}"),
    }
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
