use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

type SnapshotCapture = Box<dyn FnOnce(PausedHeap<'_>) -> HeapSnapshotWriter + Send>;

struct Backend {
    polls: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    snapshot: parking_lot::Mutex<Option<SnapshotCapture>>,
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
        let sampling = Sampling::with_config(enabled.then_some(100), false, |_, _, _| unreachable!("installed backend needs no preparation"));
        sampling.install(Box::new(Backend {
            polls: polls.clone(),
            drops: drops.clone(),
            snapshot: Default::default(),
        })).ok().unwrap();
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
    fn capture_heap_snapshot(&self, heap: PausedHeap<'_>) -> HeapSnapshotWriter {
        let capture = self.snapshot.lock().take().expect("test snapshot callback");
        capture(heap)
    }
    fn direct_referrers(&self, _: PausedHeap<'_>, _: usize) -> Vec<super::super::retention::RetainerInfo> { Vec::new() }
    fn retaining_roots(&self, _: PausedHeap<'_>, _: usize) -> Vec<super::super::retention::RootInfo> { Vec::new() }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn snapshot_captures_worker_allocations_and_allows_gc_during_write() {
    use crate::gc::ambient::HeapGuard;
    use crate::object::Value;
    use crate::threads::safepoint::{request_gc_pause, NativeParkGuard};
    use std::{sync::mpsc, time::Duration};

    const TIMEOUT: Duration = Duration::from_secs(10);
    let ctx = VmContext::new();
    std::thread::scope(|scope| {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (collect_tx, collect_rx) = mpsc::channel();
        let (collected_tx, collected_rx) = mpsc::channel();
        let core = ctx.core_arc();
        scope.spawn(move || {
            let worker = VmContext::new_with_core(core);
            let _ambient = HeapGuard::enter(worker.heap());
            let value = worker.heap().alloc_array(vec![Value::I64(4312)]);
            let weak = worker.heap().make_weak(&value).unwrap();
            let park = NativeParkGuard::enter(&worker);
            ready_tx.send(()).unwrap();
            if collect_rx.recv_timeout(TIMEOUT).is_err() {
                return;
            }
            drop(park);
            let pause = request_gc_pause(&worker).unwrap();
            worker.heap().collect_full();
            assert!(worker.heap().upgrade_weak(&weak).is_none());
            drop(pause);
            collected_tx.send(()).unwrap();
        });

        ready_rx.recv_timeout(TIMEOUT).unwrap();
        let inspected_core = Arc::downgrade(ctx.core());
        let capture: SnapshotCapture = Box::new(move |heap| {
            assert!(inspected_core.upgrade().unwrap().threads().collector_is_active());
            let mut observed = None;
            heap.live_values(&mut |value| {
                if let Value::Array(array) = value {
                    if let Some(Value::I64(4312)) = array.borrow().get(0) {
                        observed = Some(4312i64);
                    }
                }
            });
            Box::new(move |_| {
                assert!(!inspected_core.upgrade().unwrap().threads().collector_is_active());
                collect_tx.send(()).unwrap();
                collected_rx.recv_timeout(TIMEOUT).unwrap();
                Ok(observed.expect("snapshot must see the parked worker's array"))
            })
        });
        crate::diagnostics::install_profiler_backend(&ctx, Box::new(Backend {
            polls: Default::default(),
            drops: Default::default(),
            snapshot: parking_lot::Mutex::new(Some(capture)),
        })).unwrap();
        assert_eq!(crate::diagnostics::write_heap_snapshot(&ctx, "test").unwrap(), 4312);
    });
}
