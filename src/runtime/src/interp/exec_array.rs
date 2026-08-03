/// Array instructions: allocation, element access, length.
/// add-gc-oom-exception: array_new / array_new_lit return Option<Value>
/// to propagate Std.OutOfMemoryException when alloc returns Null under
/// strict OOM mode. Other helpers remain Result<()>.

use crate::metadata::types::default_value_for_tag;
use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::ops::to_usize;
use super::Frame;

pub(super) fn array_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, size: u32, elem_tag: u8, element_type: &str,
) -> Result<Option<Value>> {
    let n = to_usize(frame.get(size)?, "ArrayNew size")?;
    let default = default_value_for_tag(elem_tag);
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
    dst: u32, elems: &[u32], element_type: &str,
) -> Result<Option<Value>> {
    let vals: Vec<Value> = elems.iter()
        .map(|r| frame.get(*r).map(|v| v.clone()))
        .collect::<Result<_>>()?;
    let n = vals.len();
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

pub(super) fn array_get(frame: &mut Frame, dst: u32, arr: u32, idx: u32) -> Result<()> {
    let result = match frame.get(arr)? {
        Value::Array(rc) => {
            let rc = rc.clone();
            let i = to_usize(frame.get(idx)?, "ArrayGet index")?;
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
    let arr_value = frame.get(arr)?.clone();
    match &arr_value {
        Value::Array(rc) => {
            let i = to_usize(frame.get(idx)?, "ArraySet index")?;
            let mut borrowed = rc.borrow_mut();
            if i >= borrowed.len() {
                bail!("array index {} out of bounds (len={})", i, borrowed.len());
            }
            borrowed.set_boxed(i, v.clone());   // typed accessor: unboxes into packed
            drop(borrowed);
            if v.is_heap_ref() {
                ctx.heap().write_barrier_array_elem(&arr_value, i, &v);
            }
            Ok(())
        }
        other => bail!("ArraySet: expected array, got {:?}", other),
    }
}

pub(super) fn array_len(frame: &mut Frame, dst: u32, arr: u32) -> Result<()> {
    let len = match frame.get(arr)? {
        Value::Array(rc) => rc.borrow().len() as i32,
        other => bail!("ArrayLen: expected array, got {:?}", other),
    };
    frame.set(dst, Value::I64(len as i64));
    Ok(())
}
