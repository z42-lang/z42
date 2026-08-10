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

pub(super) fn array_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, size: u32, elem_tag: u8, element_type: &str, stack_alloc: bool,
) -> Result<Option<Value>> {
    let n = to_usize(frame.get(size)?, "ArrayNew size")?;
    let default = default_value_for_tag(elem_tag);
    // add-escape-analysis-stack-alloc: non-escaping array → frame arena (no GC).
    if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let arr = crate::metadata::types::ArrayObj::typed(element_type, vec![default; n]);
        let idx = ctx.stack_arena.lock().alloc_arr(frame.frame_id, arr);
        frame.set(dst, Value::StackArray { idx, frame_id: frame.frame_id });
        return Ok(None);
    }
    // add-reflection-array-element-type: carry the element type for non-erased
    // `arr.GetType().GetElementType()`.
    let arr = ctx.heap().alloc_array_typed(element_type, vec![default; n]);
    if matches!(arr, Value::Null) {
        ctx.heap().set_strict_oom(false);
        let exc = crate::exception::make_stdlib_exception(
            ctx, module, "Std.OutOfMemoryException",
            format!("cannot allocate array[{n}]: heap limit exceeded"),
        ).unwrap_or(Value::Null);
        ctx.heap().set_strict_oom(true);
        return Ok(Some(exc));
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
    if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let arr = crate::metadata::types::ArrayObj::typed(element_type, vals);
        let idx = ctx.stack_arena.lock().alloc_arr(frame.frame_id, arr);
        frame.set(dst, Value::StackArray { idx, frame_id: frame.frame_id });
        return Ok(None);
    }
    let arr = ctx.heap().alloc_array_typed(element_type, vals);
    if matches!(arr, Value::Null) {
        ctx.heap().set_strict_oom(false);
        let exc = crate::exception::make_stdlib_exception(
            ctx, module, "Std.OutOfMemoryException",
            format!("cannot allocate array literal[{n}]: heap limit exceeded"),
        ).unwrap_or(Value::Null);
        ctx.heap().set_strict_oom(true);
        return Ok(Some(exc));
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
            borrowed.get_boxed(i)   // typed accessor: boxes packed primitives
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
