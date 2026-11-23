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
mod crash_report;
pub(crate) use crash_report::install as install_crash_report;
mod exit_report;
pub(crate) use exit_report::{configure_exit_report, report_on_exit};
mod observer;
pub use observer::{RuntimeEvent, RuntimeObserver, RuntimeObserverRegistry};
pub mod sampling;
pub mod retention;

use std::sync::{Arc, atomic::AtomicU64};
use parking_lot::Mutex;
use crate::vm::VmContext;

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

pub(crate) fn profiler_loaded(ctx: &VmContext) -> bool {
    ctx.core().diagnostics().sampler.is_loaded()
}

pub(crate) fn prepare_profiler(ctx: &VmContext) -> anyhow::Result<()> {
    ctx.core().diagnostics().sampler.provider(ctx).map(|_| ())
}

pub(crate) fn write_heap_snapshot(ctx: &VmContext, path: &str) -> anyhow::Result<i64> {
    ctx.core().diagnostics().sampler.provider(ctx)?.write_heap_snapshot(ctx, path)
}
