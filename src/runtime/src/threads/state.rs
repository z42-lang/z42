//! Shared mutator coordination state for one VM.
use parking_lot::{Condvar, Mutex};
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize}};
use crate::vm::VmContext;
use super::safepoint::GcPhase;

pub struct ThreadState {
    pub safepoint_throttle: u32,
    /// Fatal VM errors propagate to all contexts sharing this core.
    /// Unwinding bypasses z42 handlers and cannot become a catchable z42 exception.
    pub fatal: AtomicBool,
    /// Registered contexts remain alive while this lock is held by the root scanner.
    pub(crate) vm_contexts: Mutex<Vec<VmContextPtr>>,
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
    /// Visit registered addresses while holding the registry lock, without blocking.
    /// Addresses are valid only during the callback; inspecting thread-local data
    /// still requires the owner/parked protocol. The registry cannot be mutated.
    pub fn try_visit_contexts(&self, mut visit: impl FnMut(usize, *const super::super::vm::VmContext)) -> Option<usize> {
        let contexts = self.vm_contexts.try_lock()?;
        for (index, context) in contexts.iter().enumerate() { visit(index, context.0); }
        Some(contexts.len())
    }
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
}

/// Type-erased pointer to a registered [`VmContext`].
///
/// Stored in `VmCore.threads.vm_contexts` so the GC scanner can walk every thread's
/// per-thread state (call stack / pending exception / func-ref slots) during
/// mark phase. Without this registry, the scanner could only see the first
/// VmContext it was given a clone of — multi-threading would silently miss
/// roots and free live cross-thread objects.
///
/// # Safety
///
/// - The pointer is registered by [`VmContext::new`] AFTER `Pin<Box<...>>`
///   wrapping guarantees address stability for the entire lifetime of the
///   VmContext (the Box's heap allocation address is stable, and Pin prevents
///   `mem::swap` / move-out).
/// - It is deregistered by [`VmContext::drop`] BEFORE the Box is freed
///   (`retain` runs in Drop, prior to memory deallocation), so any
///   dereference performed while the entry is in `vm_contexts` is on a live
///   VmContext.
/// - Cross-thread access: the per-thread fields on VmContext are
///   `Arc<Mutex<...>>` / atomics (Send + Sync), so reading them from another
///   thread under registry-lock-then-deref discipline is sound. The one
///   exception is `call_stack`, readable cross-thread only while its owner is
///   parked (`crate::exec::ThreadExecState::scan_frames_parked`).
pub struct VmContextPtr(pub(crate) *const VmContext);

// SAFETY: see SAFETY block on `VmContextPtr` above — the raw pointer is
// kept alive by the Box/Pin ownership of the registering thread, registry
// updates happen under `vm_contexts: Mutex<...>`, and dereferenced fields
// are themselves Send + Sync.
unsafe impl Send for VmContextPtr {}
unsafe impl Sync for VmContextPtr {}


impl ThreadState {
    /// Join under phase -> registry lock order; an unregistered mutator cannot
    /// enter an active scan. Requested still admits it into the next park count.
    pub(crate) fn register_context(&self, ptr: VmContextPtr) {
        let mut phase = self.gc_phase.lock();
        while matches!(*phase, GcPhase::Marking) { self.gc_phase_cv.wait(&mut phase); }
        self.vm_contexts.lock().push(ptr);
    }

    pub(crate) fn deregister_context(
        &self, ptr: *const VmContext,
        retained: &crate::diagnostics::RuntimeCounters,
        departing: &crate::diagnostics::RuntimeCounters,
    ) {
        {
            let mut registry = self.vm_contexts.lock();
            retained.absorb(departing);
            registry.retain(|p| p.0 != ptr);
        }
        // Avoid missed wakeups when removal lowers the required parked count.
        let _phase = self.gc_phase.lock();
        self.gc_phase_cv.notify_all();
    }

    /// Keep every shard alive during the callback. References cannot escape.
    pub(crate) fn with_counter_shards<R>(
        &self,
        inspect: impl for<'a> FnOnce(&mut dyn Iterator<Item = &'a crate::diagnostics::RuntimeCounters>) -> R,
    ) -> R {
        let registry = self.vm_contexts.lock();
        // SAFETY: deregistration and deallocation wait for this registry lock.
        let mut shards = registry.iter().map(|p| &unsafe { &*p.0 }.diagnostics.counters);
        inspect(&mut shards)
    }
}

impl ThreadLocalState {
    /// Force the next safepoint check through its slow path.
    pub fn force_safepoint(&self) {
        self.safepoint_skip.store(1, std::sync::atomic::Ordering::Relaxed);
    }
}
