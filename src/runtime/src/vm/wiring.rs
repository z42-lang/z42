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

    crate::threads::install_vm_roots(core);
    core.heap().set_context_reclaimer(core.loader().context_reclaimer());
}
