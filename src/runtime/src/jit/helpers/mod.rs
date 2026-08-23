/// `extern "C"` helper functions called by JIT-compiled code, plus the
/// shared utilities and helper-table registry.
///
/// Architecture
/// ------------
/// * Each `Instruction` category lives in its own submodule, mirroring
///   the interpreter's `interp/exec_*.rs` split (see `docs/design/runtime/vm-architecture.md`
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
///   * Every helper takes `frame: *mut JitFrame, ctx: *const JitModuleCtx`
///     as the first two parameters

pub mod arith;
pub mod array;
pub mod call;
pub mod closure;
pub mod control;
pub mod object;
pub mod registry;
pub mod struct_ops;
pub mod value;
pub mod vcall;

pub use registry::{declare_imports, register_symbols, HelperIds};

use crate::metadata::Value;
use crate::vm_context::VmContext;

use super::frame::{JitFrame, JitModuleCtx};

// ─── ABI version ────────────────────────────────────────────────────────────
//
// Bumped whenever the helper set or any helper signature changes. There is
// no runtime version check in the current single-JIT-implementation regime —
// this constant exists as a hook for future tier-up / multiple JIT backend
// scenarios (review.md Part 4 §4.2). When that arrives, the consumer will
// fail the JIT init if its compiled-against version doesn't match.

#[allow(dead_code)] // hook for future tier-up / multiple JIT backend version-mismatch detection
pub const VM_JIT_INTERFACE_VERSION: u32 = 1;

// ─── VmContext access via JitModuleCtx ──────────────────────────────────────
//
// Every JIT helper receives `*const JitModuleCtx` as its 2nd parameter
// (after `*mut JitFrame`). The `JitModuleCtx::vm_ctx: *mut VmContext` field
// (set by `JitModule::run` for the duration of one entry call) is the only
// runtime-mutable VM state — the previous `PENDING_EXCEPTION` and
// `STATIC_FIELDS` thread_local slots have been removed.

/// Borrow the VmContext from a JitModuleCtx pointer for the duration of the
/// helper call.
///
/// SAFETY: caller must ensure
///   1. `jit_ctx` is non-null and points to a valid JitModuleCtx
///   2. `(*jit_ctx).vm_ctx` is non-null (always true while inside
///      `JitModule::run` / `run_fn`)
///   3. The returned reference's lifetime does not outlive the helper call
pub(super) unsafe fn vm_ctx_ref<'a>(jit_ctx: *const JitModuleCtx) -> &'a VmContext {
    &*((*jit_ctx).vm_ctx)
}

pub(super) fn set_exception(ctx: &VmContext, v: Value) {
    ctx.set_exception(v);
}

pub(super) fn take_exception(ctx: &VmContext) -> Option<Value> {
    ctx.take_exception()
}

pub fn take_exception_error(ctx: &VmContext, module: &crate::metadata::Module) -> anyhow::Error {
    let msg = take_exception(ctx)
        .as_ref()
        .map(|v| crate::exception::format_uncaught(v, module))
        .unwrap_or_else(|| "uncaught exception".to_owned());
    anyhow::anyhow!("{}", msg)
}

// ─── JIT function type alias ────────────────────────────────────────────────

pub type JitFn = unsafe extern "C" fn(frame: *mut JitFrame, ctx: *const JitModuleCtx) -> u8;

// ─── Shared numeric helpers ─────────────────────────────────────────────────
//
// converge-vm-arith-semantics (H3): the scalar rules formerly duplicated here
// (`int_binop_helper` / `int_bitop_helper` / `numeric_lt_helper` were verbatim
// copies of `interp/ops.rs`) now live once in `crate::semantics`. Call sites in
// `arith.rs` use `crate::semantics::{int_binop, int_bitop, numeric_lt}` directly.
