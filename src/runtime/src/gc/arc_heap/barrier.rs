//! **refactor (2026-09-12)**: the generational write barrier, split out of
//! `generational.rs` (which had reached the 886-line limit).
//!
//! The barrier's whole job is the **card invariant**: every old→young reference the mutator
//! creates must leave a dirty card behind, because the minor mark phase re-roots from those
//! cards and from nothing else that reaches into the old generation. A young *owner* needs
//! no card — it is found by scanning the young generation — which is exactly the asymmetry
//! that makes changing the definition of "old" a delicate operation (see
//! `ArcMagrGC::apply_promotion_age`).
//!
//! Nothing changed in the move.

use crate::metadata::Value;
use crate::gc::refs::GcRef;
use crate::gc::heap::MagrGC;

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-generational-gc P1 (2026-05-22)**: cross-gen detection
    /// helper for the write-barrier override. Marks the owner's chunk
    /// dirty when `owner.gen_age >= PROMOTION_THRESHOLD` (old) AND
    /// `new.gen_age < PROMOTION_THRESHOLD` (young).
    ///
    /// Same routine for both field + array_elem barriers — checks the
    /// owner Value's kind to pick the right region's card bitmap.
    /// Non-heap or stack-kind owners → no-op (no card to mark).
    pub(super) fn maybe_mark_cross_gen_card(&self, owner: &Value, new: &Value) {
        let new_age = match new {
            Value::Object(gc) => GcRef::gen_age(gc),
            // add-boxed-struct-identity (P4b): a boxed struct is a shared region_object
            // entry — a young box stored into an old owner MUST mark the card, else it is
            // missed by minor GC and freed prematurely.
            Value::BoxedStruct(gc) => GcRef::gen_age(gc),
            Value::Array(gc)  => GcRef::gen_age(gc),
            // fix-minor-gc-skips-var-region (#533) gave the closure block its own age, and
            // `gen_age_of` — the judge the minor mark phase actually uses — reads *that*.
            // This barrier was still reading the `env` array's age, so the two could disagree
            // (`env` is allocated before the block, hence never younger): an old-looking
            // closure stored into an old owner would skip the card while the block itself was
            // still young. Read the same age the mark phase reads.
            Value::Closure(c) => c.gen_age(),
            // make-value-copy: a `Ref` handle never escapes into a heap slot (is_heap_ref
            // = false), so a write barrier here is unreachable for it; its target's age is
            // handled via the transient-arena root scan.
            _ => return,
        };
        // Only old→young triggers a card. Young→young is in-young
        // scan already; old→old won't reach young.
        if new_age >= self.promotion_age() {
            return;
        }
        match owner {
            // add-boxed-struct-identity (P4b): a boxed struct owner is a region_object
            // entry too (reflection SetValue writes a ref leaf into its struct_refs).
            Value::Object(gc) | Value::BoxedStruct(gc) => {
                if GcRef::gen_age(gc) < self.promotion_age() { return; }
                // owner is old; mark its chunk in region_object dirty.
                let entry_ptr = gc.entry_ptr();
                // SAFETY: entry pointer valid for GcRef lifetime.
                let entry = unsafe { entry_ptr.as_ref() };
                let (ci, ei) = entry.location;
                if ci != u32::MAX {
                    self.region_object.lock().mark_card_dirty(ci, ei);
                }
            }
            Value::Array(gc) => {
                if GcRef::gen_age(gc) < self.promotion_age() { return; }
                let entry_ptr = gc.entry_ptr();
                let entry = unsafe { entry_ptr.as_ref() };
                let (ci, ei) = entry.location;
                if ci != u32::MAX {
                    self.region_array.lock().mark_card_dirty(ci, ei);
                }
            }
            _ => {} // non-heap owners — no card to mark
        }
    }

    #[allow(unused_variables)]
    pub(super) fn write_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        #[cfg(test)]
        self.fire_barrier_field(owner, slot, new);

        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => {} // no-op (one generation → no cross-gen edge)
            crate::gc::GcMode::ConcurrentMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_field caller must filter primitives via Value::is_heap_ref"
                );
                if Self::mark_if_unmarked(new) {
                    #[cfg(debug_assertions)]
                    debug_assert!(
                        !self.debug_stw_no_push.load(std::sync::atomic::Ordering::SeqCst),
                        "BUG: write_barrier_field pushing to mark_queue while debug_stw_no_push=true (STW sweep is active!) — thread {:?}",
                        std::thread::current().id()
                    );
                    self.mark_queue.lock().push(new.clone());
                }
            }
            crate::gc::GcMode::GenerationalMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_field caller must filter primitives via Value::is_heap_ref"
                );
                // **add-generational-gc P1 (2026-05-22)**: cross-gen
                // detection. If owner is old (gen_age >= threshold)
                // AND new is young (gen_age < threshold), the owner's
                // chunk gets card-dirtied so the upcoming minor GC
                // re-roots from that chunk (the young target would
                // otherwise be missed).
                self.maybe_mark_cross_gen_card(owner, new);
            }
        }
    }

    #[allow(unused_variables)]
    pub(super) fn write_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
        #[cfg(test)]
        self.fire_barrier_array_elem(arr, idx, new);

        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => {}
            crate::gc::GcMode::ConcurrentMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_array_elem caller must filter primitives via Value::is_heap_ref"
                );
                if Self::mark_if_unmarked(new) {
                    self.mark_queue.lock().push(new.clone());
                }
            }
            crate::gc::GcMode::GenerationalMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_array_elem caller must filter primitives via Value::is_heap_ref"
                );
                // add-generational-gc P1: same cross-gen check.
                self.maybe_mark_cross_gen_card(arr, new);
            }
        }
    }
}
