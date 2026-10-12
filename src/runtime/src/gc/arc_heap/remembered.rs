//! Object model R6: the **remembered set** — the object half of the generational card
//! invariant.
//!
//! The invariant is unchanged: *every old owner that holds a young reference must be re-rooted
//! by the next minor*. Arrays still meet it with the card table of their fixed-size region.
//! Objects live in the variable-length region, which has no card table, so each object answers
//! for itself:
//!
//! - an old object that receives a young reference (write barrier, `barrier.rs`) or becomes old
//!   while holding one (promotion: minor sweep, major aging) gets `REMEMBERED_BIT` set on its
//!   block — a CAS, so it is pushed onto [`ArcMagrGC::remembered`] exactly once;
//! - a minor traces every remembered object through ([`ArcMagrGC::seed_from_remembered`]),
//!   queues its young children, and **drops it from the set (clearing the bit) once it reaches
//!   nothing young** — the per-object version of cleaning a card;
//! - an object that dies while remembered is dropped when its block is tombstoned
//!   (`process_flagged_dead` → [`ArcMagrGC::forget_remembered`]), before its chunk can be
//!   reclaimed, so the set never holds an address that is no longer a block header.
//!
//! The mutator path takes no region lock: a relaxed age load, one `fetch_or` on the block
//! header, and — only when the bit was clear — a push under the set's own mutex.

use crate::gc::obj_ref::ObjRef;
use crate::gc::var_region::VarGcRef;
use crate::metadata::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// The barrier's half: `owner` (already known to be old) just received a young reference.
    #[inline]
    pub(super) fn remember_object(&self, owner: &ObjRef) {
        if owner.var_ref().try_set_remembered() {
            self.remembered.lock().push(*owner);
        }
    }

    /// Promotion's half (`fix-promotion-creates-uncarded-old-to-young`, for objects): an object
    /// that just became old and still refers to something young is remembered, exactly as a
    /// write would have made it. Only objects crossing the line are examined, once each.
    pub(super) fn remember_newly_old_objects(&self, objs: &[VarGcRef]) {
        for &v in objs {
            let o = ObjRef::from_var(v);
            if self.refers_to_young(&Value::Object(o)) {
                self.remember_object(&o);
            }
        }
    }

    /// Minor roots from the remembered set: trace every remembered object through, queue its
    /// young children, and keep it only while it still has some. Returns how many objects were
    /// looked at (`phase_timer` note).
    ///
    /// Skipped and dropped: an object that died since it was remembered (a tombstoned block not
    /// yet drained — its header is intact until the chunk is reclaimed, so `is_live` is a sound
    /// test), and a doomed one while an incremental major sweeps (garbage the cursor has not
    /// reached; its children may be gone — the same rule as `seed_from_dirty_cards`).
    pub(super) fn seed_from_remembered(&self, queue: &mut Vec<Value>, threshold: u8) -> usize {
        let set = std::mem::take(&mut *self.remembered.lock());
        let seen = set.len();
        let doomed = self.doomed_unless_marked();
        let mut keep = Vec::with_capacity(set.len());
        for o in set {
            let v = o.var_ref();
            if !v.is_live() {
                continue;
            }
            if doomed.is_some_and(|k| !ObjRef::is_marked(&o, k)) {
                v.clear_remembered();
                continue;
            }
            if Self::seed_card_entry(Value::Object(o), queue, threshold) {
                keep.push(o);
            } else {
                v.clear_remembered();
            }
        }
        // Nothing can have been pushed meanwhile (the world is stopped), but merge anyway.
        let mut cur = self.remembered.lock();
        keep.append(&mut cur);
        *cur = keep;
        seen
    }

    /// Drop the objects at the given block addresses (they just died) from the set.
    pub(super) fn forget_remembered(&self, dead: &[usize]) {
        let mut set = self.remembered.lock();
        if set.is_empty() {
            return;
        }
        let dead: std::collections::HashSet<usize> = dead.iter().copied().collect();
        set.retain(|o| !dead.contains(&o.var_ref().addr()));
    }

    /// Adaptive promotion lowered the promotion age: objects that were young under the old line
    /// may now be old with young children no barrier recorded. Remember every alive old object
    /// that refers to something young — the object half of `Region::dirty_every_card`, and the
    /// same one-off whole-heap cost. Runs at the end of a major (nothing doomed).
    pub(super) fn remember_every_old_object_with_young(&self) {
        let threshold = self.promotion_age();
        let mut found: Vec<ObjRef> = Vec::new();
        {
            let region = self.region_var.lock();
            region.iterate_alive_of(crate::gc::var_region::BlockType::Object, |h, header| {
                if header.gen_age() >= threshold && !header.is_remembered() {
                    found.push(ObjRef::from_var(h));
                }
            });
        }
        for o in found {
            if self.refers_to_young(&Value::Object(o)) {
                self.remember_object(&o);
            }
        }
    }

    /// The remembered set's length (tests / diagnostics).
    #[cfg(test)]
    pub(crate) fn remembered_len_for_test(&self) -> usize {
        self.remembered.lock().len()
    }
}
