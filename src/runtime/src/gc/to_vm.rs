//! Services supplied by the VM. GC knows roots and pause lifetimes, not VM state.

use crate::object::{ScriptObject, Value};

/// Root access is only valid while all owners are suspended, or on a standalone heap.
pub trait GcRoots {
    /// # Safety
    /// Every mutator that owns these roots must be suspended throughout the visit.
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value));
}

impl<F: Fn(&mut dyn FnMut(&Value))> GcRoots for F {
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value)) {
        self(visit);
    }
}

pub type ExternalRootScanner = Box<dyn GcRoots + Send + Sync>;

/// Dropping the token resumes mutators. The token cannot outlive its VM adapter.
pub trait GcPause {}

/// The collecting mutator supplies the pause service for this collection only.
/// # Safety
/// A successful pause suspends all other mutators until the token is dropped.
pub unsafe trait GcToVm: GcRoots {
    /// None means another collector owns the pause; no collection may run.
    fn suspend_all(&self) -> Option<Box<dyn GcPause + '_>>;
}

/// VM-owned association snapshot, retained across incremental sweep slices.
pub trait ContextReclaimSnapshot: Send {
    fn is_empty(&self) -> bool;
    fn observe_live_object(&mut self, object: &ScriptObject);
    /// Called only after sweep finishes, while the world is still suspended.
    fn reclaim(self: Box<Self>);
}

pub trait ContextReclaim: Send + Sync {
    fn is_unloading(&self) -> bool;
    fn snapshot(&self) -> Box<dyn ContextReclaimSnapshot>;
}

pub type ContextReclaimHook = Box<dyn ContextReclaim>;

#[cfg(test)]
#[path = "to_vm_tests.rs"]
mod to_vm_tests;
