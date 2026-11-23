//! Connect VM-owned services after the core has a stable shared owner.
use std::sync::Arc;
use super::{VmCore, VM_CORES};

pub(super) fn wire(core: &Arc<VmCore>, config: &crate::config::VmConfig) {
    {
        let weak = Arc::downgrade(core);
        core.runtime.types.set_update_observer(Arc::new(move |id| {
            if let Some(core) = weak.upgrade() {
                core.runtime.reflection_types.invalidate_attributes(&core.heap, id);
            }
        }));
    }

    // add-os-signal-handler : register this Arc<VmCore> into
    // the process-wide VM_CORES registry so the POSIX signal handler
    // can walk it to capture z42 call stacks on hard crash. Lazy-prune
    // dead Weak entries at the same time (cheap O(n) sweep amortized
    // across VmCore creations).
    if let Ok(mut g) = VM_CORES.lock() {
        g.retain(|w| w.strong_count() > 0);
        g.push(Arc::downgrade(core));
    }

    // add-gc-safepoint-auto-threshold : wire the
    // needs_auto_collect flag into the heap so its pressure-trip
    // path defers to the next safepoint check instead of collecting
    // inline (the inline path has no &VmContext and would race with
    // concurrent mutators' frame.regs writes).
    core.heap.set_external_needs_collect_flag(Arc::clone(&core.threads.needs_auto_collect));
    // P0-16: and let it send every mutator to the slow path when it raises that flag, so the
    // collection does not wait out the safepoint throttle. `Weak`, like the root scanner.
    {
        let core_weak = Arc::downgrade(core);
        core.heap.set_safepoint_poke(Arc::new(move || {
            if let Some(c) = core_weak.upgrade() {
                crate::threads::safepoint::poke_safepoints(&c);
            }
        }));
    }

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

    core.heap.set_external_root_scanner(Box::new(crate::threads::roots::CoreRoots(Arc::downgrade(core))));

    core.heap.set_context_reclaimer(Box::new(super::context_gc::CoreContextReclaimer::new(core)));

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
