//! VM observation state, runtime counters and event subscriptions.
pub mod counters;
pub mod observer;
pub(crate) mod sampling;

use std::sync::{Arc, atomic::AtomicU64};
use parking_lot::Mutex;

/// VM-wide observation state. Context counters are transferred here under the
/// thread registry lock before their context is removed from that registry.
/// Fields retain their own synchronization; the aggregate adds no lock.
pub struct DiagnosticsState {
    pub counters: Arc<crate::vm::diagnostics::counters::RuntimeCounters>,
    pub runtime_observers: crate::vm::diagnostics::observer::RuntimeObserverRegistry,
    pub(crate) park_histogram: Mutex<crate::gc::types::PauseHistogram>,
    pub(crate) lock_contentions: AtomicU64,
    pub(crate) lock_wait_us: AtomicU64,
    pub(crate) sampler: sampling::Sampling,
}

/// Observation-only shard; transferred under the registry lock before deregistration.
#[derive(Default)]
pub(crate) struct ThreadDiagnosticsState {
    pub(crate) counters: crate::vm::diagnostics::counters::RuntimeCounters,
}
