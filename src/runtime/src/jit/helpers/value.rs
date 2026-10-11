#![allow(dangerous_implicit_autorefs)]
//! Value-shuffling helpers: constants, copy, string formation, and the small
//! glue op `get_bool`. (`Ret` and the prologue's register-file pointer are
//! inline loads/stores on `JitFrame` — see `JIT_FRAME_*_OFFSET`.)

use crate::corelib::convert::with_value_str;
use crate::metadata::Value;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::super::invoke::{call_entry, NativeOutcome};
use super::{set_exception, vm_ctx_ref};

// ── Constants ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_i32(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, val: i32,
) {
    (*frame).regs[dst as usize] = Value::I64(val as i64);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_i64(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, val: i64,
) {
    (*frame).regs[dst as usize] = Value::I64(val);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_f64(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, val: f64,
) {
    (*frame).regs[dst as usize] = Value::F64(val);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_bool(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, val: u8,
) {
    (*frame).regs[dst as usize] = Value::Bool(val != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_char(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, val: i32,
) {
    (*frame).regs[dst as usize] = Value::Char(char::from_u32(val as u32).unwrap_or('\0'));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_null(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32,
) {
    (*frame).regs[dst as usize] = Value::Null;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_const_str(
    frame: *mut JitFrame,
    ctx:   *const JitModuleCtx,
    dst:   u32,
    idx:   u32,
) -> u8 {
    let vm = vm_ctx_ref(ctx);
    // Same lookup as interp `const_str`: the VM's string table, lock-free once interned.
    if let Some(s) = vm.const_str(&*(*ctx).module, idx) {
        (*frame).regs[dst as usize] = Value::Str(s);
        return 0;
    }
    set_exception(vm, Value::Str(format!("string pool index {} out of range", idx).into()));
    1
}

// ── Copy ─────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_copy(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, src: u32,
) {
    let v = (*frame).regs[src as usize].clone();
    (*frame).regs[dst as usize] = v;
}

// ── String ───────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_str_concat(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) -> u8 {
    match (&(*frame).regs[a as usize], &(*frame).regs[b as usize]) {
        (Value::Str(sa), Value::Str(sb)) => {
            // fuse-str-concat-alloc: one fused GC block, no intermediate `format!` String.
            let s = vm_ctx_ref(ctx).heap().alloc_str_concat2(sa, sb);
            (*frame).regs[dst as usize] = Value::Str(s);
            0
        }
        (va, vb) => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("StrConcat: expected two strings, got {:?} and {:?}", va, vb).into()));
            1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_to_str(
    frame: *mut JitFrame, ctx: *const JitModuleCtx, dst: u32, src: u32,
) -> u8 {
    let val = &(*frame).regs[src as usize];
    if let Value::Object(rc) = val {
        let type_desc = rc.type_desc_arc().clone();
        let func_name_opt = type_desc.vtable_index.get("ToString")
            .map(|&slot| type_desc.vtable[slot].1.clone());
        if let Some(func_name) = func_name_opt {
            let ctx_ref = &*ctx;
            if let Some(entry) = ctx_ref.resolve_fn_by_name(func_name.as_str()) {
                let callee = JitFrame::new(vm_ctx_ref(ctx), entry.max_reg, &[val.clone()]);
                let ret = match call_entry(vm_ctx_ref(ctx), ctx, entry, callee) {
                    NativeOutcome::Returned(ret) => ret,
                    NativeOutcome::Threw => return 1,
                };
                let heap = vm_ctx_ref(ctx).heap();
                let s: crate::metadata::vstr::Str = match ret {
                    Some(Value::Str(s)) => s,
                    Some(ref other)     => with_value_str(other, |t| heap.alloc_str(t)),
                    None                => heap.alloc_str(""),
                };
                (*frame).regs[dst as usize] = Value::Str(s);
                return 0;
            }
        }
    }
    // Not natively callable (no `ToString` override, or one the JIT can't run)
    // or a boxed struct: the interpreter's stringification, same as `ToStr` /
    // `+` under interp. A throwing `ToString` surfaces as its own exception.
    // perf-str-concat-direct: format straight into the GC string (no intermediate `String`).
    let heap = vm_ctx_ref(ctx).heap();
    if matches!(val, Value::Object(_) | Value::BoxedStruct(_)) {
        let v = *val;
        match crate::interp::dispatch::stringify_dispatch_gc(vm_ctx_ref(ctx), &v) {
            Ok(s) => (*frame).regs[dst as usize] = Value::Str(s),
            Err(e) => return super::arith::stringify_failed(ctx, e),
        }
    } else {
        (*frame).regs[dst as usize] = Value::Str(with_value_str(val, |t| heap.alloc_str(t)));
    }
    0
}

// ── Branch / return glue ─────────────────────────────────────────────────────

/// `jit_get_bool`'s return value for a non-Bool condition (exception set).
/// The BrCond translation tests for it before branching.
pub(crate) const JIT_GET_BOOL_ERR: u8 = 255;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_get_bool(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    reg: u32,
) -> u8 {
    match &(*frame).regs[reg as usize] {
        Value::Bool(b) => if *b { 1 } else { 0 },
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("BrCond: expected bool, got {:?}", other).into()));
            JIT_GET_BOOL_ERR
        }
    }
}
