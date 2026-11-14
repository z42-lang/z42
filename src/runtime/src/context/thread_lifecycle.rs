//! Thread attachment and balanced deregistration from a shared VM.
use super::*;

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
            while matches!(*phase, crate::threads::safepoint::GcPhase::Marking) {
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
