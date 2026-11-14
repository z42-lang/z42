//! VM observation state, runtime counters and event subscriptions.
pub mod counters;
pub mod observer;
pub mod sampling;
pub mod retention;

use std::sync::{Arc, atomic::AtomicU64};
use parking_lot::Mutex;

/// VM-wide observation state. Context counters are transferred here under the
/// thread registry lock before their context is removed from that registry.
/// Fields retain their own synchronization; the aggregate adds no lock.
pub struct DiagnosticsState {
    pub counters: Arc<crate::diagnostics::counters::RuntimeCounters>,
    pub runtime_observers: crate::diagnostics::observer::RuntimeObserverRegistry,
    pub park_histogram: Mutex<crate::gc::types::PauseHistogram>,
    pub lock_contentions: AtomicU64,
    pub lock_wait_us: AtomicU64,
    pub sampler: sampling::Sampling,
}

/// Observation-only shard; transferred under the registry lock before deregistration.
#[derive(Default)]
pub struct ThreadDiagnosticsState {
    pub counters: crate::diagnostics::counters::RuntimeCounters,
}

pub(crate) mod contention;
