/// Array instructions: allocation, element access, length.
/// add-gc-oom-exception: array_new / array_new_lit return Option<Value>
/// to propagate Std.OutOfMemoryException when alloc returns Null under
/// strict OOM mode. Other helpers remain Result<()>.
///
/// add-escape-analysis-stack-alloc: when the compiler proves an array does not
/// escape its frame, `stack_alloc` routes allocation to the per-context stack
/// arena (no GC). `Value::StackArray { idx, frame_id }` handles are resolved by
/// array_get / array_set / array_len through `ctx.stack_arena` (validated: idx in
/// range + frame_id matches, else a clear stale-handle diagnostic).

use crate::metadata::types::default_value_for_tag;
use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::ops::to_usize;
use super::Frame;

/// add-struct-array-codegen (P3b follow-up): if `element_type` is a **blob value struct**
/// (a value type with ≥2 fields + a delivered byte layout — the same `IsBlobStruct`
/// criterion the compiler uses to emit inline-struct access), build a `StructBytes`-backed
/// array of `len` zero-initialized elements (C# inline `struct[]`). `None` for primitives /
/// reference types / single-field structs (they keep `Boxed` / packed backings).
pub(crate) fn try_struct_backed(ctx: &VmContext, element_type: &str, len: usize) -> Option<crate::metadata::types::ArrayObj> {
    let td = ctx.try_lookup_type(element_type)?;
    if td.fields.len() < 2 { return None; }   // FieldCount >= 2 (matches IsBlobStruct)
    let layout = td.struct_layout()?;         // value struct with a delivered byte layout
    if layout.size == 0 { return None; }      // self-referential / empty guard
    // unify-gc-heap PR-3: the constructor allocates the struct[] byte + ref blocks in the GC heap.
    Some(crate::metadata::types::ArrayObj::struct_backed(ctx.heap(), element_type, len, layout))
}

/// add-struct-array-codegen (P3b follow-up): pack one struct value `v` into element `i` of
/// a `StructBytes`-backed array (for `new Point[]{ p1, p2 }`). `v` is a `StructRef` (arena
/// blob — resolved via `ctx.struct_arena`) or a `BoxedStruct` (owned snapshot); its bytes +
/// reference leaves are copied into the element's byte window + ref side-table slots.
/// `Null` = default (leave the zero-initialized element untouched).
pub(crate) fn pack_struct_elem(ctx: &VmContext, arr: &mut crate::metadata::types::ArrayObj, i: usize, v: &Value) -> Result<()> {
    let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = match v {
        // add-boxed-struct-identity (P4b): read the source box's blob out of its shared object.
        Value::BoxedStruct(b) => { let o = b.borrow(); (o.bytes.to_vec(), o.refs.to_vec()) }
        Value::StructRef { idx, frame_id } =>
            ctx.struct_arena.lock().with(*idx, *frame_id, |s| (s.bytes.to_vec(), s.refs.to_vec()))?,
        Value::Null => return Ok(()),
        other => bail!("struct array literal element must be a struct value, got {other:?}"),
    };
    // unify-gc-heap PR-3: struct[] element bytes + ref side-table live in GC blocks now;
    // write through the heap-aware accessor (block payloads are private to ArrayObj).
    arr.write_struct_elem(i, &src_bytes, &src_refs);
    Ok(())
}

pub(super) fn array_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, size: u32, elem_tag: u8, element_type: &str, stack_alloc: bool,
) -> Result<Option<Value>> {
    let n = to_usize(frame.get(size)?, "ArrayNew size")?;
    // add-struct-array-codegen: blob value-struct element → StructBytes heap backing
    // (skips stack-alloc + packed paths; element access via StructRefHeap handle).
    if let Some(sb) = try_struct_backed(ctx, element_type, n) {
        let arr = ctx.heap().alloc_array_obj(sb);
        if matches!(arr, Value::Null) {
            return Ok(Some(crate::exception::make_oom_exception(
                ctx, module,
                format!("cannot allocate struct array[{n}]: heap limit exceeded"),
            )));
        }
        frame.set(dst, arr);
        return Ok(None);
    }
    let default = default_value_for_tag(elem_tag);
    // add-escape-analysis-stack-alloc: non-escaping array → frame arena (no GC).
    if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let arr = crate::metadata::types::ArrayObj::stack_typed(element_type, vec![default; n]);
        let idx = ctx.stack_alloc_arr(frame.frame_id, arr);
        frame.set(dst, Value::StackArray { idx, frame_id: frame.frame_id });
        return Ok(None);
    }
    // add-reflection-array-element-type: carry the element type for non-erased
    // `arr.GetType().GetElementType()`.
    let arr = ctx.heap().alloc_array_typed(element_type, vec![default; n]);
    if matches!(arr, Value::Null) {
        return Ok(Some(crate::exception::make_oom_exception(
            ctx, module,
            format!("cannot allocate array[{n}]: heap limit exceeded"),
        )));
    }
    frame.set(dst, arr);
    Ok(None)
}

pub(super) fn array_new_lit(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, elems: &[u32], element_type: &str, stack_alloc: bool,
) -> Result<Option<Value>> {
    let vals: Vec<Value> = elems.iter()
        .map(|r| frame.get(*r).map(|v| v.clone()))
        .collect::<Result<_>>()?;
    let n = vals.len();
    // add-escape-analysis-stack-alloc (diagnostic #2): ArrayNewLit.Elems is an
    // escape sink — a stored element must never be a stack handle (would outlive
    // its frame). Assert the analysis kept that invariant.
    debug_assert!(
        !vals.iter().any(|v| matches!(v, Value::StackObject { .. } | Value::StackArray { .. })),
        "stack-alloc handle stored into an array literal — escape analysis unsound"
    );
    // add-struct-array-codegen: blob value-struct literal → StructBytes backing, packing
    // each element's bytes + reference leaves (skips stack-alloc; heap-only for v1).
    if let Some(mut sb) = try_struct_backed(ctx, element_type, n) {
        for (i, v) in vals.iter().enumerate() { pack_struct_elem(ctx, &mut sb, i, v)?; }
        let arr = ctx.heap().alloc_array_obj(sb);
        if matches!(arr, Value::Null) {
            return Ok(Some(crate::exception::make_oom_exception(
                ctx, module,
                format!("cannot allocate struct array literal[{n}]: heap limit exceeded"),
            )));
        }
        frame.set(dst, arr);
        return Ok(None);
    }
    if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let arr = crate::metadata::types::ArrayObj::stack_typed(element_type, vals);
        let idx = ctx.stack_alloc_arr(frame.frame_id, arr);
        frame.set(dst, Value::StackArray { idx, frame_id: frame.frame_id });
        return Ok(None);
    }
    let arr = ctx.heap().alloc_array_typed(element_type, vals);
    if matches!(arr, Value::Null) {
        return Ok(Some(crate::exception::make_oom_exception(
            ctx, module,
            format!("cannot allocate array literal[{n}]: heap limit exceeded"),
        )));
    }
    frame.set(dst, arr);
    Ok(None)
}

pub(super) fn array_get(ctx: &VmContext, frame: &mut Frame, dst: u32, arr: u32, idx: u32) -> Result<()> {
    // Read the index first so its `frame.get` borrow ends before we borrow the
    // array — this lets us borrow the `GcRef` through `frame.get(arr)` directly
    // instead of `rc.clone()`-ing it (saves one Arc refcount atomic per access,
    // the hot path of interp array-scan loops).
    let i = to_usize(frame.get(idx)?, "ArrayGet index")?;
    let result = match frame.get(arr)? {
        // add-escape-analysis-stack-alloc: stack array — resolve via arena.
        Value::StackArray { idx: aidx, frame_id } => {
            let (aidx, frame_id) = (*aidx, *frame_id);
            ctx.stack_arena.lock().with_arr(aidx, frame_id, |a| {
                if i >= a.len() {
                    bail!("array index {} out of bounds (len={})", i, a.len());
                }
                Ok(a.get_boxed(i))
            })??
        }
        Value::Array(rc) => {
            let borrowed = rc.borrow();
            if i >= borrowed.len() {
                bail!("array index {} out of bounds (len={})", i, borrowed.len());
            }
            // add-struct-array-codegen (P3b follow-up): a value-struct array element is
            // returned as a `StructRefHeap` handle into the array's byte backing (route
            // α — in-place `arr[i].x` / value-copy at consumers), not a boxed snapshot.
            // The array `GcRef` is only reachable here (not in `get_boxed`), so the
            // handle must be built at the exec layer.
            if matches!(&borrowed.backing, crate::metadata::types::ArrayBacking::StructBytes { .. }) {
                let arr_gc = *rc;
                drop(borrowed);
                // make-value-copy: StructRefHeap payload → transient arena; Value holds an 8B handle.
                let fid = frame.frame_id;
                let hidx = ctx.transient_alloc(
                    fid,
                    crate::interp::transient_arena::TransientPayload::StructElem(
                        crate::metadata::types::StructArrayElem { arr: arr_gc, index: i as u32 },
                    ),
                );
                Value::StructRefHeap { idx: hidx, frame_id: fid }
            } else {
                borrowed.get_boxed(i)   // typed accessor: boxes packed primitives
            }
        }
        other => bail!("ArrayGet: expected array, got {:?}", other),
    };
    frame.set(dst, result);
    Ok(())
}

/// `ArraySet` dispatch.
///
/// **add-write-barriers (2026-05-21)**: dispatches `write_barrier_array_elem`
/// after a successful element write *iff* the new value is a heap
/// reference (`v.is_heap_ref()`). Primitive writes skip the dispatch
/// per Decision 1.
pub(super) fn array_set(ctx: &VmContext, frame: &mut Frame, arr: u32, idx: u32, val: u32) -> Result<()> {
    let v = frame.get(val)?.clone();
    // add-escape-analysis-stack-alloc (diagnostic #2): ArraySet.val is an escape
    // sink — a stored element must never be a stack handle.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into an array element — escape analysis unsound (ArraySet.val)"
    );
    // Read the index first so its borrow ends before we borrow the array; then
    // borrow the array `Value` through `frame.get(arr)` directly — no
    // `arr_value.clone()` (Arc atomic) on the hot path. The write barrier (rare,
    // heap-ref values only) uses the match-bound `&Value` reference in place.
    let i = to_usize(frame.get(idx)?, "ArraySet index")?;
    match frame.get(arr)? {
        // add-escape-analysis-stack-alloc: stack array — write via arena. No GC
        // write barrier: not a heap slot (heap-ref elems kept live by root scan).
        Value::StackArray { idx: aidx, frame_id } => {
            let (aidx, frame_id) = (*aidx, *frame_id);
            ctx.stack_arena.lock().with_arr_mut(aidx, frame_id, |a| {
                if i >= a.len() {
                    bail!("array index {} out of bounds (len={})", i, a.len());
                }
                a.set_boxed(i, v.clone());
                Ok(())
            })??;
            Ok(())
        }
        arr_val @ Value::Array(rc) => {
            let mut borrowed = rc.borrow_mut();
            if i >= borrowed.len() {
                bail!("array index {} out of bounds (len={})", i, borrowed.len());
            }
            borrowed.set_boxed(i, v.clone());   // typed accessor: unboxes into packed
            drop(borrowed);
            if v.is_heap_ref() {
                ctx.heap().write_barrier_array_elem(arr_val, i, &v);
            }
            Ok(())
        }
        other => bail!("ArraySet: expected array, got {:?}", other),
    }
}

pub(super) fn array_len(ctx: &VmContext, frame: &mut Frame, dst: u32, arr: u32) -> Result<()> {
    let len = match frame.get(arr)? {
        Value::StackArray { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_arr(idx, frame_id, |a| a.len() as i32)?
        }
        Value::Array(rc) => rc.borrow().len() as i32,
        other => bail!("ArrayLen: expected array, got {:?}", other),
    };
    frame.set(dst, Value::I64(len as i64));
    Ok(())
}
