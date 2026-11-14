//! Counter aggregation and runtime event access for this VM.
use super::*;

impl VmContext {
    /// Public accessor for runtime atomic counters. Used by main.rs
    /// `--print-stats-on-exit` flag and embedders that want to observe
    /// JIT compiles / builtin calls / exception traffic.
    /// docs/review.md Part 4 D6 Phase 1 .
    /// This thread's counters — increment sites use these. For a total across
    /// the VM's threads use [`Self::counters_snapshot`].
    pub fn counters(&self) -> &crate::diagnostics::counters::RuntimeCounters {
        &self.diagnostics.counters
    }

    /// The VM-wide totals: every live context plus those already dropped.
    /// The registry lock makes it exact with respect to a context dropping
    /// concurrently (its counts move to the core under the same lock).
    pub fn counters_snapshot(&self) -> crate::diagnostics::counters::Snapshot {
        let registry = self.core.threads.vm_contexts.lock();
        let mut total = self.core.diagnostics.counters.snapshot();
        for p in registry.iter() {
            // SAFETY: registered contexts are alive while the registry lock is
            // held (`Drop` deregisters under it before the storage goes away).
            total = total.add(unsafe { &*p.0 }.diagnostics.counters.snapshot());
        }
        total
    }

    /// Register a [`crate::diagnostics::observer::RuntimeObserver`]. The observer
    /// receives every subsequent [`crate::diagnostics::observer::RuntimeEvent`] fired
    /// via [`Self::fire_runtime_event`]. Multiple observers fan-out.
    /// docs/review.md Part 4 D3 Phase 1 .
    pub fn add_runtime_observer(&self, obs: Arc<dyn crate::diagnostics::observer::RuntimeObserver>) {
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

    pub fn fire_runtime_event(&self, event: &crate::diagnostics::observer::RuntimeEvent) -> usize {
        self.core.diagnostics.runtime_observers.fire(event)
    }

}
