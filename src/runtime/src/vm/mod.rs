//! VM state composition, pinned attachment and balanced lifetime management.
//! Domain behavior belongs to exec, loader, metadata, threads and diagnostics.
//!
//! The retired aggregate namespace has no compatibility export:
//! ```compile_fail
//! use z42::context::VmContext;
//! ```
//! VM consumers use the module contract independently of its implementation layout:
//! ```
//! use z42::vm::{VmCore, VmContext, VmExtensions};
//! let _: Option<std::sync::Arc<VmCore>> = None;
//! let _: Option<std::pin::Pin<Box<VmContext>>> = None;
//! let _ = VmExtensions::new();
//! ```
use std::marker::PhantomPinned;
use std::sync::{Arc, Weak};
use crate::gc::Heap;

mod extensions;
mod wiring;
pub(crate) mod context_gc;
pub use extensions::{ExtensionState, VmExtensions};


pub(crate) mod app_state;
pub(crate) mod assembly;
pub(crate) mod defaults;
pub(crate) mod program;
#[cfg(test)]
mod program_tests;

/// State shared by all contexts of one VM, owned through `Arc<VmCore>`.
/// Subsystems own their synchronization and teardown; contexts retain thread-local
/// frames, pending exceptions and allocation arenas. Concrete core-library and
/// interop resources live in frozen extension state.
pub struct VmCore {
    /// Instances stop before heap/engine teardown; their library leases outlive callbacks.
    pub(crate) components: std::sync::OnceLock<crate::components::api::VmComponents>,
    pub(crate) threads: crate::threads::ThreadState,
    /// Captured at construction from `VmConfig` (worker-thread stack size;
    /// runtime-audit P0-4). Spawns read this snapshot, never process config.
    pub(crate) vm_thread_stack_size: usize,
    /// Shared execution state, including engine registrations and backend code lifetime.
    pub(crate) exec: crate::exec::ExecState,
    pub(crate) object_protocol: crate::object::ObjectProtocol<VmContext, crate::exec::builtin_abi::BuiltinError>,
    /// Owned heap. Its root providers keep Weak<VmCore> to avoid ownership cycles.
    /// Metadata and interop providers remain alive until after heap teardown.
    pub(crate) heap:               Box<Heap>,
    pub(crate) runtime: crate::metadata::RuntimeState,
    pub(crate) loader: crate::loader::LoaderState,

    /// Observation state shared by this VM; per-thread counters stay on each context.
    pub(crate) diagnostics: crate::diagnostics::DiagnosticsState,
    // The backend in diagnostics must drop before its owning profiler instance.
    profiling: crate::components::api::ProfilingState,

    /// Released after the heap and engines so extension-owned resources outlive them.
    pub(crate) ext_state: crate::vm::ExtensionState,
}

/// One pinned per-thread view of a VM. Subsystems retain their own synchronization;
/// the aggregate adds no lock. Core remains shared while all other state is private.
pub struct VmContext {
    pub(crate) core: Arc<VmCore>,
    pub(crate) exec: crate::exec::ThreadExecState,
    pub(crate) diagnostics: crate::diagnostics::ThreadDiagnosticsState,
    pub(crate) runtime: crate::metadata::ThreadRuntimeState,
    pub(crate) threads: crate::threads::ThreadLocalState,
    /// Keeps the registered context and nested JIT fields at stable addresses.
    pub(crate) _pin: PhantomPinned,
}

/// Actual nested offset of the per-thread safepoint throttle in the context.
/// Generated code must use this constant instead of assuming Rust field order.
pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET: usize =
    std::mem::offset_of!(VmContext, threads.safepoint_skip);

/// Stack bound read directly by every compiled prologue.
pub const VM_CONTEXT_STACK_LIMIT_OFFSET: usize =
    std::mem::offset_of!(VmContext, exec.jit_head.stack_limit);

/// Registry pointer read by inline string intrinsics' package-init gate.
pub const VM_CONTEXT_CCTORS_OFFSET: usize =
    std::mem::offset_of!(VmContext, exec.jit_head.cctors);

impl VmContext {
    #[inline] pub fn core(&self) -> &Arc<VmCore> { &self.core }
    #[inline] pub fn exec(&self) -> &crate::exec::ThreadExecState { &self.exec }
    #[inline] pub fn diagnostics(&self) -> &crate::diagnostics::ThreadDiagnosticsState { &self.diagnostics }
    #[inline] pub fn runtime(&self) -> &crate::metadata::ThreadRuntimeState { &self.runtime }
    #[inline] pub fn threads(&self) -> &crate::threads::ThreadLocalState { &self.threads }
}
impl VmCore {
    #[inline] pub(crate) fn object_protocol(&self) -> &crate::object::ObjectProtocol<VmContext, crate::exec::builtin_abi::BuiltinError> { &self.object_protocol }
    #[inline] pub(crate) fn profiling(&self) -> &crate::components::api::ProfilingState { &self.profiling }
    #[inline] pub(crate) fn components(&self) -> &std::sync::OnceLock<crate::components::api::VmComponents> { &self.components }
    #[inline] pub(crate) fn exec(&self) -> &crate::exec::ExecState { &self.exec }
    #[inline] pub fn heap(&self) -> &Heap { &self.heap }
    #[inline] pub fn runtime(&self) -> &crate::metadata::RuntimeState { &self.runtime }
    #[inline] pub fn diagnostics(&self) -> &crate::diagnostics::DiagnosticsState { &self.diagnostics }
    #[inline] pub fn loader(&self) -> &crate::loader::LoaderState { &self.loader }
    #[inline] pub fn threads(&self) -> &crate::threads::ThreadState { &self.threads }
    #[inline] pub fn ext_state(&self) -> &crate::vm::ExtensionState { &self.ext_state }
}

/// Process-wide registry of all live [`VmCore`] instances.
///
/// The crash reporter walks this list to capture z42 call stacks across all
/// VMs in the process. Weak references avoid explicit deregistration;
/// the handler ignores entries whose `upgrade()` returns `None` and the
/// next `VmCore::new` call lazy-prunes dead entries via `retain`.
///
/// Use the public [`vm_cores_snapshot`] to grab a Vec of live cores from
/// non-signal contexts (tests, debug tools); the signal handler uses
/// `try_lock` directly to avoid deadlock on contended mutator threads.
pub static VM_CORES: std::sync::Mutex<Vec<Weak<VmCore>>> =
    std::sync::Mutex::new(Vec::new());

/// Snapshot all live `Arc<VmCore>` instances. Convenience for tests +
/// diagnostic tools. The signal handler does NOT use this (uses
/// `VM_CORES.try_lock()` directly to avoid blocking on contended cores).
pub fn vm_cores_snapshot() -> Vec<Arc<VmCore>> {
    VM_CORES
        .lock()
        .ok()
        .map(|g| g.iter().filter_map(|w| w.upgrade()).collect())
        .unwrap_or_default()
}

impl VmContext {
    // ── GC heap ───────────────────────────────────────────────────────────

    /// Borrow the GC heap as a trait object. All script-driven allocations go
    /// through this entry point; see `docs/internals/src/runtime/gc.md`.
    pub fn heap(&self) -> &Heap {
        self.core.heap.as_ref()
    }


    /// Public accessor for the shared compiled Module installed by
    /// [`with_module`](Self::with_module). Returns `None` if VmContext was
    /// built via [`new`](Self::new) (test path).
    pub fn module(&self) -> Option<&Arc<crate::metadata::Module>> {
        self.core.runtime.module.as_ref()
    }

    /// The VM's function table (per-VM `FnId`s; see `metadata::ids::func_table`).
    #[inline]
    pub fn funcs(&self) -> &crate::metadata::ids::func_table::FuncTable {
        &self.core.runtime.funcs
    }

    /// The VM's type table (process-global `TypeId`s; see `metadata::ids::type_table`).
    #[inline]
    pub fn types(&self) -> &crate::metadata::ids::type_table::TypeTable {
        &self.core.runtime.types
    }

    /// Clone the shared `Arc<VmCore>` — needed by external integration
    /// tests / embedders that spawn raw OS threads and want to construct
    /// a child VmContext via [`new_with_core`](Self::new_with_core). The
    /// `core` field itself is `pub`; this is the public escape hatch.
    pub fn core_arc(&self) -> Arc<VmCore> {
        Arc::clone(&self.core)
    }

    /// Borrow this VM's extension state by its exact Rust type, without a new lock.
    /// Returns `None` when the type was not registered before VM creation.
    pub fn ext<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.core.ext_state.get::<T>()
    }

    /// The file system this VM reads and writes through.
    pub fn vfs(&self) -> &Arc<dyn crate::pal::fs::Vfs> {
        &self.core.loader.vfs
    }

}
/// Mechanism inputs resolved by the host; no concrete backend or configuration parser.
struct RuntimeInputs {
    extensions: crate::vm::ExtensionState,
    config: crate::config::VmConfig,
    object_protocol: crate::object::ObjectProtocol<VmContext, crate::exec::builtin_abi::BuiltinError>,
    engines: crate::exec::EngineRegistry,
    builtins: &'static [crate::exec::builtin_abi::BuiltinDesc],
    catalog: &'static crate::metadata::builtins::BuiltinCatalog,
    fusion: crate::loader::fusion_inputs::FusionInputSource,
    sampler: crate::diagnostics::Sampling,
}
impl VmContext {
    fn with_runtime(
        module: Option<Arc<crate::metadata::Module>>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
        inputs: RuntimeInputs,
    ) -> std::pin::Pin<Box<Self>> {
        let RuntimeInputs { extensions: ext_state, config, object_protocol, engines,
            builtins, catalog, fusion, sampler } = inputs;
        // Construct VmCore with heap embedded; scanner is installed AFTER
        // wrapping in Arc so the closure can capture Weak<VmCore> (cycle
        // avoidance: heap owns the scanner, scanner refs core, core owns
        // heap → strong-Arc loop = leak). Weak.upgrade() per call.
        let core: Arc<VmCore> = Arc::new(VmCore {
            components: std::sync::OnceLock::new(),
            profiling: Default::default(),
            threads: crate::threads::ThreadState::new(config.safepoint_throttle),
            vm_thread_stack_size: config.vm_thread_stack_size,
            loader: crate::loader::LoaderState::new(vfs, fusion),
            heap:                 Box::new(Heap::new(config.gc)),
            runtime: crate::metadata::RuntimeState::new(module, catalog),

            diagnostics: crate::diagnostics::DiagnosticsState::new(config.gc.pause_window, sampler),
            exec: crate::exec::ExecState::new(
                engines, builtins, config.stack_alloc,
            ),
            object_protocol,
            ext_state,
        });

        wiring::wire(&core, &config);

        Self::new_with_core(core)
    }
}
impl VmContext {
    /// Spawned-thread entry: build a VmContext that **shares** an existing
    /// `Arc<VmCore>` instead of creating a new one. Used by
    /// `__thread_spawn`'s spawned closure so the worker thread sees the
    /// same module / static_fields / heap / lazy_loader / extension state
    /// state as the parent thread.
    ///
    /// Register each worker with the Core so GC can find its thread-local
    /// roots. Drop removes the registration before deallocation.
    pub fn new_with_core(core: Arc<VmCore>) -> std::pin::Pin<Box<Self>> {
        let safepoint_throttle = core.threads.safepoint_throttle;
        let ctx = Self {
            exec: crate::exec::ThreadExecState::new(&core.runtime.cctors, &core.exec.engines),
            core,
            diagnostics: Default::default(),
            runtime: crate::metadata::ThreadRuntimeState::new(),
            threads: crate::threads::ThreadLocalState::new(safepoint_throttle),
            _pin: PhantomPinned,
        };
        let boxed = Box::pin(ctx);
        // SAFETY: this fresh context belongs to its Core and stays pinned until
        // its Drop removes the registration.
        unsafe { boxed.core.threads.register_context(boxed.as_ref()) };
        // add-gc-tlab (stage 2): arm this thread for TLAB allocation (balanced
        // in Drop). A spawned worker allocates into the TLAB fast path.
        crate::gc::tlab::arm();
        // add-incremental-major-gc M2a: this thread's SATB records belong to this heap (balanced in Drop).
        crate::gc::satb::bind_thread(boxed.core.heap.heap_epoch());
        boxed
    }


}

impl Drop for VmContext {
    /// Retire mutator resources and remove the registration before freeing
    /// the pinned allocation.
    fn drop(&mut self) {
        // add-gc-tlab (stage 2): retire this thread's TLAB before the context
        // goes away — merge its borrowed chunks' filled objects back into the
        // shared region so they stay GC-visible (a thread may have handed
        // objects to other threads). Runs on the owning thread; leaves the
        // thread-local TLAB unbound for the next context. No-op if unbound.
        self.core.heap.retire_thread_tlab();
        self.core.heap.retire_parked_tlab(&self.threads.parked_tlab);
        // add-gc-tlab (stage 2): balance the arm() from construction.
        crate::gc::tlab::disarm();
        // add-incremental-major-gc M2a: the retire above handed this thread's SATB buffer over.
        crate::gc::satb::unbind_thread();
        self.core.threads.deregister_context(self, &self.core.diagnostics.counters);
    }
}

impl VmContext {
    #[inline]
    pub fn loader(&self) -> crate::loader::RuntimeLoader<'_> {
        crate::loader::RuntimeLoader::new(
            &self.core.loader, &self.core.runtime, &self.runtime,
            self.core.diagnostics.module_loads(),
        )
    }
    #[inline]
    pub fn statics(&self) -> crate::metadata::statics::StaticFields<'_> {
        crate::metadata::statics::StaticFields::new(&self.core.runtime.statics, &self.core.runtime.cctors)
    }
    #[inline]
    pub fn initialization(&self) -> crate::exec::Initialization<'_> {
        crate::exec::Initialization::new(self)
    }
    #[inline]
    pub fn counters(&self) -> &crate::diagnostics::RuntimeCounters { &self.diagnostics.counters }

    /// The minimal resolution view handed to `metadata::resolve` — the
    /// composition layer owns the adaptation, resolution never sees the VM.
    #[inline]
    pub(crate) fn resolve_services(&self) -> VmResolveServices<'_> {
        VmResolveServices {
            funcs: self.funcs(),
            builtins: &self.core.runtime.builtins,
            static_table: &self.core.runtime.statics,
            cctors: &self.core.runtime.cctors,
            loader: self.loader(),
        }
    }
}

/// The borrowed tables and loader used during token resolution.
/// Pending owners are enqueued and drained before resolved tokens are published.
pub(crate) struct VmResolveServices<'a> {
    funcs: &'a crate::metadata::ids::func_table::FuncTable,
    builtins: &'a crate::metadata::builtins::BuiltinCatalog,
    static_table: &'a crate::metadata::statics::StaticTable,
    cctors: &'a crate::metadata::statics::CctorRegistry,
    loader: crate::loader::RuntimeLoader<'a>,
}

impl crate::metadata::resolve::ResolveServices for VmResolveServices<'_> {
    fn funcs(&self) -> &crate::metadata::ids::func_table::FuncTable { self.funcs }
    fn builtins(&self) -> &crate::metadata::builtins::BuiltinCatalog { self.builtins }
    fn statics(&self) -> crate::metadata::statics::StaticFields<'_> {
        crate::metadata::statics::StaticFields::new(self.static_table, self.cctors)
    }
    fn has_loaded_type(&self, class_fq: &str) -> bool { self.loader.has_loaded_type(class_fq) }
    fn settle_type_loads(&self, owners: &[String]) {
        for owner in owners {
            self.loader.enqueue_type_load(owner);
        }
        self.loader.drain_pending_type_loads();
    }
}


#[cfg(all(test, z42_test_facade))]
mod tests;

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub use defaults::jit_test_context;
