//! Facade-owned primary VM construction; thread mechanisms remain in vm::context.
use std::marker::PhantomPinned;
use std::sync::Arc;
use parking_lot::Mutex;
use crate::gc::Heap;
use crate::object::Value;
use crate::vm::context::{VmContext, VmCore, VmContextPtr, VM_CORES};

impl VmContext {
    /// Standard test entry: constructs a VmContext with `VmCore.module = None`.
    /// Cargo unit tests use this — they don't need a real Module for
    /// builtin / static-field / alloc tests. Production paths use
    /// [`with_module`](Self::with_module).
    pub fn new() -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, crate::pal::fs::default_vfs(), Default::default())
    }

    /// [`new`](Self::new) on the file system `vfs` instead of the default one.
    pub fn with_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, vfs, Default::default())
    }

    /// Production entry: constructs a VmContext with the user's compiled
    /// Module wrapped in `Arc` for cross-thread sharing. Required by any
    /// path that may invoke `__thread_spawn` (which dispatches into the
    /// shared module from the spawned thread).
    pub fn with_module(module: crate::vm::runtime::Module) -> std::pin::Pin<Box<Self>> {
        Self::with_module_and_vfs(module, crate::pal::fs::default_vfs())
    }

    /// [`with_module`](Self::with_module) on the file system `vfs`: every path-based file
    /// builtin and every package load of this VM goes through it.
    pub fn with_module_and_vfs(
        module: crate::vm::runtime::Module,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(Some(module), vfs, Default::default())
    }

    /// Create a VM with owned extension registrations, frozen for its entire lifetime.
    /// `None` omits the entry module, as in [`Self::new`]. Module linkage is reset
    /// exactly as in [`Self::with_module_and_vfs`]. Worker contexts share this state.
    pub fn with_extensions(
        module: Option<crate::vm::runtime::Module>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
        extensions: crate::vm::ext::VmExtensions,
    ) -> std::pin::Pin<Box<Self>> {
        let module = module.map(|mut module| {
            module.clear_link_state();
            Arc::new(module)
        });
        Self::new_internal(
            module, vfs, crate::assembly::extension::freeze(extensions),
            crate::config::runtime_config().vm_config(),
            crate::assembly::object_protocol::services(),
        )
    }

    /// Instantiate shared package definitions with fresh per-VM types and caches.
    pub fn with_image(
        image: Arc<crate::metadata::image::PackageImage>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        Self::with_image_and_vfs(image, crate::pal::fs::default_vfs())
    }

    /// Instantiate an image using this VM's file system for subsequent loads.
    pub fn with_image_and_vfs(
        image: Arc<crate::metadata::image::PackageImage>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        let mut module = crate::vm::runtime::Module::from_image(image);
        crate::vm::runtime::types::build_type_registry(&mut module);
        crate::vm::runtime::types::verify_constraints(&module)?;
        crate::vm::loader::package::build_block_indices(&mut module);
        Ok(Self::with_module_and_vfs(module, vfs))
    }

    fn new_internal(
        module: Option<Arc<crate::vm::runtime::Module>>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
        ext_state: crate::vm::ext::ExtensionState,
        config: crate::vm::config::VmConfig,
        object_protocol: crate::vm::exec::object_protocol::ObjectProtocol,
    ) -> std::pin::Pin<Box<Self>> {
        // Construct VmCore with heap embedded; scanner is installed AFTER
        // wrapping in Arc so the closure can capture Weak<VmCore> (cycle
        // avoidance: heap owns the scanner, scanner refs core, core owns
        // heap → strong-Arc loop = leak). Weak.upgrade() per call.
        let core: Arc<VmCore> = Arc::new(VmCore {
            threads: crate::vm::threads::ThreadState::new(),
            loader: crate::vm::loader::LoaderState::new(vfs, crate::assembly::loader::FUSION_INPUTS),
            heap:                 Box::new(Heap::new(config.gc)),
            runtime: crate::vm::runtime::RuntimeState::new(module, &crate::assembly::builtin::CATALOG),

            diagnostics: crate::vm::diagnostics::DiagnosticsState {
                counters: Arc::new(crate::vm::diagnostics::counters::RuntimeCounters::new()),
                runtime_observers: crate::vm::diagnostics::observer::RuntimeObserverRegistry::new(),
                park_histogram: Mutex::new(crate::gc::types::PauseHistogram::new(config.gc.pause_window)),
                lock_contentions: std::sync::atomic::AtomicU64::new(0),
                lock_wait_us:     std::sync::atomic::AtomicU64::new(0),
                // Construct the backend here; the VM owns only its sampling capability.
                sampler: {
                    let backend = match config.sample_hz {
                        Some(hz) => crate::profiler::cpu::Sampler::start(hz, config.trace_timeline),
                        None => crate::profiler::cpu::Sampler::disabled(),
                    };
                    crate::vm::diagnostics::sampling::Sampling::new(backend.enabled(), backend)
                },
            },
            exec: crate::vm::exec::ExecState::new(
                crate::assembly::engine::registry(), crate::assembly::builtin::BUILTINS, object_protocol,
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
                    crate::vm::threads::safepoint::poke_safepoints(&c);
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

        core.heap.set_external_root_scanner(Box::new(crate::vm::threads::roots::CoreRoots(Arc::downgrade(&core))));

        // add-lazy-context-unload: wire the collectible-context reclaimer so a
        // major GC reclaims `Unloading` AssemblyLoadContexts with no live refs.
        // Captures `Weak<VmCore>` (cycle avoidance) + the unloading-count flag.
        {
            let unloading = core.loader.context_registry.lock().unloading_flag();
            core.heap.set_context_reclaimer(Box::new(crate::vm::loader::context_gc::CoreContextReclaimer {
                core: Arc::downgrade(&core),
                unloading,
            }));
        }

        // add-heap-retention-diagnostics: wire the CATEGORIZED root scanner (for
        // retention-query L2 root reporting). Mirrors the anonymous mark scanner
        // above but tags each root with its `RootKind`. Called only on-demand by
        // `retention_roots`, never on the mark hot path.
        {
            let core_weak = Arc::downgrade(&core);
            core.heap.set_categorized_root_scanner(Box::new(move |visit| {
                use crate::gc::walk::RootKind;
                let Some(c) = core_weak.upgrade() else { return; };
                c.runtime.statics.scan_roots(&mut |v| visit(v, RootKind::StaticField));
                // Interned `ConstStr` literals (VM-wide, like static fields).
                c.runtime.strings.scan_roots(|s| visit(&Value::Str(s), RootKind::StaticField));
                let registry = c.threads.vm_contexts.lock();
                for ctx_ptr in registry.iter() {
                    // SAFETY: same invariant as the anonymous root scanner —
                    // entries are removed in `VmContext::drop` before dealloc,
                    // and we hold `vm_contexts.lock()` for the whole walk.
                    let ctx = unsafe { &*ctx_ptr.0 };
                    if let Some(v) = ctx.exec.pending_exception.lock().as_ref() {
                        visit(v, RootKind::StackFrame);
                    }
                    if let Some(v) = ctx.exec.pending_thrown.lock().as_ref() {
                        visit(v, RootKind::StackFrame);
                    }
                    // SAFETY: as in the mark scanner above — callers hold the
                    // GC pause, so other contexts are parked.
                    unsafe {
                        ctx.scan_frames_parked(|frames| {
                            crate::vm::exec::stack::visit_roots(frames, |v| visit(v, RootKind::StackFrame));
                        });
                    }
                    ctx.exec.stack_arena
                        .lock()
                        .scan_roots(&mut |v| visit(v, RootKind::StackFrame));
                    // add-struct-value-semantics: value-struct blob reference leaves.
                    ctx.exec.struct_arena
                        .lock()
                        .scan_roots(&mut |v| visit(v, RootKind::StackFrame));
                    // make-value-copy: transient payloads' GC leaves.
                    ctx.exec.transient_arena
                        .lock()
                        .scan_roots(&mut |v| visit(v, RootKind::StackFrame));
                }
            }));
        }

        let ctx = Self {
            exec: crate::vm::exec::ThreadExecState::new(&core.runtime.cctors, &core.exec.engines),
            core,
            diagnostics: Default::default(),
            runtime: crate::vm::runtime::ThreadRuntimeState::new(),
            threads: crate::vm::threads::ThreadLocalState::new(),
            _pin: PhantomPinned,
        };
        // Heap-allocate so the address is stable for the scanner registry.
        let boxed = Box::new(ctx);
        // SAFETY: VmContext registers its OWN address into VmCore.threads.vm_contexts
        // here and removes the entry in Drop (running BEFORE Box dealloc).
        // The Pin wrapper + PhantomPinned prevents any subsequent move-out.
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        boxed.core.threads.vm_contexts.lock().push(ptr);

        // add-z42-compression : register ext builtins that need no
        // I/O (the bundled wasm build). The dlopen build loads a library on the
        // first lookup miss for one of its builtins (`ext::ensure_lib_for`).
        // Workers via `new_with_core` share the core's table.
        #[cfg(feature = "native-interop")]
        if let Err(e) = crate::interop::ext::load_all(&boxed) {
            tracing::warn!("native ext loader: {:#}", e);
        }

        // SAFETY: We never expose `&mut Box<VmContext>` to user code (only
        // `Pin<&mut VmContext>` via `Pin::as_mut`, which respects
        // `PhantomPinned`), so the contents stay at a stable address until
        // Drop. Constructing the Pin here is the standard idiom for
        // self-referential heap data.
        // add-gc-tlab (stage 2): arm this thread for TLAB allocation (balanced
        // in Drop). The primary VM thread now takes the lock-free alloc path.
        crate::gc::tlab::arm();
        // add-incremental-major-gc M2a: this thread's SATB records belong to this heap (balanced in Drop).
        crate::gc::satb::bind_thread(boxed.core.heap.heap_epoch());
        unsafe { std::pin::Pin::new_unchecked(boxed) }
    }
}

#[cfg(test)]
#[path = "context_assembly_tests.rs"]
mod tests;
