#![allow(dangerous_implicit_autorefs)]
//! JIT field-access helpers: the hoisted non-throwing field-slot resolvers
//! (`jit_obj_field_slot` / `jit_obj_ref_field_slot`) and the general
//! `jit_field_get` / `jit_field_set` runtime calls they fall back to.
//!
//! Split out of `object.rs` (which keeps object allocation, type tests and
//! static fields) — that file was over the 500-line limit and may not grow.

use crate::metadata::Value;

use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref};

// ── Field access ─────────────────────────────────────────────────────────────

/// post-layout JIT perf (P5-B): **non-throwing** byte-aware field resolver for the
/// loop-invariant hoist. Emitted ONCE in the JIT entry block for an object register
/// proven never-reassigned (e.g. `this`) + a fixed field name. If the field is a
/// direct **inline primitive** (scalar packed in `bytes`) whose runtime width/tag
/// match the JIT's compile-time expectation (from `reg_types`), writes
/// `out_bytes_ptr = bytes.as_ptr()` and `out_off = byte offset`; the per-`FieldGet`/
/// `FieldSet` inline then does a native width-aware byte load/store. Otherwise
/// (non-object / null / field-not-found / reference / inlined obj-array ref /
/// struct root / string / width-or-tag mismatch) writes `out_off = -1` and does NOT
/// throw — the inline detects `off < 0` and falls back to `jit_field_get`/
/// `jit_field_set` (correct exception / Str.Length / write-barrier semantics at the
/// real site). GC-safe: non-moving collector + fixed `bytes` allocation + the object
/// is held live ⇒ the returned ptr stays valid for the frame.
///
/// unify-object-byte-layout (PR-2) had stubbed this to always signal "no fast path"
/// (byte storage broke the old `slots.as_ptr()` + STRIDE assumption); P5-B restores
/// it against the `bytes` layout via `ScriptObject::inline_prim_field`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_field_slot(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    obj: u32, field_name_ptr: *const u8, field_name_len: usize,
    expected_width: u32, expected_tag: u32,
    out_bytes_ptr: *mut *const u8, out_off: *mut i64,
) {
    // default: no fast path → the inline sees off<0 and routes to the helper.
    *out_bytes_ptr = std::ptr::null();
    *out_off = -1;
    let field_name = match std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len)) {
        Ok(s) => s,
        Err(_) => return,
    };
    let Value::Object(rc) = &(*frame).regs[obj as usize] else { return };
    let b = rc.borrow();
    if let Some((ptr, off, width, tag)) = b.inline_prim_field(field_name) {
        // Confirm the runtime layout matches the JIT's compile-time (width, tag) —
        // guards against aliases / layout surprises: a mismatch keeps the helper.
        if width == expected_width && tag as u32 == expected_tag {
            *out_bytes_ptr = ptr;
            *out_off = off as i64;
        }
    }
}

/// post-layout JIT perf (T1-B): **non-throwing** byte-inlined **reference** field
/// resolver for the loop-invariant hoist — the reference twin of
/// [`jit_obj_field_slot`]. Emitted ONCE in the JIT entry block for an object register
/// proven never-reassigned (e.g. `this`) + a fixed field name. If the field is a
/// direct byte-inlined class-instance or array reference (8B tagged pointer in
/// `bytes`, `ref_slot == -1`), writes `out_bytes_ptr = bytes.as_ptr()`,
/// `out_off = byte offset`, and `out_tag` = the `Value` discriminant to stamp on a
/// non-null load (`7` = `Value::Object` / `6` = `Value::Array`); the per-`FieldGet`
/// inline then does a native 8B load + `raw==0 ? Null : tagged store`, byte-identical
/// to `read_inline_ref`. Otherwise (non-object receiver / null / field-not-found /
/// primitive / side-table reference = closure·func·**string** / struct root) writes
/// `out_off = -1` and does NOT throw — the inline detects `off < 0` and falls back to
/// `jit_field_get` (correct Str.Length / null-throw / string-GcRef semantics at the
/// real site). GC-safe: non-moving collector + fixed `bytes` allocation + the object
/// held live ⇒ the returned ptr stays valid for the frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_ref_field_slot(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    obj: u32, field_name_ptr: *const u8, field_name_len: usize,
    out_bytes_ptr: *mut *const u8, out_off: *mut i64, out_tag: *mut i32,
) {
    // default: no fast path → the inline sees off<0 and routes to the helper.
    *out_bytes_ptr = std::ptr::null();
    *out_off = -1;
    *out_tag = 0;
    let field_name = match std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len)) {
        Ok(s) => s,
        Err(_) => return,
    };
    let Value::Object(rc) = &(*frame).regs[obj as usize] else { return };
    let b = rc.borrow();
    if let Some((ptr, off, is_array)) = b.inline_ref_field(field_name) {
        *out_bytes_ptr = ptr;
        *out_off = off as i64;
        // `Value` `#[repr(C, u8)]` discriminants (pinned by `value_discriminants_pinned`
        // in `metadata/types_tests.rs`): `Array` = 6, `Object` = 7.
        *out_tag = if is_array { 6 } else { 7 };
    }
}

/// `jit_field_get` after formalize-jit-method-token Phase 2.E (2026-05-08):
/// per-site `FieldIC` is threaded in (stable raw pointer baked at codegen).
/// Mirrors interp `field_get` — IC hit fetches `slots[cached_slot]` directly;
/// miss walks `field_index` and writes (TypeId, slot) to IC. Non-Object
/// receivers (Str / Array) bypass the IC since their field set is hardcoded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_get(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32,
    field_name_ptr: *const u8, field_name_len: usize,
    ic_ptr: *const crate::metadata::resolver::FieldIC,
) -> u8 {
    let field_name = std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len))
        .unwrap_or("<invalid>");
    let obj_val = &(*frame).regs[obj as usize];
    let val = match obj_val {
        // fix-jit-osr-stackobject: under OSR the interp portion may have created a
        // stack-allocated object (escape analysis) that is live in `frame.regs` when
        // the JIT takes over. Mirror interp `field_get` — resolve via the per-context
        // stack arena, reusing the same monomorphic `FieldIC` as the heap path.
        // (Non-OSR JIT never produces a StackObject, so this arm only fires on the OSR
        // entry path. See `jit_array_get` for the StackArray analogue.)
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            let res = vm_ctx_ref(ctx).stack_arena.lock().with_obj(idx, frame_id, |obj| {
                if !ic_ptr.is_null() {
                    let recv_type = obj.type_desc.id.0;
                    if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                        return obj.field_value(slot as usize);
                    }
                    if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                        return obj.field_value(slot);
                    }
                    return Value::Null;
                }
                match obj.type_desc.field_index.get(field_name) {
                    Some(&slot) => obj.field_value(slot),
                    None => Value::Null,
                }
            });
            match res {
                Ok(v) => v,
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
            }
        }
        Value::Object(rc) => {
            let b = rc.borrow();
            // PIC fast path (review.md C4 P2 — 4-slot linear scan).
            if !ic_ptr.is_null() {
                let recv_type = b.type_desc.id.0;
                if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                    let v = b.field_value(slot as usize);
                    (*frame).regs[dst as usize] = v;
                    return 0;
                }
                if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                    crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                    b.field_value(slot)
                } else { Value::Null }
            } else if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                b.field_value(slot)
            } else { Value::Null }
        }
        Value::Str(s) if field_name == "Length"     => Value::I64(crate::corelib::str_meta::char_len(s) as i64),
        Value::Str(s) if field_name == "ByteLength" => Value::I64(s.len() as i64),
        Value::Array(rc) if field_name == "Length" || field_name == "Count" => Value::I64(rc.borrow().len() as i64),
        // fix-jit-field-get-stackarray: the `StackArray` analogue of the `StackObject`
        // arm above — a stack-allocated array (escape analysis) reaching the JIT and
        // having its `.Length` read. `jit_array_get` already resolves StackArray via
        // the stack arena (three sites); this path was the missing counterpart, so any
        // `arr.Length` on a stack-allocated array fell through to the catch-all and
        // raised `FieldGet: expected object, got StackArray`.
        // Repro (pre-fix: interp OK, JIT throws): a stdlib method that allocates
        // `new char[1]` and hands it to a sibling overload which reads `.Length`
        // (`Std.String.Trim(char)` → `Trim(char[])`, added by augment-string-prelude).
        Value::StackArray { idx: aidx, frame_id } if field_name == "Length" || field_name == "Count" => {
            let (aidx, frame_id) = (*aidx, *frame_id);
            match vm_ctx_ref(ctx).stack_arena.lock().with_arr(aidx, frame_id, |a| a.len()) {
                Ok(n) => Value::I64(n as i64),
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
            }
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("FieldGet: expected object, got {:?}", other).into()));
            return 1;
        }
    };
    (*frame).regs[dst as usize] = val;
    0
}

/// JIT FieldSet helper.
///
/// **add-write-barriers (2026-05-21)**: dispatches `write_barrier_field`
/// after a successful slot write *iff* `v.is_heap_ref()`. Mirrors
/// `interp::exec_object::field_set` — primitive writes skip dispatch
/// (Decision 1); both IC fast and slow paths fire (Decision 5).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    obj: u32,
    field_name_ptr: *const u8, field_name_len: usize, val: u32,
    ic_ptr: *const crate::metadata::resolver::FieldIC,
) -> u8 {
    let field_name = std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len))
        .unwrap_or("<invalid>");
    let v = (*frame).regs[val as usize].clone();
    let owner = (*frame).regs[obj as usize].clone();
    match &owner {
        // fix-jit-osr-stackobject: OSR-entry stack object — write the slot in the
        // arena (validated), mirroring interp `field_set`. No GC write barrier: the
        // stack object is not a heap slot; its heap-ref fields are kept live by
        // root-scanning the arena. Reuses the same FieldIC as the heap path. See
        // `jit_field_get` / `jit_array_set`.
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            let res = vm_ctx_ref(ctx).stack_arena.lock().with_obj_mut(idx, frame_id, |obj| {
                let slot_opt: Option<usize> = if !ic_ptr.is_null() {
                    let recv_type = obj.type_desc.id.0;
                    if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                        Some(slot as usize)
                    } else if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                        Some(slot)
                    } else {
                        None
                    }
                } else {
                    obj.type_desc.field_index.get(field_name).copied()
                };
                if let Some(slot) = slot_opt {
                    obj.set_field_value(slot, &v);
                }
            });
            match res {
                Ok(()) => 0,
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
            }
        }
        Value::Object(rc) => {
            let mut b = rc.borrow_mut();
            // PIC fast path
            if !ic_ptr.is_null() {
                let recv_type = b.type_desc.id.0;
                if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                    let slot = slot as usize;
                    let wrote_ref = b.set_field_value(slot, &v);
                    drop(b);
                    if wrote_ref && v.is_heap_ref() {
                        vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                    }
                    return 0;
                }
                let slot_opt = b.type_desc.field_index.get(field_name).copied();
                if let Some(slot) = slot_opt {
                    crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                    let wrote_ref = b.set_field_value(slot, &v);
                    drop(b);
                    if wrote_ref && v.is_heap_ref() {
                        vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                    }
                }
            } else if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                let wrote_ref = b.set_field_value(slot, &v);
                drop(b);
                if wrote_ref && v.is_heap_ref() {
                    vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                }
            }
            0
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("FieldSet: expected object, got {:?}", other).into()));
            1
        }
    }
}
