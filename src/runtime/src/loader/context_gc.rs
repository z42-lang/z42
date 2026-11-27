//! Loader-owned context reclamation after GC sweep.
use super::contexts::{ContextId, ContextLiveness, ContextRegistry};
use super::LoaderState;
use crate::gc::to_vm::{ContextReclaim, ContextReclaimHook, ContextReclaimSnapshot};
use crate::object::ScriptObject;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

impl LoaderState {
    /// Give the heap a reclamation service without extending the loader's lifetime.
    pub(crate) fn context_reclaimer(&self) -> ContextReclaimHook {
        Box::new(ContextReclaimer {
            registry: Arc::downgrade(&self.context_registry),
            unloading: self.context_registry.lock().unloading_flag(),
        })
    }
}

struct ContextReclaimer {
    registry: Weak<Mutex<ContextRegistry>>,
    unloading: Arc<AtomicUsize>,
}

impl ContextReclaim for ContextReclaimer {
    fn is_unloading(&self) -> bool {
        self.unloading.load(Ordering::Relaxed) > 0
    }
    fn snapshot(&self) -> Box<dyn ContextReclaimSnapshot> {
        let associations = self
            .registry
            .upgrade()
            .map(|registry| registry.lock().liveness_snapshot())
            .unwrap_or_default();
        Box::new(ContextSnapshot {
            registry: self.registry.clone(),
            associations,
            live: HashSet::new(),
        })
    }
}
struct ContextSnapshot {
    registry: Weak<Mutex<ContextRegistry>>,
    associations: ContextLiveness,
    live: HashSet<ContextId>,
}
impl ContextReclaimSnapshot for ContextSnapshot {
    fn is_empty(&self) -> bool {
        self.associations.is_empty()
    }
    fn observe_live_object(&mut self, object: &ScriptObject) {
        let ptr = std::ptr::from_ref(&*object.type_desc) as usize;
        if let Some(id) = self.associations.retained_context(ptr, &object.native()) {
            self.live.insert(id);
        }
    }
    fn reclaim(self: Box<Self>) {
        if let Some(registry) = self.registry.upgrade() {
            registry.lock().reclaim(&self.live);
        }
    }
}
