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
    /// may now be old with young children no barrier recorded. Remember those that refer to
    /// something young — the object half of `Region::dirty_every_card`. Runs at the end of a major
    /// (nothing doomed), before the aging pass.
    ///
    /// Only the **cohort** the new line reclassified needs looking at — objects with an age in
    /// `[new line, old line)`, which are exactly the young-set objects now at or past the line
    /// (`VarRegion::objects_past_lowered_line`). An object already old under the old line cannot
    /// gain an unrecorded old→young edge from a lower line: a child young under the new line was
    /// young under the old one too, so the barrier (owner already old) or promotion (owner
    /// crossing the old line) remembered the owner, and the remembered set only lets an owner go
    /// once it reaches nothing young. Walking every object block instead was a whole-heap pass in
    /// the pause that closes the major — 21.6–23.0 ms of a 25.5–26.9 ms slice on
    /// `13_gc_large_heap` (400 000 live objects), the benchmark's maximum pause.
    pub(super) fn remember_lowered_cohort(&self) {
        let t = crate::gc::phase_timer::PhaseTimer::start("promotion/remember cohort");
        let cohort = self.region_var.lock().objects_past_lowered_line();
        t.count(cohort.len());
        for v in cohort {
            let o = ObjRef::from_var(v);
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
