//! Light profiling contracts and a lazy per-VM provider; no analysis implementation.
use crate::{gc::walk::PausedHeap, vm::VmContext};
use std::sync::OnceLock;

pub trait SampleBackend: Send + Sync {
    fn maybe_sample(&self, ctx: &VmContext);
    fn flush_folded(&self, path: &str) -> std::io::Result<()>;
    fn flush_trace(&self, path: &str) -> std::io::Result<()>;
}

/// Writes a captured graph using data owned independently of the VM pause.
pub type HeapSnapshotWriter = Box<dyn FnOnce(&str) -> anyhow::Result<i64>>;

/// Internal same-core registration contract. Retention queries receive a
/// collected heap while its owning VM is paused.
pub trait ProfilerBackend: SampleBackend {
    /// Capture the graph while paused. The writer runs after the VM resumes.
    fn capture_heap_snapshot(&self, heap: PausedHeap<'_>) -> HeapSnapshotWriter;
    fn direct_referrers(&self, heap: PausedHeap<'_>, target: usize) -> Vec<super::retention::RetainerInfo>;
    fn retaining_roots(&self, heap: PausedHeap<'_>, target: usize) -> Vec<super::retention::RootInfo>;
}

pub struct Sampling {
    enabled: bool,
    hz: Option<u32>,
    trace: bool,
    prepare: fn(&VmContext, u32, bool) -> anyhow::Result<()>,
    initialized: OnceLock<Result<(), String>>,
    backend: OnceLock<Box<dyn ProfilerBackend>>,

}
impl Sampling {
    pub fn with_config(hz: Option<u32>, trace: bool, prepare: fn(&VmContext, u32, bool) -> anyhow::Result<()>) -> Self {
        Self { enabled: hz.is_some(), hz, trace, prepare,
            initialized: OnceLock::new(), backend: OnceLock::new() }
    }
    /// Called only by a validated same-build component through the shared core.
    pub(super) fn install(&self, backend: Box<dyn ProfilerBackend>) -> anyhow::Result<()> {
        self.backend.set(backend).map_err(|_| anyhow::anyhow!("profiler provider is already installed"))
    }
    pub(super) fn provider(&self, ctx: &VmContext) -> anyhow::Result<&dyn ProfilerBackend> {
        let result = self.initialized.get_or_init(|| {
            if self.backend.get().is_some() { return Ok(()); }
            (self.prepare)(ctx, self.hz.unwrap_or(0), self.trace)
                .map_err(|e| format!("{e:#}"))
        });
        result.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.backend.get().map(|b| b.as_ref())
            .ok_or_else(|| anyhow::anyhow!("profiler component did not install its provider"))
    }
    pub(super) fn is_loaded(&self) -> bool {
        self.initialized.get().is_some_and(|result| result.is_ok()) && self.backend.get().is_some()
    }
    #[inline]
    pub(super) fn enabled(&self) -> bool { self.enabled }
    #[inline]
    pub(super) fn poll(&self, ctx: &VmContext) {
        // The host resolves enabled sampling before execution. Disabled polls do
        // not resolve a provider, take a clock, allocate, lock or dispatch.
        if self.enabled {
            if let Some(backend) = self.backend.get() { backend.maybe_sample(ctx); }
        }
    }
    pub(super) fn flush_folded(&self, path: &str) -> std::io::Result<()> {
        self.backend.get().ok_or_else(|| std::io::Error::other("profiler is not initialized"))?.flush_folded(path)
    }
    pub(super) fn flush_trace(&self, path: &str) -> std::io::Result<()> {
        self.backend.get().ok_or_else(|| std::io::Error::other("profiler is not initialized"))?.flush_trace(path)
    }
}
#[cfg(all(test, z42_test_facade))]
#[path = "sampling_tests.rs"]
mod tests;
