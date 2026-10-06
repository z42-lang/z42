//! The one native-call sequence: `push_frame` → run compiled code → `pop_frame` →
//! recycle the callee frame. Every place that runs a compiled function — the JIT
//! entry (`JitModule::run_fn`), the call helpers (`jit_call` / `jit_vcall` /
//! `jit_call_indirect` / `jit_obj_new` / `jit_to_str`) and the interpreter's
//! mixed-mode diverts (`try_native_exec` / OSR / `try_native_static_call` /
//! `try_native_method_call`) — goes through [`call_native`]. What differs between
//! them (how the callee frame is filled, where the return value goes, whether the
//! exception stays pending or is taken) stays at the call site.

use std::sync::Arc;

use crate::metadata::Value;
use crate::vm_context::VmContext;

use super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::helpers::JitFn;

/// How a native call ended.
#[must_use]
pub(crate) enum NativeOutcome {
    /// Normal return; the value `jit_set_ret` stored (`None` for a void body).
    Returned(Option<Value>),
    /// The callee threw. The exception is still **pending** on the `VmContext`
    /// (`set_exception`) — JIT callers propagate it by returning `1`, interp
    /// callers take it (see [`NativeOutcome::into_exec`]).
    Threw,
}

impl NativeOutcome {
    /// JIT-helper shape: store the return value (`Null` for void) into
    /// `regs[dst]` and report `0`, or report `1` with the exception left pending.
    pub(crate) fn store_into(self, regs: &mut [Value], dst: u32) -> u8 {
        match self {
            NativeOutcome::Returned(ret) => { regs[dst as usize] = ret.unwrap_or(Value::Null); 0 }
            NativeOutcome::Threw => 1,
        }
    }

    /// Interpreter shape: a throw takes the pending exception off the context.
    pub(crate) fn into_exec(self, vm: &VmContext) -> crate::interp::ExecOutcome {
        match self {
            NativeOutcome::Returned(ret) => crate::interp::ExecOutcome::Returned(ret),
            NativeOutcome::Threw =>
                crate::interp::ExecOutcome::Thrown(vm.take_exception().unwrap_or(Value::Null)),
        }
    }
}

/// Run compiled code `code` on the already-filled `frame`.
///
/// The frame is enrolled as a GC root + stack-trace row (one `VmFrame` named
/// `name` / `file`) for exactly the duration of the native call, then recycled.
/// Nothing between the caller building `frame` and the `push_frame` here may
/// reach a safepoint: until the push, the callee's arguments are reachable only
/// from `frame`. The stack-overflow check runs in the callee's prologue, i.e.
/// after the push, so a fatal report includes this frame.
///
/// `name` / `file` are taken by value so interp callers can copy them out of the
/// `FnEntry` before the call (no borrow of the `JitModuleCtx` slot held across it).
///
/// # Safety
/// `code` must be a compiled `JitFn` from the module `jit_ctx` points at, and
/// `jit_ctx` must be valid with its `vm_ctx` set to `vm` (true for the whole of
/// `JitModule::run_fn`).
pub(crate) unsafe fn call_native(
    vm: &VmContext, jit_ctx: *const JitModuleCtx,
    code: *const u8, name: Arc<str>, file: Arc<str>, mut frame: JitFrame,
) -> NativeOutcome {
    let jit_fn: JitFn = unsafe { std::mem::transmute(code) };
    vm.push_frame(crate::exception::VmFrame::new(
        name, file, &frame.regs as *const _, &frame.env_arena as *const _));
    let r = unsafe { jit_fn(&mut frame, jit_ctx) };
    vm.pop_frame();
    if r != 0 {
        frame.recycle();
        return NativeOutcome::Threw;
    }
    let ret = frame.ret.take();
    frame.recycle();
    NativeOutcome::Returned(ret)
}

/// [`call_native`] for a borrowed compiled entry (the JIT helpers' case).
///
/// # Safety
/// As [`call_native`]; `entry` must belong to `jit_ctx`.
#[inline]
pub(crate) unsafe fn call_entry(
    vm: &VmContext, jit_ctx: *const JitModuleCtx, entry: &FnEntry, frame: JitFrame,
) -> NativeOutcome {
    unsafe { call_native(vm, jit_ctx, entry.ptr, entry.name.clone(), entry.file.clone(), frame) }
}
