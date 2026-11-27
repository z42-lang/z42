//! Connect VM-owned services after the core has a stable shared owner.
use std::sync::Arc;
use super::VmCore;

pub(super) fn wire(core: &Arc<VmCore>, config: &crate::config::VmConfig) {
    {
        let weak = Arc::downgrade(core);
        core.runtime.types.set_update_observer(Arc::new(move |id| {
            if let Some(core) = weak.upgrade() {
                core.runtime.reflection_types.invalidate_attributes(&core.heap, id);
            }
        }));
    }

    crate::diagnostics::register_crash_core(core);
    crate::threads::wire_gc_pressure(core);

    // add-gc-runtime-knobs : apply the process-wide GC knobs.
    //
    // `Z42_GC_MAX_BYTES` is what *arms* automatic collection at all —
    // `maybe_auto_collect` returns immediately while `max_bytes` is `None`,
    // and the three `Z42_GC_*_RATIO` knobs are fractions of this budget, so
    // without it they are inert. Unset keeps the historical behaviour
    // (collect only on an explicit `Std.GC.Collect()`).
    {
        if let Some(max) = config.gc_max_bytes {
            core.heap.set_max_heap_bytes(Some(max));
        }
        if config.gc_trace {
            core.heap.add_observer(Arc::new(crate::gc::trace::GcTracer::default()));
        }
    }

    crate::threads::install_vm_roots(core);
    core.heap.set_context_reclaimer(Box::new(super::context_gc::CoreContextReclaimer::new(core)));
}
