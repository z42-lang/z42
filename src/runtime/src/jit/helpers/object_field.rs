#![allow(dangerous_implicit_autorefs)]
//! JIT field-access helpers — thin adapters over [`crate::objops::field`]:
//! register read/write plus mapping `OpError` to the pending-exception channel
//! ([`super::raise`]). The semantics (FieldIC, stack objects, `Length` pseudo-fields,
//! boxed structs, write barrier, null → `NullReferenceException`) live in objops,
//! shared with the interpreter.
//!
//! They are the slow path of the native field IC (`jit::translate::field_ic`): a site
//! whose inline guard misses — null or non-object receiver, a type or cell kind entry 0
//! does not cache yet — calls them, and they install the site's IC.

use crate::metadata::resolver::FieldIC;

use super::super::frame::JitFrame;
use crate::vm_context::VmContext;
use super::{raise, vm_ctx_ref};

#[inline(always)]
unsafe fn ic_ref<'a>(ic_ptr: *const FieldIC) -> Option<&'a FieldIC> {
    ic_ptr.as_ref()
}

#[cfg(test)]
thread_local! {
    /// Test probe: calls of the two helpers on this thread — which accesses left the native path.
    pub(crate) static HELPER_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[inline(always)]
fn note_helper_call() {
    #[cfg(test)]
    HELPER_CALLS.with(|c| c.set(c.get() + 1));
}

/// `FieldGet` helper. `ic_ptr` = the site's `FieldIC` (stable pointer baked at codegen; may be null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_get(
    frame: *mut JitFrame, ctx: *const VmContext,
    dst: u32, obj: u32,
    field_name_ptr: *const u8, field_name_len: usize,
    ic_ptr: *const FieldIC,
) -> u8 {
    note_helper_call();
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
    frame: *mut JitFrame, ctx: *const VmContext,
    obj: u32,
    field_name_ptr: *const u8, field_name_len: usize, val: u32,
    ic_ptr: *const FieldIC,
) -> u8 {
    note_helper_call();
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
