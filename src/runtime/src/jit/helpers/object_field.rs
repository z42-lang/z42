#![allow(dangerous_implicit_autorefs)]
//! JIT field-access helpers — thin adapters over [`crate::objops::field`]:
//! register read/write plus mapping `OpError` to the pending-exception channel
//! ([`super::raise`]). The semantics (FieldIC, stack objects, `Length` pseudo-fields,
//! boxed structs, write barrier, null → `NullReferenceException`) live in objops,
//! shared with the interpreter.
//!
//! The two `*_slot` resolvers back the loop-invariant hoist: emitted once in the
//! entry block for a never-reassigned receiver. They never throw — `off < 0` makes
//! the inline code fall back to `jit_field_get` / `jit_field_set`, which raise the
//! real exception at the real access.

use crate::metadata::resolver::FieldIC;

use super::super::frame::{JitFrame, JitModuleCtx};
use super::{raise, vm_ctx_ref};

#[inline(always)]
unsafe fn ic_ref<'a>(ic_ptr: *const FieldIC) -> Option<&'a FieldIC> {
    ic_ptr.as_ref()
}

/// Hoisted **inline primitive** field resolver (P5-B). On success writes the object's
/// `bytes` base and the field's byte offset; the per-access inline does a native
/// width-aware load/store. The runtime `(width, tag)` must match the JIT's
/// compile-time expectation, else no fast path. GC-safe: non-moving collector, fixed
/// `bytes` allocation, receiver held live by the frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_field_slot(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    obj: u32, field_name_ptr: *const u8, field_name_len: usize,
    expected_width: u32, expected_tag: u32,
    out_bytes_ptr: *mut *const u8, out_off: *mut i64,
) {
    *out_bytes_ptr = std::ptr::null();
    *out_off = -1;
    let field_name = super::baked_str(field_name_ptr, field_name_len);
    let recv = &(*frame).regs[obj as usize];
    if let Some((ptr, off, width, tag)) = crate::objops::field::inline_prim_slot(recv, field_name) {
        if width == expected_width && tag as u32 == expected_tag {
            *out_bytes_ptr = ptr;
            *out_off = off as i64;
        }
    }
}

/// Hoisted **byte-inlined reference** field resolver (T1-B): the reference twin of
/// [`jit_obj_field_slot`]. `out_tag` = the `Value` discriminant to stamp on a non-null
/// load (`7` = `Value::Object`, `6` = `Value::Array`; pinned by
/// `value_discriminants_pinned` in `metadata/types_tests.rs`). Read-only (no barrier).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_ref_field_slot(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    obj: u32, field_name_ptr: *const u8, field_name_len: usize,
    out_bytes_ptr: *mut *const u8, out_off: *mut i64, out_tag: *mut i32,
) {
    *out_bytes_ptr = std::ptr::null();
    *out_off = -1;
    *out_tag = 0;
    let field_name = super::baked_str(field_name_ptr, field_name_len);
    let recv = &(*frame).regs[obj as usize];
    if let Some((ptr, off, is_array)) = crate::objops::field::inline_ref_slot(recv, field_name) {
        *out_bytes_ptr = ptr;
        *out_off = off as i64;
        *out_tag = if is_array { 6 } else { 7 };
    }
}

/// `FieldGet` helper. `ic_ptr` = the site's `FieldIC` (stable pointer baked at codegen; may be null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_get(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32,
    field_name_ptr: *const u8, field_name_len: usize,
    ic_ptr: *const FieldIC,
) -> u8 {
    let field_name = super::baked_str(field_name_ptr, field_name_len);
    let recv = &(*frame).regs[obj as usize];
    match crate::objops::field::field_get(vm_ctx_ref(ctx), recv, field_name, ic_ref(ic_ptr)) {
        Ok(v) => { (*frame).regs[dst as usize] = v; 0 }
        Err(e) => raise(ctx, e),
    }
}

/// `FieldSet` helper (write barrier + primitive-slot type check inside objops).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    obj: u32,
    field_name_ptr: *const u8, field_name_len: usize, val: u32,
    ic_ptr: *const FieldIC,
) -> u8 {
    let field_name = super::baked_str(field_name_ptr, field_name_len);
    let regs = &(*frame).regs;
    match crate::objops::field::field_set(
        vm_ctx_ref(ctx), &regs[obj as usize], field_name, &regs[val as usize], ic_ref(ic_ptr),
    ) {
        Ok(()) => 0,
        Err(e) => raise(ctx, e),
    }
}

#[cfg(test)]
#[path = "object_field_tests.rs"]
mod object_field_tests;
