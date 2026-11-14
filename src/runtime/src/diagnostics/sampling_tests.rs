use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct Backend {
    polls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}
impl SampleBackend for Backend {
    fn maybe_sample(&self, _: &VmContext) {
        self.polls.fetch_add(1, Ordering::Relaxed);
    }
    fn flush_folded(&self, path: &str) -> std::io::Result<()> {
        Err(std::io::Error::other(format!("folded:{path}")))
    }
    fn flush_trace(&self, path: &str) -> std::io::Result<()> {
        Err(std::io::Error::other(format!("trace:{path}")))
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn gate_dispatch_errors_and_backend_lifetime() {
    let ctx = VmContext::new();
    for enabled in [false, true] {
        let polls = Arc::new(AtomicUsize::new(0));
        let drops = Arc::new(AtomicUsize::new(0));
        let sampling = Sampling::with_config(enabled.then_some(100), false);
        sampling.install(Box::new(Backend { polls: polls.clone(), drops: drops.clone() })).ok().unwrap();
        assert_eq!(sampling.enabled(), enabled);
        sampling.poll(&ctx);
        assert_eq!(polls.load(Ordering::Relaxed), usize::from(enabled));
        assert_eq!(
            sampling.flush_folded("out").unwrap_err().to_string(),
            "folded:out"
        );
        assert_eq!(
            sampling.flush_trace("out").unwrap_err().to_string(),
            "trace:out"
        );
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        drop(sampling);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
    }
}

impl ProfilerBackend for Backend {
    fn write_heap_snapshot(&self, _: &VmContext, _: &str) -> anyhow::Result<i64> { anyhow::bail!("test provider") }
    fn direct_referrers(&self, _: &Heap, _: usize) -> Vec<super::super::retention::RetainerInfo> { Vec::new() }
    fn retaining_roots(&self, _: &Heap, _: usize) -> Vec<super::super::retention::RootInfo> { Vec::new() }
}
