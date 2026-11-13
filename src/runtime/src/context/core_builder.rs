//! Primary context construction from explicit, already-resolved facade inputs.
use super::*;
use parking_lot::Mutex;
/// Mechanism inputs resolved by the host; no concrete backend or configuration parser.
pub struct RuntimeInputs {
    pub extensions: crate::context::extensions::ExtensionState,
    pub config: crate::config::VmConfig,
    pub object_protocol: crate::exec::object_protocol::ObjectProtocol,
    pub engines: crate::exec::engine::EngineRegistry,
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
                counters: Arc::new(crate::diagnostics::counters::RuntimeCounters::new()),
                runtime_observers: crate::diagnostics::observer::RuntimeObserverRegistry::new(),
                park_histogram: Mutex::new(crate::gc::types::PauseHistogram::new(config.gc.pause_window)),
                lock_contentions: std::sync::atomic::AtomicU64::new(0),
                lock_wait_us:     std::sync::atomic::AtomicU64::new(0),
                // Construct the backend here; the VM owns only its sampling capability.
                sampler,
            },
            exec: crate::exec::ExecState::new(
                engines, builtins, object_protocol, config.stack_alloc,
            ),
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
