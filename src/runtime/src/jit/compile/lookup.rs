//! Read-only linkage facts used while compiling a function.
//! Queries neither load packages nor execute type initializers.
use crate::vm::runtime::{Function, statics::StaticCell};
use crate::vm_context::VmContext;
use super::state::JitRuntime;
use crate::vm::exec::call::forward::{builtin_forward, chained_forward};

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
    pub(crate) fn of_builtin(id: u32, builtins: &crate::vm::runtime::builtins::BuiltinCatalog) -> Option<Self> {
        let is = |name| builtins.id_of(name).is_some_and(|b| b.0 == id);
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
/// Stable VM linkage addresses used by the caller's native field-reader guard.
pub(crate) struct DirectFieldReader {
    pub entry: *const crate::vm::runtime::dispatch::ic::FieldICEntry,
    pub ready: *const std::sync::atomic::AtomicU32,
    pub epoch: *const std::sync::atomic::AtomicU32,
    pub ambiguous: *const std::sync::atomic::AtomicBool,
}

pub(crate) trait ForwardLookup {
    fn direct_field_reader(&self, _id: u32) -> Option<DirectFieldReader> {
        None
    }

    fn builtins(&self) -> &crate::vm::runtime::builtins::BuiltinCatalog;
    /// The heap's barrier-gate bytes (`Heap::jit_barrier_bytes`) that compiled reference
    /// field stores read; `None` keeps every reference store on the helper.
    fn barrier_bytes(&self) -> Option<(*const std::sync::atomic::AtomicU8, *const std::sync::atomic::AtomicU8)> {
        None
    }
    /// The cell of static field `id` and the cctor registry's barrier epoch
    /// (`VmContext::jit_static_cell`) that compiled `StaticGet` / `StaticSet` read; `None` keeps the
    /// access on the helper.
    fn static_cell(&self, _id: u32) -> Option<(*const StaticCell, *const std::sync::atomic::AtomicU32)> {
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
        builtin_forward(func, self.vm.core.runtime.builtins).or_else(|| chained_forward(func, |name| {
            self.probe_fn(name, |f| builtin_forward(f, self.vm.core.runtime.builtins))
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
    fn direct_field_reader(&self, id: u32) -> Option<DirectFieldReader> {
        let func = self.rt.fn_of(id as usize)?;
        let entry = crate::vm::exec::call::forward::direct_reader_site(func)?;
        Some(DirectFieldReader {
            entry,
            ready: &func.call_init_ready,
            epoch: self.vm.core.runtime.cctors.barrier_epoch_ptr(),
            ambiguous: crate::vm::loader::lazy::ambiguity_seen_ptr(),
        })
    }

    fn builtins(&self) -> &crate::vm::runtime::builtins::BuiltinCatalog { self.vm.core.runtime.builtins }
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

    /// The static table and the cctor registry belong to the core too (cells never move).
    fn static_cell(&self, id: u32) -> Option<(*const StaticCell, *const std::sync::atomic::AtomicU32)> {
        self.vm.jit_static_cell(id)
    }
}
