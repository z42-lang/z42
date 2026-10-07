/// Array instructions: allocation, element access, length — thin adapters over
/// [`crate::objops::array`] (the single implementation shared with the JIT).
///
/// Each adapter reads registers, calls objops, writes `dst`, and maps an
/// [`OpError`](crate::objops::OpError) to the interp throw channel via [`raise`]:
/// `Ok(Some(exc))` = a catchable exception (`NullReferenceException` /
/// `IndexOutOfRangeException` / `OverflowException` / `OutOfMemoryException`),
/// `Err` = internal VM error.

use crate::metadata::types::{default_value_for, ElemType};
use crate::metadata::{Module, Value};
use crate::objops;
use crate::vm_context::VmContext;
use anyhow::Result;

use super::ops::raise;
use super::Frame;

/// 编译器证明不逃逸、且运行期允许栈分配时，返回取帧号的闭包（帧号惰性分配）。
fn stack_frame<'f>(ctx: &'f VmContext, frame: &'f Frame, stack_alloc: bool) -> Option<impl FnOnce() -> u32 + 'f> {
    (stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled()).then(|| move || frame.frame_id(ctx))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn array_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, size: u32, elem_tag: u8, element_type: ElemType, stack_alloc: bool,
    type_param_kind: u8, type_param_index: i32,
) -> Result<Option<Value>> {
    let len = *frame.get(size)?;
    let prim_zero = generic_prim_zero(frame, type_param_kind, type_param_index);
    let r = objops::array::array_new(
        ctx, &len, elem_tag, element_type, prim_zero, stack_frame(ctx, frame, stack_alloc));
    match r {
        Ok(arr) => { frame.set(dst, arr); Ok(None) }
        Err(e) => raise(ctx, module, e),
    }
}

/// fix-generic-array-value-zero-init (方案 C): `new T[n]` with `T` a generic type
/// parameter — resolve `T` at runtime (method-level → `frame.method_type_args`,
/// class-level → receiver `type_args`, same as `default(T)`); if it is a **primitive
/// value type**, its zero is the per-slot default instead of `Null`.
///
/// Deliberately narrow: struct / reference params keep the erased reference-backed array
/// (generic containers store structs by reference). The class-level resolution is shared
/// with the JIT (`objops::array::class_type_param_zero`); a method-level site never JITs
/// (`unsupported_reason`: the JIT frame has no `method_type_args`).
fn generic_prim_zero(frame: &Frame, kind: u8, index: i32) -> Option<Value> {
    if index < 0 { return None; }
    let idx = index as usize;
    match kind {
        1 => {
            let d = default_value_for(frame.method_type_args.get(idx)?);
            (!matches!(d, Value::Null)).then_some(d)
        }
        2 => objops::array::class_type_param_zero(frame.get(0).ok(), idx),
        _ => None,
    }
}

pub(super) fn array_new_lit(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, elems: &[u32], element_type: ElemType, stack_alloc: bool,
) -> Result<Option<Value>> {
    // Validate every source register up front (the error a bad register raises), so
    // the build below can read them infallibly.
    for r in elems {
        let v = frame.get(*r)?;
        // add-escape-analysis-stack-alloc (diagnostic #2): ArrayNewLit.Elems is an
        // escape sink — a stored element must never be a stack handle.
        debug_assert!(
            !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
            "stack-alloc handle stored into an array literal — escape analysis unsound"
        );
    }
    let vals = elems.iter().map(|r| frame.get(*r).copied().unwrap_or(Value::Null));
    let r = objops::array::array_new_lit(ctx, vals, element_type, stack_frame(ctx, frame, stack_alloc));
    match r {
        Ok(arr) => { frame.set(dst, arr); Ok(None) }
        Err(e) => raise(ctx, module, e),
    }
}

#[inline]
pub(super) fn array_get(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, arr: u32, idx: u32,
) -> Result<Option<Value>> {
    // `Value: Copy` — copy the operands out so the lazy frame-id closure can borrow `frame`.
    let (a, i) = (*frame.get(arr)?, *frame.get(idx)?);
    match objops::array::array_get(ctx, &a, &i, || frame.frame_id(ctx)) {
        Ok(v) => { frame.set(dst, v); Ok(None) }
        Err(e) => raise(ctx, module, e),
    }
}

#[inline]
pub(super) fn array_set(
    ctx: &VmContext, module: &Module, frame: &mut Frame, arr: u32, idx: u32, val: u32,
) -> Result<Option<Value>> {
    let v = frame.get(val)?;
    // add-escape-analysis-stack-alloc (diagnostic #2): ArraySet.val is an escape sink.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into an array element — escape analysis unsound (ArraySet.val)"
    );
    match objops::array::array_set(ctx, frame.get(arr)?, frame.get(idx)?, v) {
        Ok(()) => Ok(None),
        Err(e) => raise(ctx, module, e),
    }
}

pub(super) fn array_len(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, arr: u32,
) -> Result<Option<Value>> {
    match objops::array::array_len(ctx, frame.get(arr)?) {
        Ok(n) => { frame.set(dst, Value::I64(n)); Ok(None) }
        Err(e) => raise(ctx, module, e),
    }
}

#[cfg(test)]
#[path = "exec_array_tests.rs"]
mod exec_array_tests;
