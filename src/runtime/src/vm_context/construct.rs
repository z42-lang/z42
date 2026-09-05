use super::*;

impl VmContext {
    /// Public accessor for the shared compiled Module installed by
    /// [`with_module`](Self::with_module). Returns `None` if VmContext was
    /// built via [`new`](Self::new) (test path).
    pub fn module(&self) -> Option<&Arc<crate::metadata::Module>> {
        self.core.module.as_ref()
    }

    /// Clone the shared `Arc<VmCore>` — needed by external integration
    /// tests / embedders that spawn raw OS threads and want to construct
    /// a child VmContext via [`new_with_core`](Self::new_with_core). The
    /// `core` field itself is `pub(crate)`; this is the public escape hatch.
    pub fn core_arc(&self) -> Arc<VmCore> {
        Arc::clone(&self.core)
    }

    /// Public accessor for runtime atomic counters. Used by main.rs
    /// `--print-stats-on-exit` flag and embedders that want to observe
    /// JIT compiles / builtin calls / exception traffic.
    /// docs/review.md Part 4 D6 Phase 1 (2026-05-26).
    pub fn counters(&self) -> &crate::counters::RuntimeCounters {
        &self.core.counters
    }

    /// Register a [`crate::observer::RuntimeObserver`]. The observer
    /// receives every subsequent [`crate::observer::RuntimeEvent`] fired
    /// via [`Self::fire_runtime_event`]. Multiple observers fan-out.
    /// docs/review.md Part 4 D3 Phase 1 (2026-05-26).
    pub fn add_runtime_observer(&self, obs: Arc<dyn crate::observer::RuntimeObserver>) {
        self.core.runtime_observers.add(obs);
    }

    /// Fire a runtime event to all registered observers. Returns the
    /// number of observers that received the event. No-op when registry
    /// is empty (cost = one lock acquire + length check). Safe to call
    /// from any thread; observer callbacks must be `Send + Sync` so they
    /// handle cross-thread invocation themselves.
    pub fn fire_runtime_event(&self, event: &crate::observer::RuntimeEvent) -> usize {
        self.core.runtime_observers.fire(event)
    }

    /// add-gc-safepoint-counter-throttling (2026-05-21): force the next
    /// `check_safepoint` call into the slow path immediately, bypassing
    /// the throttle counter. For tests and embedders that need
    /// deterministic safepoint timing — production code should not need
    /// this (the throttle counter caps GC pause latency at N iterations
    /// which is small enough in practice).
    pub fn force_safepoint(&self) {
        self.safepoint_skip.store(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// **add-z42-launcher (2026-06-02)**: install the running program's
    /// command-line arguments (the tokens after `--` on the `z42vm`
    /// invocation). Read back by the `__env_args` builtin →
    /// `Std.IO.Environment.GetCommandLineArgs()`. Called once at startup
    /// from `main.rs`; safe to call from any context sharing this `VmCore`.
    pub fn set_program_args(&self, args: Vec<String>) {
        *self.core.program_args.lock() = args;
    }

    /// **add-z42-launcher (2026-06-02)**: snapshot the program argv.
    pub fn program_args(&self) -> Vec<String> {
        self.core.program_args.lock().clone()
    }

    /// Standard test entry: constructs a VmContext with `VmCore.module = None`.
    /// Cargo unit tests use this — they don't need a real Module for
    /// builtin / static-field / alloc tests. Production paths use
    /// [`with_module`](Self::with_module).
    pub fn new() -> std::pin::Pin<Box<Self>> {
        Self::new_internal(None)
    }

    /// Production entry: constructs a VmContext with the user's compiled
    /// Module wrapped in `Arc` for cross-thread sharing. Required by any
    /// path that may invoke `__thread_spawn` (which dispatches into the
    /// shared module from the spawned thread).
    pub fn with_module(module: crate::metadata::Module) -> std::pin::Pin<Box<Self>> {
        Self::new_internal(Some(Arc::new(module)))
    }

    /// Spawned-thread entry: build a VmContext that **shares** an existing
    /// `Arc<VmCore>` instead of creating a new one. Used by
    /// `__thread_spawn`'s spawned closure so the worker thread sees the
    /// same module / static_fields / heap / lazy_loader / native_libs
    /// state as the parent thread.
    ///
    /// The new VmContext registers itself in `core.vm_contexts` for GC root
    /// scanning, so the worker's per-thread roots (`pending_exception` /
    /// `call_stack` / `func_ref_slots`) are visible to the cycle collector.
    /// On drop, the entry is removed under the same Mutex discipline as
    /// the primary path.
    pub fn new_with_core(core: Arc<VmCore>) -> std::pin::Pin<Box<Self>> {
        let pending_exception: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let func_ref_slots: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let call_stack: Arc<Mutex<Vec<crate::exception::VmFrame>>> = Arc::new(Mutex::new(Vec::new()));

        let ctx = Self {
            core,
            pending_exception,
            call_stack,
            stack_arena: Arc::new(Mutex::new(Default::default())),
            struct_arena: Arc::new(Mutex::new(Default::default())),
            transient_arena: Arc::new(Mutex::new(Default::default())),
            stack_obj_len: std::sync::atomic::AtomicUsize::new(0),
            stack_arr_len: std::sync::atomic::AtomicUsize::new(0),
            struct_len: std::sync::atomic::AtomicUsize::new(0),
            transient_len: std::sync::atomic::AtomicUsize::new(0),
            next_frame_id: std::sync::atomic::AtomicU32::new(1),
            jit_ctx: std::sync::atomic::AtomicUsize::new(0),
            func_ref_slots,
            interned_cache: Arc::new(Mutex::new(FxHashMap::default())),
            subclass_memo: Mutex::new(FxHashMap::default()),
            isa_cache: super::isa_cache::IsaCache::new(),
            safepoint_skip: std::sync::atomic::AtomicU32::new(crate::gc::safepoint::throttle_n()),
            _pin: PhantomPinned,
        };
        let boxed = Box::new(ctx);
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        boxed.core.vm_contexts.lock().push(ptr);
        // add-gc-tlab (stage 2): arm this thread for TLAB allocation (balanced
        // in Drop). A spawned worker allocates into the TLAB fast path.
        crate::gc::tlab::arm();
        unsafe { std::pin::Pin::new_unchecked(boxed) }
    }

    fn new_internal(module: Option<Arc<crate::metadata::Module>>) -> std::pin::Pin<Box<Self>> {
        let pending_exception: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let func_ref_slots: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        // 2026-05-10 unify-frame-chain: single Vec<VmFrame> replaces the
        // previous trio (exec_stack / env_arena_stack / call_stack).
        let call_stack: Arc<Mutex<Vec<crate::exception::VmFrame>>> = Arc::new(Mutex::new(Vec::new()));

        // Construct VmCore with heap embedded; scanner is installed AFTER
        // wrapping in Arc so the closure can capture Weak<VmCore> (cycle
        // avoidance: heap owns the scanner, scanner refs core, core owns
        // heap → strong-Arc loop = leak). Weak.upgrade() per call.
        let core: Arc<VmCore> = Arc::new(VmCore {
            static_fields:      Mutex::new(Vec::new()),
            static_field_index: Mutex::new(FxHashMap::default()),
            lazy_loader:        RwLock::new(None),
            pending_type_inits: Mutex::new(Vec::new()),
            pending_type_init_count: std::sync::atomic::AtomicUsize::new(0),
            running_static_inits:    std::sync::atomic::AtomicUsize::new(0),
            static_init_error:  Mutex::new(None),
            context_registry:   Mutex::new(crate::metadata::context::ContextRegistry::new()),
            #[cfg(feature = "native-interop")]
            native_types:       RwLock::new(HashMap::new()),
            #[cfg(feature = "native-interop")]
            native_libs:        Mutex::new(Vec::new()),
            pinned_owned_buffers: Mutex::new(HashMap::new()),
            pending_thrown:       Mutex::new(None),
            processes:            ResourceRegistry::new(),
            heap:                 Box::new(ArcMagrGC::new()),
            vm_contexts:          Mutex::new(Vec::new()),
            module,
            threads:              ResourceRegistry::new(),
            mutexes:              ResourceRegistry::new(),
            channels:             ResourceRegistry::new(),
            gc_phase:             Mutex::new(crate::gc::safepoint::GcPhase::Idle),
            gc_phase_cv:          parking_lot::Condvar::new(),
            parked_count:         std::sync::atomic::AtomicUsize::new(0),
            collector_active:     std::sync::atomic::AtomicBool::new(false),
            needs_auto_collect:   Arc::new(std::sync::atomic::AtomicBool::new(false)),
            program_args:         Mutex::new(Vec::new()),
            rwlocks:              ResourceRegistry::new(),
            #[cfg(feature = "native-interop")]
            ext_builtins:         Mutex::new(crate::native::ext::ExtBuiltinTable::default()),
            file_handles:         ResourceRegistry::new(),
            #[cfg(not(target_arch = "wasm32"))]
            tcp_sockets:          ResourceRegistry::new(),
            #[cfg(not(target_arch = "wasm32"))]
            tcp_listeners:        ResourceRegistry::new(),
            #[cfg(not(target_arch = "wasm32"))]
            tls_sockets:          ResourceRegistry::new(),
            #[cfg(not(target_arch = "wasm32"))]
            udp_sockets:          ResourceRegistry::new(),

            // add-runtime-counters (2026-05-26): all-zero start; Phase 1
            // increment site = corelib::exec_builtin (builtin_calls).
            counters: Arc::new(crate::counters::RuntimeCounters::new()),
            // add-runtime-observer (2026-05-26): empty registry; embedders
            // attach via `VmContext::add_runtime_observer`. Phase 1 emits
            // ModuleLoaded from main.rs after each load_artifact.
            runtime_observers: crate::observer::RuntimeObserverRegistry::new(),
            // add-concurrency-probes (2026-08-23): empty park histogram + zeroed
            // contention counters (only written under the `profile-contention` feature).
            park_histogram: Mutex::new(crate::gc::types::PauseHistogram::default()),
            lock_contentions: std::sync::atomic::AtomicU64::new(0),
            lock_wait_us:     std::sync::atomic::AtomicU64::new(0),
            // add-sampling-profiler (2026-08-24): start the sampler only when
            // Z42_SAMPLE_HZ is set; otherwise disabled() (no thread, zero-cost).
            // Trace timeline recorded only when Z42_TRACE_OUT is also set.
            sampler: match crate::config::runtime_config().sample_hz {
                Some(hz) => crate::gc::Sampler::start(
                    hz,
                    crate::config::runtime_config().trace_out.is_some(),
                ),
                None => crate::gc::Sampler::disabled(),
            },
        });

        // add-os-signal-handler (2026-05-25): register this Arc<VmCore> into
        // the process-wide VM_CORES registry so the POSIX signal handler
        // can walk it to capture z42 call stacks on hard crash. Lazy-prune
        // dead Weak entries at the same time (cheap O(n) sweep amortized
        // across VmCore creations).
        if let Ok(mut g) = VM_CORES.lock() {
            g.retain(|w| w.strong_count() > 0);
            g.push(Arc::downgrade(&core));
        }

        // add-gc-safepoint-auto-threshold (2026-05-20): wire the
        // needs_auto_collect flag into the heap so its pressure-trip
        // path defers to the next safepoint check instead of collecting
        // inline (the inline path has no &VmContext and would race with
        // concurrent mutators' frame.regs writes).
        core.heap.set_external_needs_collect_flag(Arc::clone(&core.needs_auto_collect));

        // add-gc-runtime-knobs (2026-09-05): apply the process-wide GC knobs.
        //
        // `Z42_GC_MAX_BYTES` is what *arms* automatic collection at all —
        // `maybe_auto_collect` returns immediately while `max_bytes` is `None`,
        // and the three `Z42_GC_*_RATIO` knobs are fractions of this budget, so
        // without it they are inert. Unset keeps the historical behaviour
        // (collect only on an explicit `Std.GC.Collect()`).
        {
            let cfg = crate::config::runtime_config();
            if let Some(max) = cfg.gc_max_bytes {
                core.heap.set_max_heap_bytes(Some(max));
            }
            if cfg.gc_trace {
                core.heap.add_observer(Arc::new(crate::gc::trace::GcTracer::default()));
            }
        }

        // External GC root scanner — invoked by the cycle collector during
        // mark phase. Walks all out-of-heap Value sources so cycles whose
        // only roots are static fields / pending exception / live frame
        // regs / stack closure envs / func-ref slots stay alive.
        //
        // **add-vmcontext-registry (2026-05-20)**: scanner walks the
        // `vm_contexts` registry to find every live VmContext on this
        // VmCore. Each VmContext contributes its own per-thread roots
        // (pending_exception / call_stack frames / func_ref_slots). The
        // closure captures `Weak<VmCore>` ONLY — no per-thread Arc clones.
        {
            let core_weak = Arc::downgrade(&core);
            core.heap.set_external_root_scanner(Box::new(move |visit| {
                let Some(c) = core_weak.upgrade() else { return; };
                // 1. Shared static fields.
                for v in c.static_fields.lock().iter() {
                    visit(v);
                }
                // 2-4. Per-thread roots, one VmContext per OS thread.
                //
                // SAFETY: each VmContextPtr was registered via
                // `VmContext::new()` *after* its `Pin<Box<...>>` heap
                // allocation, and `VmContext::drop` removes the entry
                // BEFORE the Box is dealloc'd. We hold `vm_contexts.lock()`
                // for the full walk, so a concurrent drop on another thread
                // blocks until we release — no use-after-free possible.
                let registry = c.vm_contexts.lock();
                for ctx_ptr in registry.iter() {
                    let ctx = unsafe { &*ctx_ptr.0 };
                    // pending_exception
                    if let Some(v) = ctx.pending_exception.lock().as_ref() {
                        visit(v);
                    }
                    // live z42 frame state — unified VmFrame entries.
                    //
                    // SAFETY (frame.regs / env_arena): raw ptrs valid for
                    // the lifetime of the owning Rust frame (FrameGuard
                    // RAII for interp; paired push/pop for JIT). GC
                    // collect is invoked from inside script code, so
                    // every walk sees pointers still in-bounds.
                    for frame in ctx.call_stack.lock().iter() {
                        unsafe {
                            for v in (*frame.regs).iter() {
                                visit(v);
                            }
                            if !frame.env_arena.is_null() {
                                for env in (*frame.env_arena).iter() {
                                    for v in env.iter() {
                                        visit(v);
                                    }
                                }
                            }
                        }
                    }
                    // method group conversion cache slots (D1b).
                    for v in ctx.func_ref_slots.lock().iter() {
                        visit(v);
                    }
                    // unify-gc-heap PR-4: per-context interned string cache — the GC
                    // strings lazily allocated for ConstStr pool literals live only
                    // here (not in any frame reg after their instruction), so they
                    // are roots: visit each so the interned block stays marked.
                    for s in ctx.interned_cache.lock().values() {
                        visit(&Value::Str(*s));
                    }
                    // add-escape-analysis-stack-alloc: stack-alloc arena roots.
                    // A stack object's slots / stack array's elements may hold heap
                    // GcRefs that must stay marked while the stack object is live.
                    // (The stack objects themselves are not GC-heap entries and are
                    // freed by frame-exit truncation, not sweep.) The arena lock is
                    // never held across a GC trigger, so this cannot deadlock.
                    ctx.stack_arena.lock().scan_roots(visit);
                    // add-struct-value-semantics: value-struct blobs' reference
                    // leaves are GC roots too (no-op for pure-primitive structs;
                    // ref-leaf scanning by type ref-bitmap lands in A-use).
                    ctx.struct_arena.lock().scan_roots(visit);
                    // make-value-copy: transient payloads (Ref target / StructRefHeap
                    // backing array) hold GcRefs that must stay marked while the handle
                    // is live; the arena is the root (handles are not traced through).
                    ctx.transient_arena.lock().scan_roots(visit);
                }
            }));
        }

        // add-lazy-context-unload: wire the collectible-context reclaimer so a
        // major GC reclaims `Unloading` AssemblyLoadContexts with no live refs.
        // Captures `Weak<VmCore>` (cycle avoidance) + the unloading-count flag.
        {
            let unloading = core.context_registry.lock().unloading_flag();
            core.heap.set_context_reclaimer(Box::new(CoreContextReclaimer {
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
                use crate::gc::retention::RootKind;
                let Some(c) = core_weak.upgrade() else { return; };
                for v in c.static_fields.lock().iter() {
                    visit(v, RootKind::StaticField);
                }
                let registry = c.vm_contexts.lock();
                for ctx_ptr in registry.iter() {
                    // SAFETY: same invariant as the anonymous root scanner —
                    // entries are removed in `VmContext::drop` before dealloc,
                    // and we hold `vm_contexts.lock()` for the whole walk.
                    let ctx = unsafe { &*ctx_ptr.0 };
                    if let Some(v) = ctx.pending_exception.lock().as_ref() {
                        visit(v, RootKind::StackFrame);
                    }
                    for frame in ctx.call_stack.lock().iter() {
                        unsafe {
                            for v in (*frame.regs).iter() {
                                visit(v, RootKind::StackFrame);
                            }
                            if !frame.env_arena.is_null() {
                                for env in (*frame.env_arena).iter() {
                                    for v in env.iter() {
                                        visit(v, RootKind::StackFrame);
                                    }
                                }
                            }
                        }
                    }
                    for v in ctx.func_ref_slots.lock().iter() {
                        visit(v, RootKind::FuncRefSlot);
                    }
                    ctx.stack_arena
                        .lock()
                        .scan_roots(&mut |v| visit(v, RootKind::StackFrame));
                    // add-struct-value-semantics: value-struct blob reference leaves.
                    ctx.struct_arena
                        .lock()
                        .scan_roots(&mut |v| visit(v, RootKind::StackFrame));
                    // make-value-copy: transient payloads' GC leaves.
                    ctx.transient_arena
                        .lock()
                        .scan_roots(&mut |v| visit(v, RootKind::StackFrame));
                }
            }));
        }

        let ctx = Self {
            core,
            pending_exception,
            call_stack,
            stack_arena: Arc::new(Mutex::new(Default::default())),
            struct_arena: Arc::new(Mutex::new(Default::default())),
            transient_arena: Arc::new(Mutex::new(Default::default())),
            stack_obj_len: std::sync::atomic::AtomicUsize::new(0),
            stack_arr_len: std::sync::atomic::AtomicUsize::new(0),
            struct_len: std::sync::atomic::AtomicUsize::new(0),
            transient_len: std::sync::atomic::AtomicUsize::new(0),
            next_frame_id: std::sync::atomic::AtomicU32::new(1),
            jit_ctx: std::sync::atomic::AtomicUsize::new(0),
            func_ref_slots,
            interned_cache: Arc::new(Mutex::new(FxHashMap::default())),
            subclass_memo: Mutex::new(FxHashMap::default()),
            isa_cache: super::isa_cache::IsaCache::new(),
            safepoint_skip: std::sync::atomic::AtomicU32::new(crate::gc::safepoint::throttle_n()),
            _pin: PhantomPinned,
        };
        // Heap-allocate so the address is stable for the scanner registry.
        let boxed = Box::new(ctx);
        // SAFETY: VmContext registers its OWN address into VmCore.vm_contexts
        // here and removes the entry in Drop (running BEFORE Box dealloc).
        // The Pin wrapper + PhantomPinned prevents any subsequent move-out.
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        boxed.core.vm_contexts.lock().push(ptr);

        // add-z42-compression (2026-05-22): scan native search paths +
        // dlopen each lib*.{so,dylib,dll}, populating `ext_builtins`. Run
        // once at primary-VM init only (workers via `new_with_core` reuse
        // the parent's populated table). Failures are logged but never
        // abort startup — apps that don't need any ext lib still boot.
        #[cfg(feature = "native-interop")]
        if let Err(e) = crate::native::ext::load_all(&boxed) {
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
        unsafe { std::pin::Pin::new_unchecked(boxed) }
    }
}

impl Drop for VmContext {
    /// **add-vmcontext-registry (2026-05-20)**: deregister this `VmContext`
    /// from `VmCore.vm_contexts` so the GC scanner stops trying to walk a
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
        // add-gc-tlab (stage 2): balance the arm() from construction.
        crate::gc::tlab::disarm();
        let ptr = self as *const Self;
        self.core.vm_contexts.lock().retain(|p| p.0 != ptr);
        // Wake any collector sleeping in request_handshake_pause so it
        // re-evaluates the required park count. Our removal from vm_contexts
        // may lower vm_contexts.len()-1 below the current parked_count,
        // satisfying the wait condition. Must hold gc_phase lock to prevent
        // a missed-wakeup between the condition re-check and the wait call.
        let _g = self.core.gc_phase.lock();
        self.core.gc_phase_cv.notify_all();
    }
}
