/// `extern "C"` helper functions called by JIT-compiled code, plus the
/// shared utilities and helper-table registry.
///
/// Architecture
/// ------------
/// * Each `Instruction` category lives in its own submodule, mirroring
///   the interpreter's `interp/exec_*.rs` split (see `docs/internals/src/runtime/vm-architecture.md`
///   §"JIT/EE helper 边界")
/// * `registry.rs` is the single source of truth for the helper set:
///   it owns `HelperIds` (one `FuncId` per helper) and the two registration
///   functions consumed by `jit/mod.rs` and `jit/translate.rs`
/// * `mod.rs` (this file) keeps the small set of cross-cutting utilities
///   that every helper needs (`vm_ctx_ref`, `set_exception`, ...) so
///   submodules can `use super::*;`
///
/// Convention
/// ----------
///   * Functions that can fail return `u8`: 0 = success, 1 = exception
///     (stored on `VmContext` via `set_exception`)
///   * Functions that cannot fail return `()`
///   * Every helper takes `frame: *mut JitFrame, ctx: *const VmContext`
///     as the first two parameters

pub mod arith;
pub mod array;
pub mod call;
pub mod closure;
pub mod control;
pub mod object;
pub mod object_field;
pub mod registry;
pub mod struct_ops;
pub mod value;
pub mod vcall;

pub use registry::{declare_imports, register_symbols, HelperIds};

use crate::vm::runtime::Module;
use crate::metadata::Value;
use crate::vm_context::VmContext;

use super::frame::JitFrame;
use super::runtime::JitRuntime;

// ─── ABI version ────────────────────────────────────────────────────────────
//
// Bumped whenever the helper set or any helper signature changes. There is
// no runtime version check in the current single-JIT-implementation regime —
// this constant exists as a hook for future tier-up / multiple JIT backend
// scenarios (review.md Part 4 §4.2). When that arrives, the consumer will
// fail the JIT init if its compiled-against version doesn't match.

#[allow(dead_code)] // hook for future tier-up / multiple JIT backend version-mismatch detection
pub const VM_JIT_INTERFACE_VERSION: u32 = 3;

// ─── Context access ─────────────────────────────────────────────────────────
//
// Every JIT helper receives the running thread's `*const VmContext` as its 2nd
// parameter (after `*mut JitFrame`) — the same pointer the compiled caller got.
// Per-thread state (frame stack, pending exception, arenas, register pool,
// counters) is a field of it; the VM's shared JIT state is `vm.jit()` (`VmCore::jit`,
// cached in the context's `JitHead`).

/// Borrow the running thread's context for the duration of the helper call.
///
/// SAFETY: `ctx` is the context parameter compiled code was called with
/// (`jit::invoke::enter_compiled`): non-null, live, driven by this thread. The
/// returned reference must not outlive the helper call.
#[inline(always)]
pub(super) unsafe fn vm_ctx_ref<'a>(ctx: *const VmContext) -> &'a VmContext {
    &*ctx
}

/// The VM's shared JIT runtime. Compiled code only exists once `jit::run`
/// installed it, so a helper always finds it.
///
/// SAFETY: as [`vm_ctx_ref`].
#[inline]
pub(super) unsafe fn jit_rt<'a>(ctx: *const VmContext) -> &'a JitRuntime {
    (*ctx).jit().expect("JIT helper called on a VM without the JIT installed")
}

/// The module the JIT's ids are relative to (the one compiled code executes
/// against).
///
/// SAFETY: as [`vm_ctx_ref`].
#[inline]
pub(super) unsafe fn jit_module<'a>(ctx: *const VmContext) -> &'a Module {
    jit_rt(ctx).module()
}

pub(super) fn set_exception(ctx: &VmContext, v: Value) {
    ctx.set_exception(v);
}

/// objops 错误 → JIT 的异常通道：物化成异常值塞进 pending 槽，返回 1（translate 端 `check`
/// 据此跳异常分支）。与 interp 的 `interp::ops::raise` 共用 `OpError::into_exception`，异常类与
/// 消息逐字一致；内部错误退化为字符串异常（JIT helper 没有别的出口）。
///
/// # Safety
/// `ctx` 是 helper 收到的线程 `VmContext` 指针（单测的最小 ctx 可能没装 JIT——回落到 VM 自己的 module）。
#[cold]
#[inline(never)]
pub(super) unsafe fn raise(ctx: *const VmContext, e: crate::objops::OpError) -> u8 {
    let vm = vm_ctx_ref(ctx);
    let module = match vm.jit() { Some(rt) => Some(rt.module()), None => vm.module().map(|m| &**m) };
    let exc = match e.into_exception(vm, module) {
        Ok(v) => v,
        Err(err) => Value::Str(err.to_string().into()),
    };
    set_exception(vm, exc);
    1
}

pub(super) fn take_exception(ctx: &VmContext) -> Option<Value> {
    ctx.take_exception()
}

pub fn take_exception_error(ctx: &VmContext, module: &crate::vm::runtime::Module) -> anyhow::Error {
    let msg = take_exception(ctx)
        .as_ref()
        .map(|v| crate::exception::format_uncaught(v, ctx, module))
        .unwrap_or_else(|| "uncaught exception".to_owned());
    anyhow::anyhow!("{}", msg)
}

// ─── JIT function type alias ────────────────────────────────────────────────

pub type JitFn = unsafe extern "C" fn(frame: *mut JitFrame, ctx: *const VmContext) -> u8;

// ─── Shared numeric helpers ─────────────────────────────────────────────────
//
// converge-vm-arith-semantics (H3): the scalar rules formerly duplicated here
// (`int_binop_helper` / `int_bitop_helper` / `numeric_lt_helper` were verbatim
// copies of `interp/ops.rs`) now live once in `crate::semantics`. Call sites in
// `arith.rs` use `crate::semantics::{int_binop, int_bitop, numeric_lt}` directly.

/// A name the translator baked into the code as `(ptr, len)` (`TxCtx::str_val`).
/// It points into a Rust `str` owned by the module's IR, which outlives the
/// compiled code — so it is valid UTF-8 by construction, and re-validating it
/// on every helper call (`from_utf8`) was measurable on field and type-test
/// hot paths.
///
/// # Safety
/// `(ptr, len)` must come from `TxCtx::str_val`.
#[inline(always)]
pub(crate) unsafe fn baked_str<'a>(ptr: *const u8, len: usize) -> &'a str {
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) }
}
