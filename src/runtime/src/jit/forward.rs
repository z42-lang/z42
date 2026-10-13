//! The JIT side of `[Native]` stub folding (runtime-audit P1-8; the shapes, the
//! rationale and the shared pieces live in [`crate::interp::forward`]).
//!
//! A compiled entry whose function is a pure forwarder records the builtin's id
//! ([`FnEntry::forward`](super::frame::FnEntry)); `jit_call` (after its barriers) and
//! `jit_vcall`'s `invoke_entry` then run the builtin straight from the caller's
//! registers ([`call_forward`]). Cold callees (no compiled entry yet) fold the same way
//! through the function's own cache (`interp::forward::forward_of`). The translator
//! also reads the forward of a call target to inline string intrinsics.

use crate::metadata::bytecode::Function;
use crate::metadata::Value;

use super::frame::JitFrame;
use super::runtime::JitRuntime;
use crate::vm_context::VmContext;
use super::helpers::vm_ctx_ref;

pub(crate) use crate::interp::forward::{builtin_forward, chained_forward};

/// Run the folded call of `wrapper` (fold `token`, `interp::forward::forward_of`) with `this` (vcall receiver, if any)
/// followed by the caller's `arg_regs`, storing the result into `regs[dst]` (`Null`
/// for a void builtin — what the wrapper's `ret` would have produced). Returns the JIT
/// helper status: 0 = ok, 1 = exception pending.
///
/// # Safety
/// `frame` / `ctx` as for every JIT helper; `arg_regs` index `frame.regs`; `wrapper`
/// is the forwarder (valid for the run, as every `FnEntry::func`).
pub(crate) unsafe fn call_forward(
    frame: &mut JitFrame, ctx: *const VmContext, dst: u32, wrapper: *const Function, token: u32,
    this: Option<Value>, arg_regs: &[u32],
) -> u8 {
    let vm = unsafe { vm_ctx_ref(ctx) };
    let wrapper = unsafe { &*wrapper };
    let module = || unsafe { super::helpers::jit_module(ctx) };
    match crate::interp::forward::run_folded(vm, wrapper, token, this, &frame.regs, arg_regs, module) {
        Ok(Some(v)) => { frame.regs[dst as usize] = v; 0 }
        Ok(None) => { frame.regs[dst as usize] = Value::Null; 0 }
        Err(f) => super::helpers::call::raised_into_exception(vm, f.raise(vm, module(), wrapper)),
    }
}

// ── Translate-time view (perf-str-jit-intrinsics) ────────────────────────────

/// A string builtin the JIT emits inline at a call site instead of dispatching
/// (`translate::str_intrinsics`). Recognised by the builtin the call target
/// forwards to — the same identification as [`builtin_forward`] /
/// [`chained_forward`] — never by the callee's name alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrIntrinsic {
    /// `__str_length` — `string.Length` (receiver only).
    Length,
    /// `__str_char_at` — `string.CharAt(i)` and the indexer `s[i]` (receiver + index).
    CharAt,
}

impl StrIntrinsic {
    /// The intrinsic builtin `id` implements, if any.
    pub(crate) fn of_builtin(id: u32) -> Option<Self> {
        let is = |name| crate::corelib::builtin_id_of(name).is_some_and(|b| b.0 == id);
        if is("__str_length") { Some(Self::Length) }
        else if is("__str_char_at") { Some(Self::CharAt) }
        else { None }
    }

    /// Argument count of the forwarded builtin, receiver included.
    pub(crate) fn argc(self) -> usize {
        match self { Self::Length => 1, Self::CharAt => 2 }
    }
}

/// What the translator may ask about the VM while compiling a function: call targets,
/// and the heap's write-barrier gate. Implemented by [`CompileLookup`]; never loads a
/// package or binds a site.
pub(crate) trait ForwardLookup {
    /// The heap's barrier-gate bytes (`MagrGC::jit_barrier_bytes`) that compiled reference
    /// field stores read; `None` keeps every reference store on the helper.
    fn barrier_bytes(&self) -> Option<(*const std::sync::atomic::AtomicU8, *const std::sync::atomic::AtomicU8)> {
        None
    }
    /// The builtin the function under JIT id `id` forwards to (shape 1 or 2),
    /// provided a static call of it needs no type-init barrier (its owner type is
    /// loaded and has no static constructor).
    fn forward_of_id(&self, id: u32) -> Option<u32>;
    /// The same for the function registered under key `name`, looked up the way
    /// `resolve_vcall`'s candidate walk does (merged module, then loaded packages) —
    /// but without loading anything.
    fn forward_of_name(&self, name: &str) -> Option<u32>;
}

/// The translator's view of the VM while one function compiles: the shared JIT
/// runtime plus the compiling thread's context (for read-only probes of loaded
/// packages and types — VM-wide facts, so the compiled code is valid on every
/// thread).
pub(crate) struct CompileLookup<'a> {
    pub(crate) rt: &'a JitRuntime,
    pub(crate) vm: &'a VmContext,
}

impl CompileLookup<'_> {
    /// Builtin id `func` forwards to (shape 1, or shape 2 over an already-registered
    /// target), gated on its static-call barrier being a no-op.
    fn barrier_free_forward(&self, func: &Function) -> Option<u32> {
        if !self.vm.callee_owner_init_free(&func.name) {
            return None;
        }
        builtin_forward(func).or_else(|| chained_forward(func, |name| {
            self.probe_fn(name, |f| builtin_forward(f))
        }))
    }

    /// Apply `f` to the function registered under `name` — merged module first, then
    /// the packages loaded so far (the order `resolve_vcall` probes). Read-only: the
    /// translator runs under the compile lock and must not load or initialise anything.
    fn probe_fn<R>(&self, name: &str, f: impl FnOnce(&Function) -> Option<R>) -> Option<R> {
        let module = self.rt.module();
        if let Some(func) = module.func_index.get(name).and_then(|&i| module.functions.get(i)) {
            return f(func);
        }
        let func = self.vm.probe_loaded_function(name)?;
        f(&func)
    }
}

impl ForwardLookup for CompileLookup<'_> {
    fn forward_of_id(&self, id: u32) -> Option<u32> {
        self.barrier_free_forward(self.rt.fn_of(id as usize)?)
    }

    fn forward_of_name(&self, name: &str) -> Option<u32> {
        self.probe_fn(name, |f| self.barrier_free_forward(f))
    }

    /// The VM's heap outlives its JIT (both belong to the core), so the addresses stay valid
    /// for as long as the compiled code can run.
    fn barrier_bytes(&self) -> Option<(*const std::sync::atomic::AtomicU8, *const std::sync::atomic::AtomicU8)> {
        self.vm.heap().jit_barrier_bytes()
    }
}

#[cfg(test)]
#[path = "forward_tests.rs"]
mod forward_tests;
