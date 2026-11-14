//! VM state composition, pinned attachment and balanced lifetime management.
//! Domain behavior belongs to exec, loader, metadata, threads and diagnostics.
//!
//! The retired aggregate namespace has no compatibility export:
//! ```compile_fail
//! use z42::context::VmContext;
//! ```
use std::marker::PhantomPinned;
use std::sync::{Arc, Weak};
use parking_lot::Mutex;
use crate::gc::Heap;
use crate::threads::VmContextPtr;

#[path = "vm_extensions.rs"]
mod extensions;
pub use extensions::{ExtensionState, VmExtensions};


/// State shared by all contexts of one VM, owned through `Arc<VmCore>`.
/// Subsystems own their synchronization and teardown; contexts retain thread-local
/// frames, pending exceptions and allocation arenas. Concrete core-library and
/// interop resources live in frozen extension state.
pub struct VmCore {
    /// Instances stop before heap/engine teardown; their library leases outlive callbacks.
    pub(crate) components: std::sync::OnceLock<crate::components::api::VmComponents>,
    pub(crate) threads: crate::threads::ThreadState,
    /// Shared execution state, including engine registrations and backend code lifetime.
    pub(crate) exec: crate::exec::ExecState,
    pub(crate) object_protocol: crate::object::ObjectProtocol<VmContext, crate::exec::call::builtin_abi::BuiltinError>,
    /// Owned heap. Its root providers keep Weak<VmCore> to avoid ownership cycles.
    /// Metadata and interop providers remain alive until after heap teardown.
    pub(crate) heap:               Box<Heap>,
    pub(crate) runtime: crate::metadata::RuntimeState,
    pub(crate) loader: crate::loader::LoaderState,

    /// Observation state shared by this VM; per-thread counters stay on each context.
    pub(crate) diagnostics: crate::diagnostics::DiagnosticsState,

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
    #[inline] pub(crate) fn object_protocol(&self) -> &crate::object::ObjectProtocol<VmContext, crate::exec::call::builtin_abi::BuiltinError> { &self.object_protocol }
    #[inline] pub(crate) fn components(&self) -> &std::sync::OnceLock<crate::components::api::VmComponents> { &self.components }
    #[inline] pub fn exec(&self) -> &crate::exec::ExecState { &self.exec }
    #[inline] pub fn heap(&self) -> &Heap { &self.heap }
    #[inline] pub fn runtime(&self) -> &crate::metadata::RuntimeState { &self.runtime }
    #[inline] pub fn diagnostics(&self) -> &crate::diagnostics::DiagnosticsState { &self.diagnostics }
    #[inline] pub fn loader(&self) -> &crate::loader::LoaderState { &self.loader }
    #[inline] pub fn threads(&self) -> &crate::threads::ThreadState { &self.threads }
    #[inline] pub fn ext_state(&self) -> &crate::vm::ExtensionState { &self.ext_state }
}

/// Process-wide registry of all live [`VmCore`] instances.
///
/// **add-os-signal-handler **: the POSIX signal handler in
/// [`crate::host::signal`] (Phase 2 of D4 panic-hook story) walks this
/// list to capture z42 call stacks across all VMs in the process. Stored
/// as `Weak` so a VmCore drop doesn't require explicit deregistration;
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
pub struct RuntimeInputs {
    pub extensions: crate::vm::ExtensionState,
    pub config: crate::config::VmConfig,
    pub object_protocol: crate::object::ObjectProtocol<VmContext, crate::exec::call::builtin_abi::BuiltinError>,
    pub engines: crate::exec::EngineRegistry,
    pub builtins: &'static [crate::exec::call::builtin_abi::BuiltinDesc],
    pub catalog: &'static crate::metadata::builtins::BuiltinCatalog,
    pub fusion: crate::loader::fusion_inputs::FusionInputSource,
    pub sampler: crate::diagnostics::sampling::Sampling,
}
impl VmContext {
    pub fn with_runtime(
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
            threads: crate::threads::ThreadState::new(config.safepoint_throttle),
            loader: crate::loader::LoaderState::new(vfs, fusion),
            heap:                 Box::new(Heap::new(config.gc)),
            runtime: crate::metadata::RuntimeState::new(module, catalog),

            diagnostics: crate::diagnostics::DiagnosticsState {
                counters: Arc::new(crate::diagnostics::RuntimeCounters::new()),
                runtime_observers: crate::diagnostics::RuntimeObserverRegistry::new(),
                park_histogram: Mutex::new(crate::gc::types::PauseHistogram::new(config.gc.pause_window)),
                lock_contentions: std::sync::atomic::AtomicU64::new(0),
                lock_wait_us:     std::sync::atomic::AtomicU64::new(0),
                // Construct the backend here; the VM owns only its sampling capability.
                sampler,
            },
            exec: crate::exec::ExecState::new(
                engines, builtins, config.stack_alloc,
            ),
            object_protocol,
            ext_state,
        });

        {
            let weak = Arc::downgrade(&core);
            core.runtime.types.set_update_observer(Arc::new(move |id| {
                if let Some(core) = weak.upgrade() {
                    core.runtime.reflection_types.invalidate_attributes(&core.heap, id);
                }
            }));
        }

        // add-os-signal-handler : register this Arc<VmCore> into
        // the process-wide VM_CORES registry so the POSIX signal handler
        // can walk it to capture z42 call stacks on hard crash. Lazy-prune
        // dead Weak entries at the same time (cheap O(n) sweep amortized
        // across VmCore creations).
        if let Ok(mut g) = VM_CORES.lock() {
            g.retain(|w| w.strong_count() > 0);
            g.push(Arc::downgrade(&core));
        }

        // add-gc-safepoint-auto-threshold : wire the
        // needs_auto_collect flag into the heap so its pressure-trip
        // path defers to the next safepoint check instead of collecting
        // inline (the inline path has no &VmContext and would race with
        // concurrent mutators' frame.regs writes).
        core.heap.set_external_needs_collect_flag(Arc::clone(&core.threads.needs_auto_collect));
        // P0-16: and let it send every mutator to the slow path when it raises that flag, so the
        // collection does not wait out the safepoint throttle. `Weak`, like the root scanner.
        {
            let core_weak = Arc::downgrade(&core);
            core.heap.set_safepoint_poke(Arc::new(move || {
                if let Some(c) = core_weak.upgrade() {
                    crate::threads::safepoint::poke_safepoints(&c);
                }
            }));
        }

        // add-gc-runtime-knobs : apply the process-wide GC knobs.
        //
        // `Z42_GC_MAX_BYTES` is what *arms* automatic collection at all —
        // `maybe_auto_collect` returns immediately while `max_bytes` is `None`,
        // and the three `Z42_GC_*_RATIO` knobs are fractions of this budget, so
        // without it they are inert. Unset keeps the historical behaviour
        // (collect only on an explicit `Std.GC.Collect()`).
        {
            if let Some(max) = config.gc_max_bytes {
                core.heap.set_max_heap_bytes(Some(max));
            }
            if config.gc_trace {
                core.heap.add_observer(Arc::new(crate::gc::trace::GcTracer::default()));
            }
            // add-loh-bytes-knob : process-global, not per-heap — `class_for` is
            // called from the lock-free TLAB fast path, which has no heap reference. Applied
            // here (rather than read in `class_for`) so the allocation path keeps paying only
            // one relaxed load of a static rather than a `runtime_config()` lookup.
            if let Some(n) = config.process_loh_bytes {
                crate::gc::var_region::set_loh_bytes(n as usize);
            }
        }

        core.heap.set_external_root_scanner(Box::new(crate::threads::roots::CoreRoots(Arc::downgrade(&core))));

        // add-lazy-context-unload: wire the collectible-context reclaimer so a
        // major GC reclaims `Unloading` AssemblyLoadContexts with no live refs.
        // Captures `Weak<VmCore>` (cycle avoidance) + the unloading-count flag.
        {
            let unloading = core.loader.context_registry.lock().unloading_flag();
            core.heap.set_context_reclaimer(Box::new(crate::loader::context_gc::CoreContextReclaimer {
                core: Arc::downgrade(&core),
                unloading,
            }));
        }

        // Both GC and diagnostic walks enumerate the same VM roots. The heap
        // invokes these borrowed-root callbacks only while mutators are paused.
        {
            let weak = Arc::downgrade(&core);
            core.heap.set_categorized_root_scanner(Box::new(move |visit| {
                if let Some(core) = weak.upgrade() {
                    unsafe { crate::threads::roots::scan_categorized_roots(&core, visit); }
                }
            }));
        }

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
    /// The new VmContext registers itself in `core.threads.vm_contexts` for GC root
    /// scanning, so the worker's per-thread roots (`pending_exception` /
    /// `call_stack`) are visible to the cycle collector.
    /// On drop, the entry is removed under the same Mutex discipline as
    /// the primary path.
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
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        boxed.core.threads.register_context(ptr);
        // add-gc-tlab (stage 2): arm this thread for TLAB allocation (balanced
        // in Drop). A spawned worker allocates into the TLAB fast path.
        crate::gc::tlab::arm();
        // add-incremental-major-gc M2a: this thread's SATB records belong to this heap (balanced in Drop).
        crate::gc::satb::bind_thread(boxed.core.heap.heap_epoch());
        boxed
    }


}

impl Drop for VmContext {
    /// **add-vmcontext-registry **: deregister this `VmContext`
    /// from `VmCore.threads.vm_contexts` so the GC scanner stops trying to walk a
    /// soon-to-be-freed allocation. Runs BEFORE the underlying `Box` storage
    /// is released (Rust drop order: contents → Box dealloc), so any GC
    /// scan racing this Drop will block on the registry lock and see the
    /// post-removed list.
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
        let ptr = self as *const Self;
        self.core.threads.deregister_context(
            ptr, &self.core.diagnostics.counters, &self.diagnostics.counters,
        );
    }
}

impl VmContext {
    #[inline]
    pub fn loader(&self) -> crate::loader::RuntimeLoader<'_> {
        crate::loader::RuntimeLoader::new(
            &self.core.loader, &self.core.runtime, &self.runtime,
            &self.core.diagnostics.runtime_observers,
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
}

#[cfg(all(test, z42_test_facade))]
#[path = "vm_tests.rs"]
mod tests;
