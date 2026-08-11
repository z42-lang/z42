#![allow(dangerous_implicit_autorefs)]
//! add-struct-jit-value-path (P5): JIT helpers for the blob value-type
//! instructions (`StructAlloc` / `StructCopy` / `StructFieldGetPrim` /
//! `StructFieldSetPrim`).
//!
//! # Model
//! These mirror the interpreter's [`crate::interp::exec_struct`] execution but read
//! and write the `JitFrame` register file instead of an interp `Frame`. All the
//! real work — arena allocation, the byte<->`Value` codec, and the base-polymorphic
//! dispatch (arena `StructRef` / heap `Object` inline field / `StructRefHeap` array
//! element) — is the *same* code: each helper calls the frame-agnostic `*_val` core
//! in `exec_struct`, so interp and JIT stay byte-for-byte identical in semantics.
//!
//! This is the **helper-bridge** design (P5-A): the struct op itself runs at
//! interpreter speed inside the helper, but the surrounding arithmetic / control
//! flow / calls are native — so a function that *touches* a struct is no longer
//! forced back to the interpreter wholesale. Emitting the leaf byte load/store as
//! native cranelift code (skipping the helper call) is Deferred (P5-B) — see
//! `docs/spec/.../add-struct-jit-value-path/design.md` Decision D1.
//!
//! # frame_id
//! A `Value::StructRef` carries the id of the frame that allocated it, used by the
//! shared per-context arena's staleness guard. Deref of an existing handle reads
//! the id embedded in the handle (not the current frame), so only the *allocation*
//! sites ([`frame_id_of`]) need the current frame's id — assigned lazily from
//! `VmContext::next_frame_id()` the first time this frame allocates a struct. `0`
//! means "not yet allocated"; `next_frame_id()` never returns `0`.

use crate::interp::exec_struct;
use crate::metadata::Value;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref};

/// Lazily assign (and return) this JIT frame's monotonic id, so a `StructRef` it
/// allocates participates in the arena staleness guard exactly like an interp
/// frame. Called only from allocation sites (StructAlloc / unbox / array copy-out).
#[inline]
pub(super) unsafe fn frame_id_of(frame: *mut JitFrame, ctx: *const JitModuleCtx) -> u32 {
    if (*frame).frame_id == 0 {
        (*frame).frame_id = vm_ctx_ref(ctx).next_frame_id();
    }
    (*frame).frame_id
}

/// `StructAlloc dst, type_name, size` — allocate a zero-initialized blob in the
/// per-context struct arena; `regs[dst]` = `StructRef` handle. Infallible
/// (arena allocation cannot fail), so no u8 return / exception path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_struct_alloc(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, type_ptr: *const u8, type_len: usize, size: u32,
) {
    let type_name = std::str::from_utf8(std::slice::from_raw_parts(type_ptr, type_len))
        .unwrap_or("<invalid>");
    let fid = frame_id_of(frame, ctx);
    let v = exec_struct::struct_alloc_val(vm_ctx_ref(ctx), fid, type_name, size);
    (*frame).regs[dst as usize] = v;
}

/// `StructCopy dst, src, size` — value-semantics blob copy (assign/param/return).
/// Returns 0 on success, 1 (+ pending exception) on a stale/mismatched handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_struct_copy(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, src: u32, size: u32,
) -> u8 {
    let dst_val = (*frame).regs[dst as usize].clone();
    let src_val = (*frame).regs[src as usize].clone();
    match exec_struct::struct_copy_val(vm_ctx_ref(ctx), &dst_val, &src_val, size) {
        Ok(()) => 0,
        Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(format!("{e}").into())); 1 }
    }
}

/// `StructFieldGetPrim dst, base, byte_off, kind` — read the leaf at `byte_off`
/// (base = arena `StructRef` / heap `Object` inline field / `StructRefHeap` array
/// element). Returns 0 on success, 1 (+ exception) on a bad base / layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_struct_field_get_prim(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, base: u32, byte_off: u32, kind: u8,
) -> u8 {
    let base_val = (*frame).regs[base as usize].clone();
    match exec_struct::struct_field_get_val(vm_ctx_ref(ctx), &base_val, byte_off, kind) {
        Ok(v)  => { (*frame).regs[dst as usize] = v; 0 }
        Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(format!("{e}").into())); 1 }
    }
}

/// `StructFieldSetPrim base, byte_off, kind, val` — write the leaf in place (heap
/// bases route reference-leaf writes through a write barrier). Returns 0 on
/// success, 1 (+ exception) on a bad base / layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_struct_field_set_prim(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    base: u32, byte_off: u32, kind: u8, val: u32,
) -> u8 {
    let base_val = (*frame).regs[base as usize].clone();
    let v        = (*frame).regs[val as usize].clone();
    match exec_struct::struct_field_set_val(vm_ctx_ref(ctx), &base_val, byte_off, kind, &v) {
        Ok(())  => 0,
        Err(e)  => { set_exception(vm_ctx_ref(ctx), Value::Str(format!("{e}").into())); 1 }
    }
}

#[cfg(test)]
#[path = "struct_ops_tests.rs"]
mod struct_ops_tests;
