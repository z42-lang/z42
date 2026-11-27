#![allow(dangerous_implicit_autorefs)]
//! Exception-flow helpers: `throw`, `install_catch`, `match_catch_type`.
//! Mirror the interpreter's `Terminator::Throw` and `find_handler` paths.

use crate::object::Value;
use crate::jit::runtime::frame::JitFrame;
use crate::components::bridge::VmContext;
use super::{set_exception, take_exception, vm_ctx_ref};

/// `jit_throw`: `throw_offset` is the throw site's packed code offset (baked by
/// translate via `Function::linear_offset`, terminator slot). It is stamped onto
/// the throwing frame's `VmFrame::pc` before the stack is snapshotted into
/// `Std.Exception.StackTrace`; line / column are resolved from it there.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_throw(
    frame: *mut JitFrame, ctx: *const VmContext,
    reg: u32,
    throw_offset: u32,
) {
    let v = (*frame).regs[reg as usize].clone();
    let vm_ctx = vm_ctx_ref(ctx);
    let module = super::jit_module(ctx);
    vm_ctx.exec().set_top_frame_pc(throw_offset);
    crate::exec::exception::populate_stack_trace(&v, vm_ctx, module);
    set_exception(vm_ctx, v);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_install_catch(
    frame: *mut JitFrame, ctx: *const VmContext,
    catch_reg: u32,
) {
    let v = take_exception(vm_ctx_ref(ctx)).unwrap_or(Value::Null);
    (*frame).regs[catch_reg as usize] = v;
}

/// catch-by-generic-type : peek at the pending exception's runtime
/// class and return 1 if it is `target` (or a subclass) — i.e. matches a
/// `catch (target e)` clause. Returns 0 otherwise (or if there is no pending
/// exception / it is not an Object). The exception is left in place for a
/// later `jit_install_catch` call once a matching handler is selected.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_match_catch_type(
    _frame: *mut JitFrame, ctx: *const VmContext,
    target_ptr: *const u8, target_len: i64,
    key: *const crate::metadata::ids::tokens::TypeKeyCell,
) -> i8 {
    let target = match std::str::from_utf8(
        std::slice::from_raw_parts(target_ptr, target_len as usize)) {
        Ok(s)  => s,
        Err(_) => return 0,
    };
    let vm_ctx = vm_ctx_ref(ctx);
    let exc = match vm_ctx.exec().peek_exception() {
        Some(v) => v,
        None    => return 0,
    };
    let (td, inst_args) = match &exc {
        Value::Object(rc) => (rc.type_desc(), rc.type_args()),
        _                 => return 0, // primitives / null don't match typed catches
    };
    let module = super::jit_module(ctx);
    // `catch (Err<T>)` in a generic class body: bound from the receiver, own key.
    let dynamic = super::object::dynamic_target(_frame, ctx, target);
    let fresh = crate::metadata::ids::tokens::TypeKeyCell::new();
    // `key` is the exception-table row's key cell, baked into the JIT code with `target`.
    let (target, key) = match &dynamic { Some(t) => (t.as_str(), &fresh), None => (target, &*key) };
    // The thrown object's own type arguments count, as in `is` (`catch (Err<int>)` vs `Err<string>`).
    if crate::exec::ops::cast::isa_object(vm_ctx, &module.type_registry, td, inst_args, target, key) {
        1
    } else {
        0
    }
}

/// Slow branch of an inlined JIT safepoint poll. The VM owns the countdown
/// reset and pause protocol after the generated code decrements the counter.
///
/// # Safety
/// `ctx` must point to the running thread's live `VmContext`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_check_safepoint_slow(
    _frame: *mut JitFrame,
    ctx: *const VmContext,
) {
    crate::components::bridge::poll_safepoint_slow(vm_ctx_ref(ctx));
}

/// Prologue slow branch: the stack pointer is below the context's `JitHead::stack_limit`.
/// Records the fatal stack overflow (`stack_guard`) and leaves a pending
/// exception so the native caller returns the "thrown" code; the catch
/// dispatch then declines every handler while the fatal flag is set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_stack_overflow(_frame: *mut JitFrame, ctx: *const VmContext) {
    let vm_ctx = vm_ctx_ref(ctx);
    let err = crate::exec::stack_guard::overflow(vm_ctx);
    super::set_exception(vm_ctx, Value::Str(err.to_string().into()));
}

/// Non-zero while a fatal VM error unwinds — the JIT catch dispatch then
/// propagates instead of entering a handler (see `stack_guard`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_fatal_pending(_frame: *mut JitFrame, ctx: *const VmContext) -> i8 {
    crate::exec::stack_guard::is_fatal(vm_ctx_ref(ctx)) as i8
}

#[cfg(test)]
mod check_safepoint_tests {
    use super::*;

    #[test]
    fn jit_safepoint_slow_is_idle_without_a_request() {
        let ctx = VmContext::new();
        let cycles_before = ctx.heap().stats().gc_cycles;
        unsafe { jit_check_safepoint_slow(std::ptr::null_mut(), &*ctx); }
        assert_eq!(ctx.heap().stats().gc_cycles, cycles_before);
    }

    #[test]
    fn jit_safepoint_slow_drains_auto_collect_once() {
        let ctx = VmContext::new();
        let cycles_before = ctx.heap().stats().gc_cycles;
        crate::components::bridge::test_request_auto_collect(&ctx);

        unsafe { jit_check_safepoint_slow(std::ptr::null_mut(), &*ctx); }
        let cycles_after = ctx.heap().stats().gc_cycles;
        assert!(cycles_after > cycles_before, "poll should run a collection");

        unsafe { jit_check_safepoint_slow(std::ptr::null_mut(), &*ctx); }
        assert_eq!(ctx.heap().stats().gc_cycles, cycles_after, "poll should have drained the request");
    }

    #[test]
    fn safepoint_policy_is_shared_by_worker_resets() {
        for throttle in [1, 7, 1024] {
            let mut settings = z42::config::RuntimeConfig::from_getter(|_| None);
            settings.safepoint_throttle = throttle;
            let ctx = z42::components::bridge::jit_test_context(settings);
            let worker = VmContext::new_with_core(ctx.core().clone());
            for view in [&ctx, &worker] {
                view.threads().force_safepoint();
                unsafe { jit_check_safepoint_slow(std::ptr::null_mut(), &**view); }
                assert_eq!(crate::components::bridge::test_safepoint_counter(view), throttle);
            }
        }
    }

}
