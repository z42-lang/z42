//! VM observation state, runtime counters and event subscriptions.
//!
//! ```
//! use z42::diagnostics::{RuntimeCounters, CounterSnapshot, RuntimeEvent, RuntimeObserver};
//! let _: CounterSnapshot = RuntimeCounters::new().snapshot();
//! ```
//! ```
//! use std::sync::Arc;
//! use z42::{diagnostics::{add_runtime_observer, RuntimeObserver}, vm::VmContext};
//! fn subscribe(ctx: &VmContext, observer: Arc<dyn RuntimeObserver>) {
//!     add_runtime_observer(ctx, observer);
//! }
//! ```
//! ```compile_fail
//! fn registry(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.core().diagnostics().runtime_observers;
//! }
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
pub(crate) use crash_report::{install as install_crash_report, live_cores_snapshot, register_crash_core};
mod exit_report;
pub(crate) use exit_report::{configure_exit_report, report_on_exit};
mod observer;
pub(crate) use observer::ModuleLoadEvents;
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
    runtime_observers: RuntimeObserverRegistry,
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
    pub(crate) fn new(pause_window: usize, sampler: sampling::Sampling) -> Self {
        Self {
            counters: Arc::new(RuntimeCounters::new()),
            runtime_observers: RuntimeObserverRegistry::new(),
            park_histogram: Mutex::new(crate::gc::types::PauseHistogram::new(pause_window)),
            lock_contentions: AtomicU64::new(0),
            lock_wait_us: AtomicU64::new(0),
            sampler,
        }
    }

    #[inline]
    pub(crate) fn module_loads(&self) -> ModuleLoadEvents<'_> {
        self.runtime_observers.module_loads()
    }

    /// Exact totals across living and departed contexts; base is read under the
    /// same registry lock used to transfer a departing shard into it.
    pub fn snapshot(&self, threads: &crate::threads::ThreadState) -> CounterSnapshot {
        threads.with_counter_shards(|shards| {
            shards.fold(self.counters.snapshot(), |total, shard| total.add(shard.snapshot()))
        })
    }
}

/// Subscribe to subsequent events from this VM and its workers.
pub fn add_runtime_observer(ctx: &VmContext, observer: Arc<dyn RuntimeObserver>) {
    ctx.core().diagnostics().runtime_observers.add(observer);
}

/// Publish an event. The payload is only built when the VM has subscribers.
#[inline]
pub fn emit_runtime_event(ctx: &VmContext, build: impl FnOnce() -> RuntimeEvent) {
    ctx.core().diagnostics().runtime_observers.fire_with(build);
}

/// Report a managed throw, limiting the UTF-8 message prefix read by observers.
pub fn emit_exception_thrown(ctx: &VmContext, describe: impl FnOnce() -> (String, String)) {
    emit_runtime_event(ctx, || {
        let (class, mut message) = describe();
        if message.len() > 256 {
            message.truncate(message.floor_char_boundary(256));
            message.push('…');
        }
        RuntimeEvent::ExceptionThrown { class, message }
    });
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

/// Check whether this VM already has a profiler installed.
pub fn profiler_loaded(ctx: &VmContext) -> bool {
    ctx.core().diagnostics().sampler.is_loaded()
}

fn profiler(ctx: &VmContext) -> anyhow::Result<&dyn sampling::ProfilerBackend> {
    ctx.core().diagnostics().sampler.provider(ctx)
}

/// Resolve the configured profiler, validating its build and Core identity.
pub fn prepare_profiler(ctx: &VmContext) -> anyhow::Result<()> {
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

/// Write a heap snapshot with the configured profiler and return its byte size.
pub fn write_heap_snapshot(ctx: &VmContext, path: &str) -> anyhow::Result<i64> {
    try_write_heap_snapshot(ctx, path).map_err(|error| match error {
        HeapSnapshotError::Unavailable(cause) | HeapSnapshotError::Write(cause) => cause,
    })
}

/// Keep the failure stage available to callers that map it to managed errors.
pub(crate) fn try_write_heap_snapshot(ctx: &VmContext, path: &str) -> Result<i64, HeapSnapshotError> {
    profiler(ctx)
        .map_err(HeapSnapshotError::Unavailable)?
        .write_heap_snapshot(ctx, path)
        .map_err(HeapSnapshotError::Write)
}
