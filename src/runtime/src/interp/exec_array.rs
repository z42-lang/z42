/// Array instructions: allocation, element access, length — thin adapters over
/// [`crate::objops::array`] (the single implementation shared with the JIT).
///
/// Each adapter reads registers, calls objops, writes `dst`, and maps an
/// [`OpError`](crate::objops::OpError) to the interp throw channel via [`raise`]:
/// `Ok(Some(exc))` = a catchable exception (`NullReferenceException` /
/// `IndexOutOfRangeException` / `OverflowException` / `OutOfMemoryException`),
/// `Err` = internal VM error.

use crate::metadata::types::{default_value_for, ElemKind, ElemType};
use crate::vm::runtime::Module;
use crate::metadata::Value;
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
#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn array_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, size: u32, elem_tag: u8, element_type: ElemType, stack_alloc: bool,
    type_param_kind: u8, type_param_index: i32,
) -> Result<Option<Value>> {
    let len = *frame.get(size)?;
    let tp_prim = generic_prim_elem(frame, type_param_kind, type_param_index);
    let bound = generic_bound_elem(frame, type_param_kind, type_param_index);
    let r = objops::array::array_new(
        ctx, &len, elem_tag, element_type, type_param_kind != 0, tp_prim, bound, stack_frame(ctx, frame, stack_alloc));
    match r {
        Ok(arr) => { frame.set(dst, arr); Ok(None) }
        Err(e) => raise(ctx, module, e),
    }
}

/// `new T[n]`: the argument `T` is bound to here (same carriers as [`generic_prim_elem`]) — the
/// element type the array reports.
fn generic_bound_elem(frame: &Frame, kind: u8, index: i32) -> Option<ElemType> {
    if index < 0 { return None; }
    let idx = index as usize;
    match kind {
        1 => objops::array::type_param_bound_elem(frame.method_type_args.get(idx).map(String::as_str)),
        2 => objops::array::class_type_param_bound(frame.get(0).ok(), idx),
        _ => None,
    }
}

/// `new T[n]` with `T` a generic type parameter — resolve `T` at runtime (method-level →
/// `frame.method_type_args`, class-level → receiver `type_args`, same as `default(T)`); if it is a
/// **primitive value type**, M13 packs the array at its width: `(ElemKind, zero)` per slot.
///
/// Reference / struct params and unresolvable sites keep the erased cell-backed array (generic
/// containers store structs by reference; a reference `T` is `Mixed` by [`ElemKind::of_type_param`]).
/// The class-level resolution is shared with the JIT (`objops::array::class_type_param_elem`); a
/// method-level site never JITs (`unsupported_reason`: the JIT frame has no `method_type_args`).
/// Method-level names are not interned — the match runs per allocation; the sites are generic
/// static methods (`Array.Sort<T>`'s scratch), low-rate by construction.
fn generic_prim_elem(frame: &Frame, kind: u8, index: i32) -> Option<objops::array::TypeParamElem> {
    if index < 0 { return None; }
    let idx = index as usize;
    match kind {
        1 => {
            let name = frame.method_type_args.get(idx)?;
            let k = ElemKind::of_type_param(name);
            if !k.is_prim() { return None; }
            let zero = default_value_for(name);
            (!matches!(zero, Value::Null)).then_some(objops::array::TypeParamElem { kind: k, zero })
        }
        2 => objops::array::class_type_param_elem(frame.get(0).ok(), idx),
        _ => None,
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
