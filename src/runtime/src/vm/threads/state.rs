//! Shared mutator coordination state for one VM.
use parking_lot::{Condvar, Mutex};
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize}};
use crate::vm_context::VmContextPtr;
use super::safepoint::GcPhase;

pub(crate) struct ThreadState {
    /// Fatal VM errors propagate to all contexts sharing this core.
    /// Unwinding bypasses z42 handlers and cannot become a catchable z42 exception.
    pub(crate) fatal: AtomicBool,
    /// Registered contexts remain alive while this lock is held by the root scanner.
    pub(crate) vm_contexts: Mutex<Vec<VmContextPtr>>,
    /// Registration and collector transitions follow the same phase-lock protocol.
    pub(crate) gc_phase: Mutex<GcPhase>,
    pub(crate) gc_phase_cv: Condvar,
    /// Parked mutators, excluding the active collector.
    pub(crate) parked_count: AtomicUsize,
    /// CAS winner is the only collector until the pause guard releases the claim.
    pub(crate) collector_active: AtomicBool,
    /// Heap pressure publishes a sticky request; only a successful collector clears it.
    pub(crate) needs_auto_collect: Arc<AtomicBool>,
}

impl ThreadState {
    pub(crate) fn new() -> Self {
        Self {
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
