//! Loading-context reclamation adapter. GC only holds an opaque cycle snapshot.
use super::contexts::{ContextId, ContextLiveness};
use crate::gc::to_vm::{ContextReclaim, ContextReclaimSnapshot};
use crate::object::ScriptObject;
use crate::vm::context::VmCore;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

pub(crate) struct CoreContextReclaimer {
    pub(crate) core: Weak<VmCore>,
    pub(crate) unloading: Arc<AtomicUsize>,
}
impl ContextReclaim for CoreContextReclaimer {
    fn is_unloading(&self) -> bool {
        self.unloading.load(Ordering::Relaxed) > 0
    }
    fn snapshot(&self) -> Box<dyn ContextReclaimSnapshot> {
        let associations = self
            .core
            .upgrade()
            .map(|c| c.loader.context_registry.lock().liveness_snapshot())
            .unwrap_or_default();
        Box::new(ContextSnapshot {
            core: self.core.clone(),
            associations,
            live: HashSet::new(),
        })
    }
}
struct ContextSnapshot {
    core: Weak<VmCore>,
    associations: ContextLiveness,
    live: HashSet<ContextId>,
}
impl ContextReclaimSnapshot for ContextSnapshot {
    fn is_empty(&self) -> bool {
        self.associations.is_empty()
    }
    fn observe_live_object(&mut self, object: &ScriptObject) {
        let ptr = Arc::as_ptr(&object.type_desc) as usize;
        if let Some(id) = self.associations.retained_context(ptr, &object.native()) {
            self.live.insert(id);
        }
    }
    fn reclaim(self: Box<Self>) {
        if let Some(core) = self.core.upgrade() {
            core.loader.context_registry.lock().reclaim(&self.live);
        }
    }
}
