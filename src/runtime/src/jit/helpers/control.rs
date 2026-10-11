#![allow(dangerous_implicit_autorefs)]
//! Exception-flow helpers: `throw`, `install_catch`, `match_catch_type`.
//! Mirror the interpreter's `Terminator::Throw` and `find_handler` paths.

use crate::metadata::Value;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, take_exception, vm_ctx_ref};

/// `jit_throw`: `throw_offset` is the throw site's packed code offset (baked by
/// translate via `Function::linear_offset`, terminator slot). It is stamped onto
/// the throwing frame's `VmFrame::pc` before the stack is snapshotted into
/// `Std.Exception.StackTrace`; line / column are resolved from it there.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_throw(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    reg: u32,
    throw_offset: u32,
) {
    let v = (*frame).regs[reg as usize].clone();
    let vm_ctx = vm_ctx_ref(ctx);
    let module = &*(*ctx).module;
    vm_ctx.set_top_frame_pc(throw_offset);
    crate::exception::populate_stack_trace(&v, vm_ctx, module);
    set_exception(vm_ctx, v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_install_catch(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    catch_reg: u32,
) {
    let v = take_exception(vm_ctx_ref(ctx)).unwrap_or(Value::Null);
    (*frame).regs[catch_reg as usize] = v;
}

/// catch-by-generic-type (2026-05-06): peek at the pending exception's runtime
/// class and return 1 if it is `target` (or a subclass) — i.e. matches a
/// `catch (target e)` clause. Returns 0 otherwise (or if there is no pending
/// exception / it is not an Object). The exception is left in place for a
/// later `jit_install_catch` call once a matching handler is selected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_match_catch_type(
    _frame: *mut JitFrame, ctx: *const JitModuleCtx,
    target_ptr: *const u8, target_len: i64,
    key: *const crate::metadata::tokens::TypeKeyCell,
) -> i8 {
    let target = match std::str::from_utf8(
        std::slice::from_raw_parts(target_ptr, target_len as usize)) {
        Ok(s)  => s,
        Err(_) => return 0,
    };
    let vm_ctx = vm_ctx_ref(ctx);
    let exc = match vm_ctx.peek_exception() {
        Some(v) => v,
        None    => return 0,
    };
    let td = match &exc {
        Value::Object(rc) => rc.type_desc(),
        _                 => return 0, // primitives / null don't match typed catches
    };
    let module = &*(*ctx).module;
    // `key` is the exception-table row's key cell, baked into the JIT code with `target`.
    if crate::interp::dispatch::isa_td(vm_ctx, &module.type_registry, td, target, &*key) {
        1
    } else {
        0
    }
}

/// `jit_check_safepoint` (add-gc-safepoint-jit, 2026-05-21): JIT-emitted
/// code calls this at each safepoint insertion site — function entry,
/// backward Br terminators, BrCond terminators, and after Call /
/// CallIndirect helpers return. Thin trampoline into the shared
/// `gc::safepoint::check_safepoint` so the JIT path follows the same
/// Idle / Requested / Marking protocol as interp.
///
/// `_frame` is unused (signature kept for ABI uniformity with the rest of
/// the JIT helpers — every helper is `(frame, ctx, ...)`).
///
/// Safety: `ctx` must point to a valid `JitModuleCtx` whose `vm_ctx`
/// dereferences to a live `VmContext`. The JIT entry path (`JitModule::run`)
/// guarantees this for the duration of any compiled function call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_check_safepoint(
    _frame: *mut JitFrame,
    ctx:    *const JitModuleCtx,
) {
    let vm_ctx = vm_ctx_ref(ctx);
    crate::gc::safepoint::check_safepoint(vm_ctx);
}

/// `jit_check_safepoint_slow` (inline-jit-safepoint-check, 2026-08-01): the
/// rare slow branch of the JIT-inlined safepoint fast path. When the inlined
/// `load/store` decrement observes the throttle counter reaching 0, native code
/// branches here to (1) reset the counter to [`throttle_n`] and (2) run the
/// real Mutex + phase-check + auto-collect-drain via [`check_safepoint_slow`].
///
/// This mirrors the tail of [`crate::gc::safepoint::check_safepoint`] (the part
/// after the `prev > 1` early return) — the fast decrement itself is emitted
/// inline by `jit::translate::emit_safepoint_check`, so this helper is called
/// only ~0.1% of the time (once per [`throttle_n`] back-edges).
///
/// `_frame` is unused (kept for ABI uniformity: every JIT helper is
/// `(frame, ctx, ...)`).
///
/// Safety: same contract as [`jit_check_safepoint`] — `ctx` must point to a
/// valid `JitModuleCtx` whose `vm_ctx` is a live `VmContext`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_check_safepoint_slow(
    _frame: *mut JitFrame,
    ctx:    *const JitModuleCtx,
) {
    let vm_ctx = vm_ctx_ref(ctx);
    vm_ctx.safepoint_skip.store(
        crate::gc::safepoint::throttle_n(),
        std::sync::atomic::Ordering::Relaxed,
    );
    crate::gc::safepoint::check_safepoint_slow(vm_ctx);
}

/// Prologue slow branch: the stack pointer is below `JitModuleCtx::stack_limit`.
/// Records the fatal stack overflow (`stack_guard`) and leaves a pending
/// exception so the native caller returns the "thrown" code; the catch
/// dispatch then declines every handler while the fatal flag is set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_stack_overflow(_frame: *mut JitFrame, ctx: *const JitModuleCtx) {
    let vm_ctx = vm_ctx_ref(ctx);
    let err = crate::stack_guard::overflow(vm_ctx);
    super::set_exception(vm_ctx, Value::Str(err.to_string().into()));
}

/// Non-zero while a fatal VM error unwinds — the JIT catch dispatch then
/// propagates instead of entering a handler (see `stack_guard`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_fatal_pending(_frame: *mut JitFrame, ctx: *const JitModuleCtx) -> i8 {
    crate::stack_guard::is_fatal(vm_ctx_ref(ctx)) as i8
}

#[cfg(test)]
mod check_safepoint_tests {
    //! add-gc-safepoint-jit (2026-05-21): inline tests for the
    //! `jit_check_safepoint` trampoline. End-to-end JIT-compiled coverage
    //! requires building a real JIT-compiled function via
    //! `jit::compile_module` (heavy fixture); these tests cover the
    //! trampoline ABI + the protocol routing.

    use super::*;
    use crate::vm_context::VmContext;
    use std::sync::atomic::Ordering;

    fn make_jit_ctx(vm_ctx: &VmContext) -> (JitModuleCtx, JitFrame) {
        // module pointer dangles for the test — check_safepoint never
        // dereferences it.
        let mut jit_ctx = JitModuleCtx::new(std::ptr::null(), std::ptr::null(), 1, 10_000);
        jit_ctx.vm_ctx = vm_ctx as *const VmContext as *mut VmContext;
        (jit_ctx, JitFrame::new(vm_ctx, 0, &[]))
    }

    #[test]
    fn jit_check_safepoint_idle_is_no_op_fast_path() {
        // Idle phase + no pending auto-collect: trampoline should return
        // immediately without touching gc_cycles.
        let ctx = VmContext::new();
        let cycles_before = ctx.heap().stats().gc_cycles;
        let (jit_ctx, mut frame) = make_jit_ctx(&ctx);
        unsafe {
            jit_check_safepoint(
                &mut frame as *mut JitFrame,
                &jit_ctx as *const JitModuleCtx,
            );
        }
        assert_eq!(ctx.heap().stats().gc_cycles, cycles_before,
            "Idle path should be a no-op");
    }

    #[test]
    fn jit_check_safepoint_drains_pending_auto_collect() {
        // Pre-set the needs_auto_collect flag; trampoline should reach
        // gc::safepoint::check_safepoint which atomically swaps it and
        // runs a stop-the-world collect.
        let ctx = VmContext::new();
        // add-gc-safepoint-counter-throttling (2026-05-21): force the
        // first check_safepoint into the slow path (otherwise the
        // throttle counter would skip it).
        ctx.safepoint_skip.store(1, Ordering::Relaxed);
        let cycles_before = ctx.heap().stats().gc_cycles;
        ctx.core.needs_auto_collect.store(true, Ordering::Release);

        let (jit_ctx, mut frame) = make_jit_ctx(&ctx);
        unsafe {
            jit_check_safepoint(
                &mut frame as *mut JitFrame,
                &jit_ctx as *const JitModuleCtx,
            );
        }
        assert!(!ctx.core.needs_auto_collect.load(Ordering::Acquire),
            "trampoline should have drained the flag");
        assert!(ctx.heap().stats().gc_cycles > cycles_before,
            "trampoline should have run a real collect");
    }
}
