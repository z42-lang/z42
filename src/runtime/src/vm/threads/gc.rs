//! Pause adaptation for the collecting mutator. The guard borrows its context.
use super::safepoint::{request_gc_pause, GcPauseGuard};
use crate::gc::to_vm::{GcPause, GcToVm};
use crate::vm::context::VmContext;

impl GcPause for GcPauseGuard<'_> {}
// SAFETY: the existing pause guard coordinates every registered mutator.
unsafe impl GcToVm for VmContext {
    fn suspend_all(&self) -> Option<Box<dyn GcPause + '_>> {
        request_gc_pause(self).map(|guard| Box::new(guard) as Box<dyn GcPause + '_>)
    }
}
