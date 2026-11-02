use super::*;
use crate::gc::to_vm::{ContextReclaim, ContextReclaimSnapshot};
use std::sync::{Mutex as StdMutex, Weak};

struct Reclaimer {
    heap: Weak<ArcMagrGC>,
    dead: crate::gc::WeakRef,
    events: Arc<StdMutex<Vec<&'static str>>>,
}
struct Snapshot {
    heap: Weak<ArcMagrGC>,
    dead: crate::gc::WeakRef,
    events: Arc<StdMutex<Vec<&'static str>>>,
}
impl ContextReclaim for Reclaimer {
    fn is_unloading(&self) -> bool {
        true
    }
    fn snapshot(&self) -> Box<dyn ContextReclaimSnapshot> {
        self.events.lock().unwrap().push("snapshot");
        Box::new(Snapshot {
            heap: self.heap.clone(),
            dead: self.dead.clone(),
            events: self.events.clone(),
        })
    }
}
impl ContextReclaimSnapshot for Snapshot {
    fn is_empty(&self) -> bool {
        false
    }
    fn observe_live_object(&mut self, object: &crate::object::ScriptObject) {
        assert_eq!(
            &*object.type_desc.name, "Live",
            "unmarked garbage must not be observed"
        );
        self.events.lock().unwrap().push("observe");
    }
    fn reclaim(self: Box<Self>) {
        let heap = self.heap.upgrade().unwrap();
        assert!(
            heap.upgrade_weak(&self.dead).is_none(),
            "sweep must finish before reclamation"
        );
        self.events.lock().unwrap().push("reclaim");
    }
}

fn run(incremental: bool) {
    let heap = Arc::new(ArcMagrGC::default());
    let td = dummy_type_desc("Live");
    let live: Vec<_> = (0..100)
        .map(|_| heap.alloc_object(td.clone(), vec![], NativeData::None))
        .collect();
    let _pin = heap.pin_root(heap.alloc_array(live));
    let dead = heap.alloc_object(dummy_type_desc("Dead"), vec![], NativeData::None);
    let events = Arc::new(StdMutex::new(Vec::new()));
    heap.set_context_reclaimer(Box::new(Reclaimer {
        heap: Arc::downgrade(&heap),
        dead: heap.make_weak(&dead).unwrap(),
        events: events.clone(),
    }));
    if incremental {
        let mut slices = 0;
        loop {
            slices += 1;
            let done = heap.run_major_slice_for_test(1);
            if done {
                break;
            }
            assert!(!events.lock().unwrap().contains(&"reclaim"));
            assert!(slices < 1024, "cycle must finish");
        }
        assert!(slices > 1, "exercise a snapshot retained across slices");
    } else {
        heap.collect_full();
    }
    let events = events.lock().unwrap();
    assert_eq!(events.first(), Some(&"snapshot"));
    assert_eq!(events.last(), Some(&"reclaim"));
    assert_eq!(events.iter().filter(|&&e| e == "observe").count(), 100);
    assert_eq!(events.iter().filter(|&&e| e == "snapshot").count(), 1);
    assert_eq!(events.iter().filter(|&&e| e == "reclaim").count(), 1);
}

#[test]
fn full_collection_observes_marked_objects_before_sweep_and_reclaims_after() {
    run(false);
}

#[test]
fn incremental_collection_keeps_its_associations_until_sweep_finishes() {
    run(true);
}
