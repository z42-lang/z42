//! Object model R6: per-object GC state that does not fit in an object's block header — weak
//! references, finalizers, soft references — and the one path that retires it when an object
//! dies.
//!
//! An object block has no finalizer slot or soft-ref counter (the fixed-size `RegionEntry` had
//! both, 12 bytes on every object for capabilities almost none use). Instead:
//!
//! - **finalizers** live in `RcHeapInner::obj_finalizers`, keyed by block-header address;
//! - **soft references** are registry entries keyed the same way (`soft_registry`);
//! - both flag the block with `SIDE_BIT`, and so does the remembered set with
//!   `REMEMBERED_BIT`. Tombstoning a flagged block records its address in the region's
//!   flagged-dead list; [`ArcMagrGC::process_flagged_dead`] drains it — firing finalizers,
//!   dropping soft entries, trimming the remembered set — and **must run before any chunk is
//!   pooled or freed**, while the dead headers are still in place. Sweep never looks anything up
//!   for an unflagged block.
//! - **weak references** record the owning chunk's reuse epoch (`WeakObjRef`), so an upgrade
//!   never reads memory that was re-carved or handed back to the allocator.

use crate::gc::obj_ref::{ObjRef, WeakObjRef};
use crate::gc::types::FinalizerFn;

impl crate::gc::arc_heap::ArcMagrGC {
    /// A weak reference to `obj`: its handle plus its chunk's current reuse epoch.
    pub(super) fn downgrade_object(&self, obj: &ObjRef) -> WeakObjRef {
        let chunk = obj.var_ref().chunk_idx();
        let epoch = if chunk == u32::MAX {
            0
        } else {
            self.region_var.lock().chunk_epoch(chunk).unwrap_or(0)
        };
        WeakObjRef { obj: *obj, chunk, epoch }
    }

    /// Upgrade a weak object reference: `None` once the object died — its chunk was pooled /
    /// freed since (epoch moved: the address may now be the middle of another block, or not
    /// mapped at all), or its block was tombstoned or reused (generation guard). Takes the
    /// variable-length region's lock; never call it with `inner` held.
    pub(crate) fn upgrade_weak_object(&self, w: &WeakObjRef) -> Option<ObjRef> {
        if w.chunk != u32::MAX {
            let region = self.region_var.lock();
            if region.chunk_epoch(w.chunk) != Some(w.epoch) {
                return None;
            }
            // Under the lock the chunk cannot be pooled or freed, so the header read is sound.
            return w.obj.var_ref().is_live().then_some(w.obj);
        }
        w.obj.var_ref().is_live().then_some(w.obj)
    }

    /// Register `fin` for `obj` (replacing any previous one).
    pub(super) fn set_object_finalizer(&self, obj: &ObjRef, fin: FinalizerFn) {
        obj.var_ref().set_side();
        self.inner.lock().obj_finalizers.insert(obj.var_ref().addr(), fin);
    }

    /// Remove and return `obj`'s finalizer.
    pub(super) fn take_object_finalizer(&self, obj: &ObjRef) -> Option<FinalizerFn> {
        self.inner.lock().obj_finalizers.remove(&obj.var_ref().addr())
    }

    /// Drain the region's flagged-dead list: fire the finalizers of the objects that died, drop
    /// their soft-registry entries, and take them out of the remembered set. Every sweep tail
    /// calls it before reclaiming chunks (`reclaim_dead_chunks_and_measure`), and so do the
    /// incremental sweep slices and `finalize_now` so finalizers fire promptly.
    pub(crate) fn process_flagged_dead(&self) {
        let dead = self.region_var.lock().take_flagged_dead();
        if dead.is_empty() {
            return;
        }
        let fins: Vec<FinalizerFn> = {
            let mut inner = self.inner.lock();
            for &addr in &dead {
                inner.soft_registry.remove_all(addr);
            }
            if inner.obj_finalizers.is_empty() {
                Vec::new()
            } else {
                dead.iter().filter_map(|a| inner.obj_finalizers.remove(a)).collect()
            }
        };
        self.forget_remembered(&dead);
        // Fired outside every lock: a finalizer is arbitrary host code.
        for f in fins {
            f();
        }
    }

    /// Objects with a registered finalizer that are still alive — `HeapStats::finalizers_pending`.
    /// An object an incremental sweep has already judged dead is not counted (`admit_resurrected`
    /// would refuse it).
    pub(super) fn count_object_finalizers(&self) -> u64 {
        let keys: std::collections::HashSet<usize> =
            self.inner.lock().obj_finalizers.keys().copied().collect();
        if keys.is_empty() {
            return 0;
        }
        let doomed = self.doomed_unless_marked();
        let region = self.region_var.lock();
        let mut n = 0u64;
        region.iterate_alive_of(crate::gc::var_region::BlockType::Object, |h, _| {
            if keys.contains(&h.addr()) && doomed.is_none_or(|k| h.is_marked(k)) {
                n += 1;
            }
        });
        n
    }
}
