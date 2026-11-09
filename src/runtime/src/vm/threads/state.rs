//! Shared mutator coordination state for one VM.
use parking_lot::{Condvar, Mutex};
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize}};
use crate::vm::context::VmContextPtr;
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
    pub fn try_visit_contexts(&self, mut visit: impl FnMut(usize, *const super::super::context::VmContext)) -> Option<usize> {
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
