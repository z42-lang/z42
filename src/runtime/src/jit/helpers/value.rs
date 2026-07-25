#![allow(dangerous_implicit_autorefs)]
//! Value-shuffling helpers: constants, copy, string formation, and the small
//! glue ops `get_bool` / `set_ret`.

use crate::corelib::convert::value_to_str;
use crate::metadata::Value;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

// ── Raw frame access (review.md C2 P1 step 1, 2026-05-28) ────────────────────
//
// `jit_regs_ptr` exposes `frame.regs.as_mut_ptr()` to JIT code. translate.rs
// calls this ONCE at function entry and caches the result; subsequent typed
// arithmetic ops compute `regs_base + reg_idx * size_of::<Value>()` inline
// and emit native Cranelift `load`/`store` against the slot, skipping the
// per-op helper call ABI.
//
// SAFETY: the returned pointer is valid for the lifetime of `frame.regs`,
// which is the JIT function's invocation duration. `JitFrame::new`
// pre-allocates with `take_pooled_regs(max_reg + 1)` and never grows the
// vector during execution → the data pointer never moves.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_regs_ptr(frame: *mut JitFrame) -> *mut Value {
    (*frame).regs.as_mut_ptr()
}

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
    let ctx_ref = &*ctx;
    // review.md C3 Phase 1 (2026-06-03): clone the pre-interned `Arc<str>`
    // (atomic refcount inc, zero heap alloc) — was `s.clone().into()`
    // (`String.clone()` + `String::into::<Arc<str>>()` = two allocs).
    match ctx_ref.string_pool.get(idx as usize) {
        Some(arc) => { (*frame).regs[dst as usize] = Value::Str(arc.clone()); 0 }
        None => {
            // make-vm-loading-lazy: an idx past the merged pool belongs to a
            // lazily-loaded zpkg — its ConstStr indices were remapped to absolute
            // (main_pool_len + offset). Resolve against the lazy loader's overflow
            // pool (mirrors interp's ConstStr overflow path). This is now reachable
            // because JIT compiles lazily-loaded functions to native (not just the
            // eagerly-merged closure).
            let vm = vm_ctx_ref(ctx);
            if let Some(arc) = vm.try_lookup_string(idx as usize) {
                (*frame).regs[dst as usize] = Value::Str(arc);
                return 0;
            }
            set_exception(vm, Value::Str(format!("string pool index {} out of range", idx).into()));
            1
        }
    }
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
            (*frame).regs[dst as usize] = Value::Str(format!("{}{}", sa, sb).into());
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
                let mut callee = JitFrame::new(entry.max_reg, &[val.clone()]);
                let jit_fn: JitFn = std::mem::transmute(entry.ptr);
                let vm_ctx = vm_ctx_ref(ctx);
                vm_ctx.push_frame(crate::exception::VmFrame::new(
                    entry.name.clone(), entry.file.clone(),
                    &callee.regs as *const _, &callee.env_arena as *const _));
                let r = jit_fn(&mut callee, ctx);
                vm_ctx.pop_frame();
                if r != 0 { callee.recycle(); return 1; }
                let s: std::sync::Arc<str> = match callee.ret.take() {
                    Some(Value::Str(s)) => s,
                    Some(ref other)     => value_to_str(other).into(),
                    None                => std::sync::Arc::from(""),
                };
                callee.recycle();
                (*frame).regs[dst as usize] = Value::Str(s);
                return 0;
            }
        }
        match crate::corelib::exec_builtin(
                vm_ctx_ref(ctx),
                crate::metadata::well_known_names::BUILTIN_OBJ_TO_STR,
                &[val.clone()]) {
            Ok(v) => { (*frame).regs[dst as usize] = Value::Str(match v { Value::Str(s) => s, ref o => value_to_str(o).into() }); }
            Err(e) => {
                set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into()));
                return 1;
            }
        }
    } else {
        (*frame).regs[dst as usize] = Value::Str(value_to_str(val).into());
    }
    0
}

// ── Branch / return glue ─────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_get_bool(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    reg: u32,
) -> u8 {
    match &(*frame).regs[reg as usize] {
        Value::Bool(b) => if *b { 1 } else { 0 },
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("BrCond: expected bool, got {:?}", other).into()));
            255
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_set_ret(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    reg: u32,
) {
    let v = (*frame).regs[reg as usize].clone();
    (*frame).ret = Some(v);
}
