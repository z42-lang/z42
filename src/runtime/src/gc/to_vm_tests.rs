use super::*;
use crate::gc::{ArcMagrGC, GcConfig, GcMode, MagrGC};
use std::sync::{Arc, Mutex};

struct TestVm {
    allow: bool,
    root: Value,
    events: Arc<Mutex<Vec<&'static str>>>,
}
impl GcRoots for TestVm {
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value)) {
        self.events.lock().unwrap().push("roots");
        visit(&self.root);
    }
}
struct Pause(Arc<Mutex<Vec<&'static str>>>);
impl GcPause for Pause {}
impl Drop for Pause {
    fn drop(&mut self) {
        self.0.lock().unwrap().push("resume");
    }
}
// SAFETY: this standalone test has no concurrent mutators.
unsafe impl GcToVm for TestVm {
    fn suspend_all(&self) -> Option<Box<dyn GcPause + '_>> {
        self.events
            .lock()
            .unwrap()
            .push(if self.allow { "pause" } else { "denied" });
        self.allow
            .then(|| Box::new(Pause(self.events.clone())) as Box<dyn GcPause>)
    }
}

fn run(allow: bool) {
    let heap = ArcMagrGC::new(GcConfig {
        mode: GcMode::StwMarkSweep,
        ..GcConfig::default()
    });
    let root = heap.alloc_array(vec![Value::I64(7)]);
    let events = Arc::new(Mutex::new(Vec::new()));
    let vm = Arc::new(TestVm {
        allow,
        root,
        events: events.clone(),
    });
    let weak_vm = Arc::downgrade(&vm);
    let provider = weak_vm.clone();
    heap.set_external_root_scanner(Box::new(move |visit: &mut dyn FnMut(&Value)| {
        if let Some(vm) = provider.upgrade() {
            unsafe {
                vm.visit_roots(visit);
            }
        }
    }));
    heap.collect_cycles_with_vm(vm.as_ref());
    assert_eq!(heap.stats().gc_cycles, u64::from(allow));
    assert!(matches!(vm.root, Value::Array(_)));
    if allow {
        let Value::Array(array) = vm.root else {
            unreachable!()
        };
        assert_eq!(array.borrow().len(), 1);
        assert_eq!(*events.lock().unwrap(), vec!["pause", "roots", "resume"]);
    } else {
        assert_eq!(*events.lock().unwrap(), vec!["denied"]);
    }
    drop(vm);
    assert!(
        weak_vm.upgrade().is_none(),
        "persistent provider must not retain the VM"
    );
}

#[test]
fn admitted_pause_scans_roots_before_raii_resume() {
    run(true);
}

#[test]
fn rejected_pause_does_not_scan_or_collect() {
    run(false);
}

#[test]
fn panic_in_root_provider_still_drops_the_pause_token() {
    let heap = ArcMagrGC::new(GcConfig {
        mode: GcMode::StwMarkSweep,
        ..GcConfig::default()
    });
    let events = Arc::new(Mutex::new(Vec::new()));
    let vm = TestVm {
        allow: true,
        root: Value::Null,
        events: events.clone(),
    };
    heap.set_external_root_scanner(Box::new(|_: &mut dyn FnMut(&Value)| panic!("root failure")));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        heap.collect_cycles_with_vm(&vm);
    }));
    assert!(result.is_err());
    assert_eq!(*events.lock().unwrap(), vec!["pause", "resume"]);
}
