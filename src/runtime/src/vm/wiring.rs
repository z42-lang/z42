//! Connect VM-owned services after the core has a stable shared owner.
use std::sync::Arc;
use super::VmCore;

pub(super) fn wire(core: &Arc<VmCore>, config: &crate::config::VmConfig) {
    {
        let weak = Arc::downgrade(core);
        core.runtime.types.set_update_observer(Arc::new(move |id| {
            if let Some(core) = weak.upgrade() {
                core.runtime().invalidate_reflection_attributes(core.heap(), id);
            }
        }));
    }

    crate::diagnostics::register_crash_core(core);
    crate::threads::wire_gc_pressure(core);

    core.heap().configure_vm_collection(config.gc_max_bytes, config.gc_trace);

    core.heap.set_external_root_scanner(Box::new(crate::threads::roots::CoreRoots(Arc::downgrade(core))));

    core.heap().set_context_reclaimer(core.loader().context_reclaimer());

    // Both GC and diagnostic walks enumerate the same VM roots. The heap
    // invokes these borrowed-root callbacks only while mutators are paused.
    {
        let weak = Arc::downgrade(core);
        core.heap.set_categorized_root_scanner(Box::new(move |visit| {
            if let Some(core) = weak.upgrade() {
                unsafe { crate::threads::roots::scan_categorized_roots(&core, visit); }
            }
        }));
    }
}
