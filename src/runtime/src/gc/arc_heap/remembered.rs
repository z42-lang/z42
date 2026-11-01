//! Object model R6 / R9: the **remembered set** — how the minor finds the old generation's
//! references into the young one.
//!
//! The invariant: *every old owner that holds a young reference must be re-rooted by the next
//! minor*. Every managed value is a block of the variable-length region, which has no card table,
//! so each owner — an object or an array — answers for itself:
//!
//! - an old owner that receives a young reference (write barrier, `barrier.rs`) or becomes old
//!   while holding one (promotion: minor sweep, major aging) gets `REMEMBERED_BIT` set on its
//!   block — an atomic or, so it is pushed onto [`ArcMagrGC::remembered`] exactly once;
//! - a minor traces every remembered owner through ([`ArcMagrGC::seed_from_remembered`]), queues
//!   its young children, and **drops it from the set (clearing the bit) once it reaches nothing
//!   young**;
//! - an owner that dies while remembered is dropped when its block is tombstoned
//!   (`process_flagged_dead` → [`ArcMagrGC::forget_remembered`]), before its chunk can be
//!   reclaimed, so the set never holds an address that is no longer a block header.
//!
//! The granularity is the owner: a remembered array is traced whole by each minor while it keeps
//! a young element (the card table it replaced also traced every array of a dirty card whole).
//!
//! The mutator path takes no region lock: a relaxed age load, one `fetch_or` on the block
//! header, and — only when the bit was clear — a push under the set's own mutex.

use crate::object::header::arr_ref::ArrRef;
use crate::object::header::obj_ref::ObjRef;
use crate::object::header::{BlockType, VarGcRef};
use crate::object::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// The barrier's half: the owner block `b` (already known to be old) just received a young
    /// reference.
    #[inline]
    pub(super) fn remember_block(&self, b: VarGcRef) {
        if b.try_set_remembered() {
            self.remembered.lock().push(b);
        }
    }

    /// The `Value` of a remembered owner block (an object or an array — nothing else is ever
    /// remembered).
    #[inline]
    fn owner_value(b: VarGcRef) -> Value {
        if b.block_type() == BlockType::Object {
            Value::Object(ObjRef::from_var(b))
        } else {
            Value::Array(ArrRef::from_var(b))
        }
    }

    /// Promotion's half (`fix-promotion-creates-uncarded-old-to-young`): an object or array that
    /// just became old and still refers to something young is remembered, exactly as a write
    /// would have made it. Only owners crossing the line are examined, once each.
    pub(super) fn remember_newly_old(&self, owners: &[VarGcRef]) {
        for &b in owners {
            if self.refers_to_young(&Self::owner_value(b)) {
                self.remember_block(b);
            }
        }
    }

    /// Minor roots from the remembered set: trace every remembered owner through, queue its young
    /// children, and keep it only while it still has some. Returns how many owners were looked at
    /// (`phase_timer` note).
    ///
    /// Skipped and dropped: an owner that died since it was remembered (a tombstoned block not yet
    /// drained — its header is intact until the chunk is reclaimed, so `is_live` is a sound test),
    /// and a doomed one while an incremental major sweeps (garbage the cursor has not reached; its
    /// children may be gone).
    pub(super) fn seed_from_remembered(&self, queue: &mut Vec<Value>, threshold: u8) -> usize {
        let set = std::mem::take(&mut *self.remembered.lock());
        let seen = set.len();
        let doomed = self.doomed_unless_marked();
        let mut keep = Vec::with_capacity(set.len());
        for b in set {
            if !b.is_live() {
                continue;
            }
            if doomed.is_some_and(|k| !b.is_marked(k)) {
                b.clear_remembered();
                continue;
            }
            if Self::seed_old_owner(Self::owner_value(b), queue, threshold) {
                keep.push(b);
            } else {
                b.clear_remembered();
            }
        }
        // Nothing can have been pushed meanwhile (the world is stopped), but merge anyway.
        let mut cur = self.remembered.lock();
        keep.append(&mut cur);
        *cur = keep;
        seen
    }

    /// Drop the owners at the given block addresses (they just died) from the set.
    pub(super) fn forget_remembered(&self, dead: &[usize]) {
        let mut set = self.remembered.lock();
        if set.is_empty() {
            return;
        }
        let dead: std::collections::HashSet<usize> = dead.iter().copied().collect();
        set.retain(|b| !dead.contains(&b.addr()));
    }

    /// Adaptive promotion lowered the promotion age: owners that were young under the old line may
    /// now be old with young children no barrier recorded. Remember those that refer to something
    /// young. Runs at the end of a major (nothing doomed), before the aging pass.
    ///
    /// Only the **cohort** the new line reclassified needs looking at — owners with an age in
    /// `[new line, old line)`, which are exactly the young-set owners now at or past the line
    /// (`VarRegion::owners_past_lowered_line`). An owner already old under the old line cannot
    /// gain an unrecorded old→young edge from a lower line: a child young under the new line was
    /// young under the old one too, so the barrier (owner already old) or promotion (owner
    /// crossing the old line) remembered the owner, and the remembered set only lets an owner go
    /// once it reaches nothing young. Walking every block instead was a whole-heap pass in the
    /// pause that closes the major — 21.6–23.0 ms of a 25.5–26.9 ms slice on `13_gc_large_heap`
    /// (400 000 live objects), the benchmark's maximum pause.
    pub(super) fn remember_lowered_cohort(&self) {
        let t = crate::gc::phase_timer::PhaseTimer::start("promotion/remember cohort");
        let cohort = self.region_var.lock().owners_past_lowered_line();
        t.count(cohort.len());
        self.remember_newly_old(&cohort);
    }

    /// The remembered set's length (tests / diagnostics).
    #[cfg(test)]
    pub(crate) fn remembered_len_for_test(&self) -> usize {
        self.remembered.lock().len()
    }
}
