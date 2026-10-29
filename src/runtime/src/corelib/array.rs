// add-array-base-class:
// Std.Array native bindings。`Clone` 浅拷贝;元素是引用类型时共享引用,与 C#
// `System.Array.Clone()` 语义一致。
// add-json-serde: reflective array create/get/set —— serde 反射建/读写
// T[](元素类型只以运行期 Type 已知,无法静态 `new T[n]`)。System.Array parity。
// P1-8 批次 12b:整域迁 builtin ABI v2;实参经 `FromArg` 直读(数组收 `ArrRef` 句柄,
// null/非数组在 ABI 层报 `NullReference`/`InvalidCast`),错误按 C# 口径类型化
// (越界 → `IndexOutOfRangeException`,负长度/负下标 → `ArgumentOutOfRangeException`)。

use crate::corelib::convert::box_prim_to_heap;
use crate::corelib::reflection::read_obj_slot;
use crate::gc::ArrRef;
use crate::metadata::types::{default_value_for, ArrayObj};
use crate::metadata::Value;
use crate::objops;
use crate::vm_context::VmContext;

/// Normalize any element-type spelling (short alias / VM tag / FQ wrapper) to the
/// **short element tag** the array modes key on (`ElemKind::of`). Reference types
/// (`string`, user classes) pass through → a REF / MIXED array.
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
        "string" | "Std.String" => "string",
        "object" | "Std.Object" => "object",
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

#[z42_macros::builtins]
pub mod typed {
    use super::*;
    use crate::corelib::abi::BuiltinError;

    /// `Array.CreateInstance(Type elementType, int length)` — allocate an `elemType[]`
    /// of length `n`, default-initialised. Primitive element types pack (`int` → a PRIM
    /// array of 4 B elements, etc.); reference types get a REF / MIXED array. The array
    /// carries its short element tag so `GetType().GetElementType()` round-trips.
    /// add-json-serde.
    #[builtin(name = "__array_create")]
    pub fn create(ctx: &VmContext, elem_type: &Value, length: i64) -> Result<Value, BuiltinError> {
        let n = usize::try_from(length).map_err(|_| BuiltinError::ArgumentOutOfRange(format!(
            "Array.CreateInstance: length must be non-negative, got {length}")))?;
        let name = type_name_of(elem_type).ok_or_else(|| BuiltinError::Argument(
            "Array.CreateInstance: element type has no name".into()))?;
        let tag = elem_tag(&name);
        let default = default_value_for(tag);
        let heap = ctx.heap();
        let et = crate::metadata::types::ElemType::intern(tag);
        // Same mode choice as `ArrayNew` (REF for a class / interface element type, …).
        Ok(match crate::objops::array::elem_mode(ctx, et, false) {
            crate::objops::array::ElemMode::Struct(layout) => ArrayObj::alloc_struct(heap, et, n, layout),
            crate::objops::array::ElemMode::Kind(kind) => ArrayObj::alloc_filled(heap, et, kind, n, default),
        })
    }

    /// `Array.GetValue(int index)` — read element `i` as an object. Packed integers are
    /// boxed to the matching wrapper (`BoxedStruct`); double / bool / char / reference
    /// elements are already object-representation. add-json-serde.
    #[builtin(name = "__array_get", this = "GetValue")]
    pub fn get(ctx: &VmContext, this: ArrRef, index: i64) -> Result<Value, BuiltinError> {
        let i = index.try_into().map_err(|_| index_oob("__array_get", index, &this))?;
        let tag = {
            let a = this.borrow();
            if i >= a.len() {
                return Err(index_oob("__array_get", index, &this));
            }
            elem_tag(&a.element_type()).to_string()
        };
        // 值 struct 数组的元素装箱成快照（`get_boxed` 没有堆，装不了箱）。
        let raw = objops::array_bulk::elem_get_boxed(ctx, &this, i)?;
        match (int_wrapper_fqn(&tag), &raw) {
            (Some(fqn), Value::I64(n)) => box_prim_to_heap(ctx, fqn, *n).map_err(|e| BuiltinError::Internal(format!(
                "__array_get: boxing element {i} as `{fqn}`: {e}"))),
            _ => Ok(raw),
        }
    }

    /// `Array.SetValue(object value, int index)` — write `value` into element `i`,
    /// unboxing a boxed primitive into the packed slot. Arg order mirrors C# as an
    /// instance method (`this` = array).
    #[builtin(name = "__array_set", this = "SetValue")]
    pub fn set(ctx: &VmContext, this: ArrRef, value: &Value, index: i64) -> Result<(), BuiltinError> {
        let i = index.try_into().map_err(|_| index_oob("__array_set", index, &this))?;
        // Unbox a boxed integer primitive to its raw `I64` so packed backings store it;
        // non-boxed values (F64 / Bool / Char / Str / Object) pass through to `set_boxed`.
        let raw = match value {
            Value::BoxedStruct(s) => match s.borrow().boxed_prim_i64() {
                Some(n) => Value::I64(n),
                None => Value::BoxedStruct(s.clone()),
            },
            other => other.clone(),
        };
        let wrote = {
            let a = this.borrow();
            if i >= a.len() {
                return Err(index_oob("__array_set", index, &this));
            }
            // 值不是元素类型（基元不同种 / 值 struct 数组收到别的东西）→ InvalidCastException（同 C#），
            // 数组不动。
            objops::array_bulk::check_untyped_store(&a, &raw)?;
            // fix-silent-array-elem-zero：走会报错的那版。此前是 `set_boxed`（release 静默存 0）
            // ⇒ `a.SetValue(objNull, 0)` 把 `int[0]` 从 9 变成 0、不抛、报成功。
            // 形参声明是 `Object` ⇒ 没有编译器站点能先转换，值是**用户**给的
            // （与 IR `ArraySet` 那条路不同，那里 debug-only 的原策略依然正确）。
            a.try_set_boxed(i, raw.clone()).map_err(|e| BuiltinError::InvalidCast(format!(
                "Array.SetValue: {e}")))?
        };
        // fix-missing-array-write-barriers : storing a reference into an array is a
        // heap write like any other, and an **old** array receiving a **young** element must mark
        // its card or the next minor will not re-root it. Same rule as `ArraySet`.
        crate::objops::array::barrier_elem(ctx, &this, i, wrote, &raw);
        Ok(())
    }

    /// `Array.CopyRange(source, sourceIndex, destination, destinationIndex, length)` —
    /// bulk element move, mirroring `System.Array.Copy(Array, int, Array, int, int)`.
    /// perf-bulk-array-copy: the script-side `Array.Copy<T>` was a per-element `for`
    /// loop, so every copied element paid an interpreted `ArrayGet` + `ArraySet`. This
    /// is the same "one bulk primitive, the algorithm stays in script" shape as
    /// `__str_to_chars` / `__str_substring`: it copies a range and nothing else.
    ///
    /// The element semantics — every backing incl. value-struct arrays, boxing / unboxing,
    /// type checks (`ArrayTypeMismatchException` / `InvalidCastException`), overlap, write
    /// barrier — live in `objops::array_bulk::copy_range`; this only parses the arguments.
    #[builtin(name = "__array_copy")]
    pub fn copy(
        ctx: &VmContext, src: ArrRef, src_index: i64, dst: ArrRef, dst_index: i64, length: i64,
    ) -> Result<(), BuiltinError> {
        let si = non_negative("__array_copy", "sourceIndex", src_index)?;
        let di = non_negative("__array_copy", "destinationIndex", dst_index)?;
        let n = non_negative("__array_copy", "length", length)?;
        objops::array_bulk::copy_range(ctx, &src, si, &dst, di, n)?;
        Ok(())
    }

    /// `List<T>.Sort()` / `Array.Sort<T>(T[])` 的基元快路径 — stable-sort
    /// `array[0, count)` natively when every element there is one primitive kind
    /// (int-like / double / char / string), in exactly the order the elements' own
    /// `CompareTo` gives (see `ArrayObj::sort_prims_prefix`). `false` = declined, array
    /// untouched: the caller's script merge sort runs instead. A permutation only moves
    /// references the array already holds; the one new reference a store can produce
    /// (a box, when a MIXED cell had fixed another primitive kind) goes to the write
    /// barrier.
    #[builtin(name = "__array_sort_prims")]
    pub fn sort_prims(ctx: &VmContext, array: ArrRef, count: i64) -> Result<bool, BuiltinError> {
        let n = usize::try_from(count).map_err(|_| BuiltinError::ArgumentOutOfRange(format!(
            "__array_sort_prims: count must be non-negative, got {count}")))?;
        let sorted = array.borrow().sort_prims_prefix(n);
        let owner = Value::Array(array);
        for b in sorted.iter().flatten() {
            ctx.heap().write_barrier_array_elem(&owner, 0, b);
        }
        Ok(sorted.is_some())
    }

    /// `Array.Clone()` — shallow copy of the receiver array. Reference-type elements
    /// are shared (the new array's slots reference the same heap objects). Empty arrays
    /// return another empty array (not the same reference).
    #[builtin(name = "__array_clone", this = "Clone")]
    pub fn clone(ctx: &VmContext, this: ArrRef) -> Value {
        // `deep_copy` allocates a fresh array block with the same bytes (reference
        // elements stay shared: a copied reference word names the same referent).
        this.borrow().deep_copy(ctx.heap())
    }

    fn non_negative(who: &str, what: &str, v: i64) -> Result<usize, BuiltinError> {
        usize::try_from(v).map_err(|_| BuiltinError::ArgumentOutOfRange(format!(
            "{who}: {what} must be non-negative, got {v}")))
    }

    fn index_oob(who: &str, index: i64, arr: &ArrRef) -> BuiltinError {
        BuiltinError::IndexOutOfRange(format!(
            "{who}: index {index} out of bounds (len {})", arr.borrow().len()))
    }
}

#[cfg(test)]
#[path = "array_tests.rs"]
mod array_tests;
