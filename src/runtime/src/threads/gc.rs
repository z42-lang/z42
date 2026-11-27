//! Connect mutator lifetime, heap pressure and GC pause services.
use std::sync::Arc;

use super::safepoint::{poke_safepoints, request_gc_pause, GcPauseGuard};
use crate::gc::Heap;
use crate::gc::to_vm::{GcPause, GcToVm};
use crate::vm::{VmContext, VmCore};

/// Defer heap pressure to a mutator poll using a weak Core reference.
pub(crate) fn wire_gc_pressure(core: &Arc<VmCore>) {
    let weak = Arc::downgrade(core);
    core.heap().defer_auto_collect_until_safepoint(
        Arc::clone(&core.threads().needs_auto_collect),
        Arc::new(move || {
            if let Some(core) = weak.upgrade() {
                poke_safepoints(&core);
            }
        }),
    );
}

pub(crate) fn attach_mutator(heap: &Heap) {
    crate::gc::tlab::arm();
    crate::gc::satb::bind_thread(heap.heap_epoch());
}

pub(crate) fn detach_mutator(ctx: &VmContext) {
    // Return borrowed allocations and the SATB buffer before unbinding the mutator.
    ctx.heap().retire_thread_tlab();
    ctx.heap().retire_parked_tlab(&ctx.threads().parked_tlab);
    crate::gc::tlab::disarm();
    crate::gc::satb::unbind_thread();
}

impl GcPause for GcPauseGuard<'_> {}
// SAFETY: the existing pause guard coordinates every registered mutator.
unsafe impl GcToVm for VmContext {
    fn suspend_all(&self) -> Option<Box<dyn GcPause + '_>> {
        request_gc_pause(self).map(|guard| Box::new(guard) as Box<dyn GcPause + '_>)
    }
}
