//! Shared mutator coordination state for one VM.
use parking_lot::{Condvar, Mutex};
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize}};
use crate::vm::VmContext;
use super::safepoint::GcPhase;

pub struct ThreadState {
    pub safepoint_throttle: u32,
    /// Fatal VM errors propagate to all contexts sharing this core.
    /// Unwinding bypasses z42 handlers and cannot become a catchable z42 exception.
    pub fatal: AtomicBool,
    /// Registered contexts remain alive while a visitor holds this lock.
    vm_contexts: Mutex<Vec<VmContextPtr>>,
    /// Registration and collector transitions follow the same phase-lock protocol.
    pub gc_phase: Mutex<GcPhase>,
    pub gc_phase_cv: Condvar,
    /// Parked mutators, excluding the active collector.
    pub parked_count: AtomicUsize,
    /// CAS winner is the only collector until the pause guard releases the claim.
    pub collector_active: AtomicBool,
    /// Heap pressure publishes a sticky request; only a successful collector clears it.
    pub needs_auto_collect: Arc<AtomicBool>,
}

impl ThreadState {
    pub fn new(safepoint_throttle: u32) -> Self {
        assert!(safepoint_throttle > 0, "safepoint throttle must be positive");
        Self {
            safepoint_throttle,
            fatal: AtomicBool::new(false),
            vm_contexts: Mutex::new(Vec::new()),
            gc_phase: Mutex::new(GcPhase::Idle),
            gc_phase_cv: Condvar::new(),
            parked_count: AtomicUsize::new(0),
            collector_active: AtomicBool::new(false),
            needs_auto_collect: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Mutator-local pause/allocation state, shared with the collector only while parked.
pub struct ThreadLocalState {
    pub parked_tlab: crate::gc::tlab::ParkedTlabSlot,
    pub safepoint_skip: std::sync::atomic::AtomicU32,
}

impl ThreadLocalState {
    pub fn new(safepoint_throttle: u32) -> Self {
        Self {
            parked_tlab: Default::default(),
            safepoint_skip: std::sync::atomic::AtomicU32::new(safepoint_throttle),
        }
    }

    /// Force the next safepoint check through its slow path.
    pub fn force_safepoint(&self) {
        self.safepoint_skip.store(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Address of a pinned context, valid under the registry lock until removal.
struct VmContextPtr(*const VmContext);

// SAFETY: registration requires a pinned, live context. Deregistration takes
// the registry lock before removing the entry or freeing its storage.
unsafe impl Send for VmContextPtr {}
unsafe impl Sync for VmContextPtr {}

fn registered_contexts(contexts: &[VmContextPtr]) -> impl Iterator<Item = &VmContext> {
    // SAFETY: callers borrow this slice from the locked registry. Each context
    // stays live for the borrow. Access to mutator-local state still follows
    // its own synchronization or park protocol.
    contexts.iter().map(|entry| unsafe { &*entry.0 })
}

impl ThreadState {
    /// Join under phase -> registry lock order. A new mutator cannot enter
    /// during an active scan, and joins the park count in Requested.
    ///
    /// # Safety
    /// The context belongs to this Core, is registered exactly once, and stays
    /// pinned and live until its Drop removes the entry.
    pub(crate) unsafe fn register_context(&self, context: Pin<&VmContext>) {
        let mut phase = self.gc_phase.lock();
        while matches!(*phase, GcPhase::Marking) { self.gc_phase_cv.wait(&mut phase); }
        self.vm_contexts.lock().push(VmContextPtr(std::ptr::from_ref(context.get_ref())));
    }

    pub(crate) fn deregister_context(
        &self,
        context: &VmContext,
        retained: &crate::diagnostics::RuntimeCounters,
    ) {
        {
            let mut registry = self.vm_contexts.lock();
            retained.absorb(&context.diagnostics.counters);
            registry.retain(|p| !std::ptr::eq(p.0, context));
        }
        // Avoid missed wakeups when removal lowers the required parked count.
        let _phase = self.gc_phase.lock();
        self.gc_phase_cv.notify_all();
    }

    pub(crate) fn registered_context_count(&self) -> usize {
        self.vm_contexts.lock().len()
    }

    /// Borrow each live context under the registry lock. The visitor must not
    /// reenter the registry. Mutator-local reads still require an owner or pause.
    pub(crate) fn visit_contexts(&self, visit: impl FnMut(&VmContext)) {
        let registry = self.vm_contexts.lock();
        registered_contexts(&registry).for_each(visit);
    }

    /// The nonblocking walk used by the crash reporter. Returns the number of
    /// contexts visited, or None when the lock is held.
    pub(crate) fn try_visit_contexts(
        &self, mut visit: impl FnMut(usize, &VmContext),
    ) -> Option<usize> {
        let registry = self.vm_contexts.try_lock()?;
        for (index, context) in registered_contexts(&registry).enumerate() {
            visit(index, context);
        }
        Some(registry.len())
    }

    /// Keep every shard alive during the callback. References cannot escape.
    pub(crate) fn with_counter_shards<R>(
        &self,
        inspect: impl for<'a> FnOnce(&mut dyn Iterator<Item = &'a crate::diagnostics::RuntimeCounters>) -> R,
    ) -> R {
        let registry = self.vm_contexts.lock();
        let mut shards = registered_contexts(&registry).map(|ctx| &ctx.diagnostics.counters);
        inspect(&mut shards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_context_ptr_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VmContextPtr>();
    }

    #[test]
    fn nonblocking_context_walk_skips_a_contended_registry() {
        let ctx = VmContext::new();
        let threads = ctx.core().threads();
        let registry = threads.vm_contexts.lock();
        assert_eq!(threads.try_visit_contexts(|_, _| panic!("visited a locked registry")), None);
        drop(registry);

        assert_eq!(threads.try_visit_contexts(|index, live| {
            assert_eq!(index, 0);
            assert!(std::ptr::eq(live, &*ctx));
        }), Some(1));
    }
}
