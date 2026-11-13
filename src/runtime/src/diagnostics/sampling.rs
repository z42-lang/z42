//! Light profiling contracts and a lazy per-VM provider; no analysis implementation.
use crate::{components::{BackendSource, VmComponents}, context::VmContext, gc::Heap};
use std::sync::OnceLock;

pub trait SampleBackend: Send + Sync {
    fn maybe_sample(&self, ctx: &VmContext);
    fn flush_folded(&self, path: &str) -> std::io::Result<()>;
    fn flush_trace(&self, path: &str) -> std::io::Result<()>;
}

/// Internal same-core registration contract. Heap callers own the GC pause and
/// root protocol; implementations must not run managed code while inspecting it.
pub trait ProfilerBackend: SampleBackend {
    fn write_heap_snapshot(&self, ctx: &VmContext, path: &str) -> anyhow::Result<i64>;
    fn direct_referrers(&self, heap: &Heap, target: usize) -> Vec<super::retention::RetainerInfo>;
    fn retaining_roots(&self, heap: &Heap, target: usize) -> Vec<super::retention::RootInfo>;
}

pub struct Sampling {
    enabled: bool,
    hz: Option<u32>,
    trace: bool,
    source: OnceLock<BackendSource>,
    initialized: OnceLock<Result<(), String>>,
    backend: OnceLock<Box<dyn ProfilerBackend>>,
    // Drop the provider before its instance/factory/code owners. Never store a
    // ComponentLease here: its Core reference would form a self-retention cycle.
    code: OnceLock<VmComponents>,
}
impl Sampling {
    pub fn with_config(hz: Option<u32>, trace: bool) -> Self {
        Self { enabled: hz.is_some(), hz, trace, source: OnceLock::new(),
            initialized: OnceLock::new(), backend: OnceLock::new(), code: OnceLock::new() }
    }
    pub fn set_source(&self, source: BackendSource) -> anyhow::Result<()> {
        anyhow::ensure!(self.initialized.get().is_none() && self.backend.get().is_none(), "profiler is already initialized");
        self.source.set(source).map_err(|_| anyhow::anyhow!("profiler source is already configured"))
    }
    /// Called only by a validated same-build component through the shared core.
    pub fn install(&self, backend: Box<dyn ProfilerBackend>) -> anyhow::Result<()> {
        self.backend.set(backend).map_err(|_| anyhow::anyhow!("profiler provider is already installed"))
    }
    pub(crate) fn retain(&self, components: VmComponents) -> anyhow::Result<()> {
        self.code.set(components).map_err(|_| anyhow::anyhow!("profiler code is already retained"))
    }
    pub fn provider(&self, ctx: &VmContext) -> anyhow::Result<&dyn ProfilerBackend> {
        let result = self.initialized.get_or_init(|| {
            if self.backend.get().is_some() { return Ok(()); }
            crate::components::profiling::attach(ctx, self.source.get_or_init(BackendSource::default), self.hz.unwrap_or(0), self.trace)
                .map_err(|e| format!("{e:#}"))
        });
        result.as_ref().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.backend.get().map(|b| b.as_ref())
            .ok_or_else(|| anyhow::anyhow!("profiler component did not install its provider"))
    }
    pub fn is_loaded(&self) -> bool {
        self.initialized.get().is_some_and(|result| result.is_ok()) && self.backend.get().is_some()
    }
    pub(crate) fn components(&self) -> Option<&VmComponents> {
        if self.is_loaded() { self.code.get() } else { None }
    }
    #[inline]
    pub fn enabled(&self) -> bool { self.enabled }
    #[inline]
    pub fn poll(&self, ctx: &VmContext) {
        // The host resolves enabled sampling before execution. Disabled polls do
        // not resolve a provider, take a clock, allocate, lock or dispatch.
        if self.enabled {
            if let Some(backend) = self.backend.get() { backend.maybe_sample(ctx); }
        }
    }
    pub fn flush_folded(&self, path: &str) -> std::io::Result<()> {
        self.backend.get().ok_or_else(|| std::io::Error::other("profiler is not initialized"))?.flush_folded(path)
    }
    pub fn flush_trace(&self, path: &str) -> std::io::Result<()> {
        self.backend.get().ok_or_else(|| std::io::Error::other("profiler is not initialized"))?.flush_trace(path)
    }
}
#[cfg(all(test, z42_test_facade))]
#[path = "sampling_tests.rs"]
mod tests;
