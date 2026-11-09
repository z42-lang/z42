use super::*;

impl VmContext {
    /// Public accessor for the shared compiled Module installed by
    /// [`with_module`](Self::with_module). Returns `None` if VmContext was
    /// built via [`new`](Self::new) (test path).
    pub fn module(&self) -> Option<&Arc<crate::vm::runtime::Module>> {
        self.core.runtime.module.as_ref()
    }

    /// The VM's function table (process-level `FnId`s; see `vm::runtime::ids::func_table`).
    #[inline]
    pub fn funcs(&self) -> &crate::vm::runtime::ids::func_table::FuncTable {
        &self.core.runtime.funcs
    }

    /// The VM's type table (process-global `TypeId`s; see `vm::runtime::ids::type_table`).
    #[inline]
    pub fn types(&self) -> &crate::vm::runtime::ids::type_table::TypeTable {
        &self.core.runtime.types
    }

    /// Clone the shared `Arc<VmCore>` — needed by external integration
    /// tests / embedders that spawn raw OS threads and want to construct
    /// a child VmContext via [`new_with_core`](Self::new_with_core). The
    /// `core` field itself is `pub`; this is the public escape hatch.
    pub fn core_arc(&self) -> Arc<VmCore> {
        Arc::clone(&self.core)
    }

    /// Public accessor for runtime atomic counters. Used by main.rs
    /// `--print-stats-on-exit` flag and embedders that want to observe
    /// JIT compiles / builtin calls / exception traffic.
    /// docs/review.md Part 4 D6 Phase 1 .
    /// This thread's counters — increment sites use these. For a total across
    /// the VM's threads use [`Self::counters_snapshot`].
    pub fn counters(&self) -> &crate::vm::diagnostics::counters::RuntimeCounters {
        &self.diagnostics.counters
    }

    /// The VM-wide totals: every live context plus those already dropped.
    /// The registry lock makes it exact with respect to a context dropping
    /// concurrently (its counts move to the core under the same lock).
    pub fn counters_snapshot(&self) -> crate::vm::diagnostics::counters::Snapshot {
        let registry = self.core.threads.vm_contexts.lock();
        let mut total = self.core.diagnostics.counters.snapshot();
        for p in registry.iter() {
            // SAFETY: registered contexts are alive while the registry lock is
            // held (`Drop` deregisters under it before the storage goes away).
            total = total.add(unsafe { &*p.0 }.diagnostics.counters.snapshot());
        }
        total
    }

    /// Register a [`crate::vm::diagnostics::observer::RuntimeObserver`]. The observer
    /// receives every subsequent [`crate::vm::diagnostics::observer::RuntimeEvent`] fired
    /// via [`Self::fire_runtime_event`]. Multiple observers fan-out.
    /// docs/review.md Part 4 D3 Phase 1 .
    pub fn add_runtime_observer(&self, obs: Arc<dyn crate::vm::diagnostics::observer::RuntimeObserver>) {
        self.core.diagnostics.runtime_observers.add(obs);
    }

    /// Fire a runtime event to all registered observers. Returns the
    /// number of observers that received the event. No-op when registry
    /// is empty (cost = one lock acquire + length check). Safe to call
    /// from any thread; observer callbacks must be `Send + Sync` so they
    /// handle cross-thread invocation themselves.
    /// Whether a runtime observer is registered — gate for building costly events.
    #[inline]
    pub fn has_runtime_observers(&self) -> bool {
        self.core.diagnostics.runtime_observers.has_observers()
    }

    pub fn fire_runtime_event(&self, event: &crate::vm::diagnostics::observer::RuntimeEvent) -> usize {
        self.core.diagnostics.runtime_observers.fire(event)
    }

    /// add-gc-safepoint-counter-throttling : force the next
    /// `check_safepoint` call into the slow path immediately, bypassing
    /// the throttle counter. For tests and embedders that need
    /// deterministic safepoint timing — production code should not need
    /// this (the throttle counter caps GC pause latency at N iterations
    /// which is small enough in practice).
    pub fn force_safepoint(&self) {
        self.threads.safepoint_skip.store(1, std::sync::atomic::Ordering::Relaxed);
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
            exec: crate::vm::exec::ThreadExecState::new(&core.runtime.cctors, &core.exec.engines),
            core,
            diagnostics: Default::default(),
            runtime: crate::vm::runtime::ThreadRuntimeState::new(),
            threads: crate::vm::threads::ThreadLocalState::new(safepoint_throttle),
            _pin: PhantomPinned,
        };
        let boxed = Box::new(ctx);
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        // **fix-context-joins-mid-pause **: never join while the collector is
        // `Marking`. By then it has counted the registered contexts as parked and is scanning
        // their roots and sweeping; one that registers now is neither parked nor scanned — it
        // runs on and allocates into regions mid-sweep, and nothing marks those objects (a
        // client thread's fresh `HttpHeaders` came back with `_count` = Null).
        //
        // Joining during `Requested` stays allowed: `request_gc_pause` re-reads
        // `vm_contexts.len()` on every wakeup and does not flip to `Marking` until this
        // context has parked too. The check-and-push runs under the phase lock, which the
        // collector holds across its own count-and-flip, so the two cannot interleave; the lock
        // order (`gc_phase` → `vm_contexts`) is the collector's.
        //
        // The wait is not a park: this thread is not registered, so no collector waits for it.
        // It does mean the thread reaches its first collection attempt only after the pause,
        // where it can win the collector role — safe because every thread blocked outside the
        // VM is parked (`NativeParkGuard`, see docs/internals/src/runtime/native-gc-contract.md). loom model B′ in
        // tests/gc_registration_race_loom.rs checks both halves.
        {
            let mut phase = boxed.core.threads.gc_phase.lock();
            while matches!(*phase, crate::vm::threads::safepoint::GcPhase::Marking) {
                boxed.core.threads.gc_phase_cv.wait(&mut phase);
            }
            boxed.core.threads.vm_contexts.lock().push(ptr);
        }
        // add-gc-tlab (stage 2): arm this thread for TLAB allocation (balanced
        // in Drop). A spawned worker allocates into the TLAB fast path.
        crate::gc::tlab::arm();
        // add-incremental-major-gc M2a: this thread's SATB records belong to this heap (balanced in Drop).
        crate::gc::satb::bind_thread(boxed.core.heap.heap_epoch());
        unsafe { std::pin::Pin::new_unchecked(boxed) }
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
        {
            // runtime-audit P0-15: move this thread's counts into the core's
            // total under the registry lock, so `counters_snapshot` never sees
            // them twice or not at all.
            let mut registry = self.core.threads.vm_contexts.lock();
            self.core.diagnostics.counters.absorb(&self.diagnostics.counters);
            registry.retain(|p| p.0 != ptr);
        }
        // Wake any collector sleeping in request_gc_pause so it
        // re-evaluates the required park count. Our removal from vm_contexts
        // may lower vm_contexts.len()-1 below the current parked_count,
        // satisfying the wait condition. Must hold gc_phase lock to prevent
        // a missed-wakeup between the condition re-check and the wait call.
        let _g = self.core.threads.gc_phase.lock();
        self.core.threads.gc_phase_cv.notify_all();
    }
}
