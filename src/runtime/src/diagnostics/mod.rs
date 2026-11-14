//! VM observation state, runtime counters and event subscriptions.
//!
//! ```
//! use z42::diagnostics::{RuntimeCounters, CounterSnapshot, RuntimeEvent, RuntimeObserver};
//! let _: CounterSnapshot = RuntimeCounters::new().snapshot();
//! ```
//! ```compile_fail
//! use z42::diagnostics::counters::RuntimeCounters;
//! ```
//! ```compile_fail
//! use z42::diagnostics::observer::RuntimeEvent;
//! ```
mod counters;
pub use counters::{RuntimeCounters, Snapshot as CounterSnapshot, ProfileSnapshot};
mod observer;
pub use observer::{RuntimeEvent, RuntimeObserver, RuntimeObserverRegistry};
pub mod sampling;
pub mod retention;

use std::sync::{Arc, atomic::AtomicU64};
use parking_lot::Mutex;

/// VM-wide observation state. Context counters are transferred here under the
/// thread registry lock before their context is removed from that registry.
/// Fields retain their own synchronization; the aggregate adds no lock.
pub struct DiagnosticsState {
    pub counters: Arc<crate::diagnostics::RuntimeCounters>,
    pub runtime_observers: crate::diagnostics::RuntimeObserverRegistry,
    pub park_histogram: Mutex<crate::gc::types::PauseHistogram>,
    pub lock_contentions: AtomicU64,
    pub lock_wait_us: AtomicU64,
    pub sampler: sampling::Sampling,
}

/// Observation-only shard; transferred under the registry lock before deregistration.
#[derive(Default)]
pub struct ThreadDiagnosticsState {
    pub counters: crate::diagnostics::RuntimeCounters,
}

pub(crate) mod contention;

impl DiagnosticsState {
    /// Exact totals across living and departed contexts; base is read under the
    /// same registry lock used to transfer a departing shard into it.
    pub fn snapshot(&self, threads: &crate::threads::ThreadState) -> CounterSnapshot {
        threads.with_counter_shards(|shards| {
            shards.fold(self.counters.snapshot(), |total, shard| total.add(shard.snapshot()))
        })
    }
}
