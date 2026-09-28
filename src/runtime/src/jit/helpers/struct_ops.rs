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

/// 把 (ptr, len) 还原成 `&str` / `&[u16]`。SAFETY：指向模块函数体里的解码结果
/// （`str_val` / `path_val` 打的常量），生命周期覆盖这段 JIT 代码。
#[inline]
unsafe fn root_and_path<'a>(
    root_ptr: *const u8, root_len: u64, path_ptr: *const u16, path_len: u64,
) -> (&'a str, &'a [u16]) {
    let root = std::str::from_utf8_unchecked(std::slice::from_raw_parts(root_ptr, root_len as usize));
    let path = std::slice::from_raw_parts(path_ptr, path_len as usize);
    (root, path)
}

/// `StructFieldGetPrim dst, base, (root_type, path), kind` — read the named leaf
/// (base = arena `StructRef` / heap `Object` inline field / `StackObject` /
/// `BoxedStruct` / `StructRefHeap` array element). Returns 0 on success,
/// 1 (+ exception) on a bad base / layout / path.
///
/// symbolic-struct-field-access P2：解析走 `exec_struct::resolve_field_path` ——
/// 与 interp **同一个**实现。struct 路径上「判据两侧各抄一份」正是本 change 要消掉的东西，
/// 所以这里不重写一份解析。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_struct_field_get_prim(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, base: u32,
    root_ptr: *const u8, root_len: u64, path_ptr: *const u16, path_len: u64,
    kind: u8,
) -> u8 {
    let (root, path) = root_and_path(root_ptr, root_len, path_ptr, path_len);
    let base_val = (*frame).regs[base as usize].clone();
    let vm = vm_ctx_ref(ctx);
    let r = exec_struct::resolve_for_access(vm, root, path, &base_val, "StructFieldGetPrim")
        .and_then(|off| exec_struct::struct_field_get_val(vm, &base_val, off, kind));
    match r {
        Ok(v)  => { (*frame).regs[dst as usize] = v; 0 }
        Err(e) => { set_exception(vm, Value::Str(format!("{e}").into())); 1 }
    }
}

/// `StructFieldSetPrim base, (root_type, path), kind, val` — write the named leaf in
/// place (heap bases route reference-leaf writes through a write barrier). Returns 0
/// on success, 1 (+ exception) on a bad base / layout / path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_struct_field_set_prim(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    base: u32,
    root_ptr: *const u8, root_len: u64, path_ptr: *const u16, path_len: u64,
    kind: u8, val: u32,
) -> u8 {
    let (root, path) = root_and_path(root_ptr, root_len, path_ptr, path_len);
    let base_val = (*frame).regs[base as usize].clone();
    let v        = (*frame).regs[val as usize].clone();
    let vm = vm_ctx_ref(ctx);
    let r = exec_struct::resolve_for_access(vm, root, path, &base_val, "StructFieldSetPrim")
        .and_then(|off| exec_struct::struct_field_set_val(vm, &base_val, off, kind, &v));
    match r {
        Ok(())  => 0,
        Err(e)  => { set_exception(vm, Value::Str(format!("{e}").into())); 1 }
    }
}

#[cfg(test)]
#[path = "struct_ops_tests.rs"]
mod struct_ops_tests;
