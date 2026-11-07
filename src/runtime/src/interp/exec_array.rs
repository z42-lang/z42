/// Array instructions: allocation, element access, length — thin adapters over
/// [`crate::objops::array`] (the single implementation shared with the JIT).
///
/// Each adapter reads registers, calls objops, writes `dst`, and maps an
/// [`OpError`](crate::objops::OpError) to the interp throw channel via [`raise`]:
/// `Ok(Some(exc))` = a catchable exception (`NullReferenceException` /
/// `IndexOutOfRangeException` / `OverflowException` / `OutOfMemoryException`),
/// `Err` = internal VM error.

use crate::object::ElemType;
use crate::vm::runtime::Module;
use crate::object::Value;
use crate::objops;
use crate::vm_context::VmContext;
use anyhow::Result;

use super::ops::raise;
use super::Frame;

/// 编译器证明不逃逸、且运行期允许栈分配时，返回取帧号的闭包（帧号惰性分配）。
fn stack_frame<'f>(ctx: &'f VmContext, frame: &'f Frame, stack_alloc: bool) -> Option<impl FnOnce() -> u32 + 'f> {
    (stack_alloc && crate::vm::exec::call::stack_alloc::stack_alloc_enabled()).then(|| move || frame.frame_id(ctx))
}

#[allow(clippy::too_many_arguments)]
#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn array_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, size: u32, elem_tag: u8, element_type: ElemType, stack_alloc: bool,
    type_param_kind: u8, type_param_index: i32, owner: Option<&str>,
) -> Result<Option<Value>> {
    let len = *frame.get(size)?;
    let (tp_prim, bound) = generic_array_args(ctx, frame, type_param_kind, type_param_index, owner);
    let r = objops::array::array_new(
        ctx, &len, elem_tag, element_type, type_param_kind != 0, tp_prim, bound, stack_frame(ctx, frame, stack_alloc));
    match r {
        Ok(arr) => { frame.set(dst, arr); Ok(None) }
        Err(e) => raise(ctx, module, e),
    }
}

fn generic_array_args(
    ctx: &VmContext, frame: &Frame, kind: u8, index: i32, owner: Option<&str>,
) -> (Option<objops::array::TypeParamElem>, Option<ElemType>) {
    if index < 0 { return (None, None); }
    let index = index as usize;
    match kind {
        1 => objops::array::method_type_param_array_args(&frame.method_type_args, index),
        2 => objops::array::class_type_param_array_args(ctx, frame.get(0).ok(), index, owner),
        _ => (None, None),
    }
}

#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
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
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, arr: u32, idx: u32, erased: bool,
) -> Result<Option<Value>> {
    // `Value: Copy` — copy the operands out so the lazy frame-id closure can borrow `frame`.
    let (a, i) = (*frame.get(arr)?, *frame.get(idx)?);
    match objops::array::array_get(ctx, &a, &i, erased, || frame.frame_id(ctx)) {
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
