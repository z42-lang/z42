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
pub(crate) mod sampling;
pub use sampling::Sampling;
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
    pub(crate) sampler: Sampling,
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

/// Sample the counters and heap statistics shared by CLI and managed diagnostics.
pub(crate) fn capture_counters(ctx: &VmContext) -> (CounterSnapshot, crate::gc::HeapStats) {
    let core = ctx.core();
    (core.diagnostics().snapshot(core.threads()), ctx.heap().stats())
}

pub(crate) fn install_profiler_backend(
    ctx: &VmContext,
    backend: Box<dyn sampling::ProfilerBackend>,
) -> anyhow::Result<()> {
    ctx.core().diagnostics().sampler.install(backend)
}

pub(crate) fn profiler_loaded(ctx: &VmContext) -> bool {
    ctx.core().diagnostics().sampler.is_loaded()
}

fn profiler(ctx: &VmContext) -> anyhow::Result<&dyn sampling::ProfilerBackend> {
    ctx.core().diagnostics().sampler.provider(ctx)
}

pub(crate) fn prepare_profiler(ctx: &VmContext) -> anyhow::Result<()> {
    profiler(ctx).map(|_| ())
}

/// Resolve an enabled sampler before the VM is published.
pub(crate) fn prepare_sampling(ctx: &VmContext) -> anyhow::Result<()> {
    let sampler = &ctx.core().diagnostics().sampler;
    if sampler.enabled() {
        sampler.provider(ctx)?;
    }
    Ok(())
}

#[inline]
pub(crate) fn poll_sampling(ctx: &VmContext) {
    ctx.core().diagnostics().sampler.poll(ctx);
}

pub(crate) enum HeapSnapshotError {
    Unavailable(anyhow::Error),
    Write(anyhow::Error),
}

impl HeapSnapshotError {
    pub(crate) fn into_diagnostic(self) -> anyhow::Error {
        match self {
            Self::Unavailable(error) | Self::Write(error) => error,
        }
    }
}

pub(crate) fn write_heap_snapshot(ctx: &VmContext, path: &str) -> Result<i64, HeapSnapshotError> {
    profiler(ctx)
        .map_err(HeapSnapshotError::Unavailable)?
        .write_heap_snapshot(ctx, path)
        .map_err(HeapSnapshotError::Write)
}
