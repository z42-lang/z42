#![allow(dangerous_implicit_autorefs)]
//! Array allocation, element access, length — thin adapters over
//! [`crate::objops::array`] (shared with the interpreter): register read/write plus
//! mapping `OpError` to the pending-exception channel ([`super::raise`]).

use crate::metadata::types::{ElemType, ElemTypeInfo};
use crate::metadata::Value;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::{raise, vm_ctx_ref};

/// `class_tp`: for `new T[n]` on a **class-level** type parameter, its index (else -1).
/// The receiver (reg 0) carries the concrete type args, so a primitive `T` gets its zero
/// per slot — resolved by the same `objops::array::class_type_param_zero` interp uses.
/// (A method-level `T` has no JIT carrier → `unsupported_reason`.) The JIT never
/// stack-allocates, so no frame id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_new(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, size: u32, elem_tag: u8,
    // the instruction's interned element-type handle (`ElemType::as_raw`) —
    // non-erased array reflection, no name allocation.
    et: *const ElemTypeInfo,
    class_tp: i32,
) -> u8 {
    let element_type = ElemType::from_raw(et);
    let len = (*frame).regs[size as usize];
    let prim_zero = if class_tp >= 0 {
        crate::objops::array::class_type_param_zero((*frame).regs.first(), class_tp as usize)
    } else { None };
    let r = crate::objops::array::array_new(
        vm_ctx_ref(ctx), &len, elem_tag, element_type, prim_zero, None::<fn() -> u32>);
    match r {
        Ok(arr) => { (*frame).regs[dst as usize] = arr; 0 }
        Err(e) => raise(ctx, e),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_new_lit(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, elems_ptr: *const u32, elem_cnt: usize,
    et: *const ElemTypeInfo,
) -> u8 {
    let elems = std::slice::from_raw_parts(elems_ptr, elem_cnt);
    let regs = &(*frame).regs;
    let vals = elems.iter().map(|&r| regs[r as usize]);
    let element_type = ElemType::from_raw(et);
    let r = crate::objops::array::array_new_lit(vm_ctx_ref(ctx), vals, element_type, None::<fn() -> u32>);
    match r {
        Ok(arr) => { (*frame).regs[dst as usize] = arr; 0 }
        Err(e) => raise(ctx, e),
    }
}

/// Packed-array fast path data (`int[]` / `long[]` / `double[]`): writes the element
/// buffer base, length and slot width (4/8). Anything else — non-packed backing,
/// stack array, non-array, null — writes width 0 = "no fast path", and the inline
/// code falls back to `jit_array_get` / `jit_array_set`, which raise the same
/// exception the interpreter does. **Never throws**; used both by the loop-invariant
/// hoist (once in the entry block) and per access. GC-safe: arrays are fixed-length
/// and the collector is non-moving, so the buffer stays put for the frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_data_opt(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    arr: u32, out_ptr: *mut *const Value, out_len: *mut i64, out_width: *mut i64,
) {
    let (ptr, len, width) = crate::objops::array::packed_data(&(*frame).regs[arr as usize]);
    *out_ptr = ptr as *const Value;
    *out_len = len;
    *out_width = width;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_get(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, arr: u32, idx: u32,
) -> u8 {
    let (a, i) = ((*frame).regs[arr as usize], (*frame).regs[idx as usize]);
    // A value-struct element becomes a `StructRefHeap` handle in the transient arena,
    // stamped with this frame's (lazily assigned) id.
    match crate::objops::array::array_get(vm_ctx_ref(ctx), &a, &i, || super::struct_ops::frame_id_of(frame, ctx)) {
        Ok(v) => { (*frame).regs[dst as usize] = v; 0 }
        Err(e) => raise(ctx, e),
    }
}

/// `ArraySet` helper (write barrier inside objops).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    arr: u32, idx: u32, val: u32,
) -> u8 {
    let regs = &(*frame).regs;
    match crate::objops::array::array_set(
        vm_ctx_ref(ctx), &regs[arr as usize], &regs[idx as usize], &regs[val as usize],
    ) {
        Ok(()) => 0,
        Err(e) => raise(ctx, e),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_len(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, arr: u32,
) -> u8 {
    let a = (*frame).regs[arr as usize];
    match crate::objops::array::array_len(vm_ctx_ref(ctx), &a) {
        Ok(n) => { (*frame).regs[dst as usize] = Value::I64(n); 0 }
        Err(e) => raise(ctx, e),
    }
}

#[cfg(test)]
#[path = "array_tests.rs"]
mod array_tests;
