//! Sampling capability supplied by the runtime facade.
use crate::vm::context::VmContext;

pub(crate) trait SampleBackend: Send + Sync {
    fn maybe_sample(&self, ctx: &VmContext);
    fn flush_folded(&self, path: &str) -> std::io::Result<()>;
    fn flush_trace(&self, path: &str) -> std::io::Result<()>;
}

/// The immutable gate is separate from the backend: disabled polls do not perform
/// a virtual call, read a clock, or acquire a backend lock.
pub(crate) struct Sampling {
    enabled: bool,
    backend: Box<dyn SampleBackend>,
}

impl Sampling {
    pub(crate) fn new(enabled: bool, backend: impl SampleBackend + 'static) -> Self {
        Self {
            enabled,
            backend: Box::new(backend),
        }
    }
    #[inline]
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
    #[inline]
    pub(crate) fn poll(&self, ctx: &VmContext) {
        if self.enabled {
            self.backend.maybe_sample(ctx);
        }
    }
    pub(crate) fn flush_folded(&self, path: &str) -> std::io::Result<()> {
        self.backend.flush_folded(path)
    }
    pub(crate) fn flush_trace(&self, path: &str) -> std::io::Result<()> {
        self.backend.flush_trace(path)
    }
}

#[cfg(test)]
#[path = "sampling_tests.rs"]
mod tests;
