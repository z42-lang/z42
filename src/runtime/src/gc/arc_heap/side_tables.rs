//! Object model R6 / R9: per-block GC state that does not fit in an object's or array's block
//! header — weak references, finalizers, soft references — and the one path that retires it when
//! the block dies.
//!
//! A block has no finalizer slot or soft-ref counter (12 bytes on every object and array for
//! capabilities almost none use). Instead:
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

use crate::gc::arr_ref::{ArrRef, WeakArrRef};
use crate::gc::obj_ref::{ObjRef, WeakObjRef};
use crate::gc::types::FinalizerFn;
use crate::gc::var_region::VarGcRef;
use crate::metadata::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// The block of an object or array `Value` — what side tables and the remembered set key on.
    #[inline]
    pub(super) fn owner_block(v: &Value) -> Option<VarGcRef> {
        match v {
            Value::Object(o) | Value::BoxedStruct(o) => Some(o.var_ref()),
            Value::Array(a) => Some(a.var_ref()),
            _ => None,
        }
    }

    /// `(chunk, epoch)` a weak reference to `b` records: its chunk and that chunk's current reuse
    /// epoch. A block outside every chunk (a detached / leaked one) is never reclaimed.
    fn weak_stamp(&self, b: VarGcRef) -> (u32, u32) {
        let region = self.region_var.lock();
        match region.chunk_of(b.addr()) {
            Some(ci) => (ci as u32, region.chunk_epoch(ci as u32).unwrap_or(0)),
            None => (u32::MAX, 0),
        }
    }

    /// Whether the block a weak reference recorded as `(chunk, epoch)` is still alive: its chunk
    /// was not pooled / freed since (epoch moved: the address may now be the middle of another
    /// block, or not mapped at all), and the block was not tombstoned or reused (generation
    /// guard). Takes the variable-length region's lock; never call it with `inner` held.
    fn weak_alive(&self, b: VarGcRef, chunk: u32, epoch: u32) -> bool {
        if chunk != u32::MAX {
            let region = self.region_var.lock();
            if region.chunk_epoch(chunk) != Some(epoch) {
                return false;
            }
            // Under the lock the chunk cannot be pooled or freed, so the header read is sound.
            return b.is_live();
        }
        b.is_live()
    }

    /// A weak reference to `obj`: its handle plus its chunk's current reuse epoch.
    pub(super) fn downgrade_object(&self, obj: &ObjRef) -> WeakObjRef {
        let (chunk, epoch) = self.weak_stamp(obj.var_ref());
        WeakObjRef { obj: *obj, chunk, epoch }
    }

    /// Upgrade a weak object reference: `None` once the object died (see [`Self::weak_alive`]).
    pub(crate) fn upgrade_weak_object(&self, w: &WeakObjRef) -> Option<ObjRef> {
        self.weak_alive(w.obj.var_ref(), w.chunk, w.epoch).then_some(w.obj)
    }

    /// A weak reference to `arr` (object model R9 — same rule as objects).
    pub(super) fn downgrade_array(&self, arr: &ArrRef) -> WeakArrRef {
        let (chunk, epoch) = self.weak_stamp(arr.var_ref());
        WeakArrRef { arr: *arr, chunk, epoch }
    }

    /// Upgrade a weak array reference: `None` once the array died.
    pub(crate) fn upgrade_weak_array(&self, w: &WeakArrRef) -> Option<ArrRef> {
        self.weak_alive(w.arr.var_ref(), w.chunk, w.epoch).then_some(w.arr)
    }

    /// Register `fin` for the object / array block `b` (replacing any previous one).
    pub(super) fn set_block_finalizer(&self, b: VarGcRef, fin: FinalizerFn) {
        b.set_side();
        self.inner.lock().obj_finalizers.insert(b.addr(), fin);
    }

    /// Remove and return `b`'s finalizer.
    pub(super) fn take_block_finalizer(&self, b: VarGcRef) -> Option<FinalizerFn> {
        self.inner.lock().obj_finalizers.remove(&b.addr())
    }

    /// Drain the region's flagged-dead list: fire the finalizers of the objects and arrays that
    /// died, drop their soft-registry entries, and take them out of the remembered set. Every sweep tail
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

    /// Objects and arrays with a registered finalizer that are still alive —
    /// `HeapStats::finalizers_pending`. A block an incremental sweep has already judged dead is not
    /// counted (`admit_resurrected` would refuse it).
    pub(super) fn count_object_finalizers(&self) -> u64 {
        let keys: std::collections::HashSet<usize> =
            self.inner.lock().obj_finalizers.keys().copied().collect();
        if keys.is_empty() {
            return 0;
        }
        let doomed = self.doomed_unless_marked();
        let region = self.region_var.lock();
        let mut n = 0u64;
        region.iterate_alive(|h, _| {
            if keys.contains(&h.addr()) && doomed.is_none_or(|k| h.is_marked(k)) {
                n += 1;
            }
        });
        n
    }
}
