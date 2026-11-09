//! Sampling capability supplied by the runtime facade.
use crate::context::VmContext;

pub trait SampleBackend: Send + Sync {
    fn maybe_sample(&self, ctx: &VmContext);
    fn flush_folded(&self, path: &str) -> std::io::Result<()>;
    fn flush_trace(&self, path: &str) -> std::io::Result<()>;
}

/// The immutable gate is separate from the backend: disabled polls do not perform
/// a virtual call, read a clock, or acquire a backend lock.
pub struct Sampling {
    enabled: bool,
    backend: Box<dyn SampleBackend>,
}

impl Sampling {
    pub fn new(enabled: bool, backend: impl SampleBackend + 'static) -> Self {
        Self {
            enabled,
            backend: Box::new(backend),
        }
    }
    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    #[inline]
    pub fn poll(&self, ctx: &VmContext) {
        if self.enabled {
            self.backend.maybe_sample(ctx);
        }
    }
    pub fn flush_folded(&self, path: &str) -> std::io::Result<()> {
        self.backend.flush_folded(path)
    }
    pub fn flush_trace(&self, path: &str) -> std::io::Result<()> {
        self.backend.flush_trace(path)
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "sampling_tests.rs"]
mod tests;
