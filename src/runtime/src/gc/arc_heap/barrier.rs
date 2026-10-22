//! **refactor **: the generational write barrier, split out of
//! `generational.rs` (which had reached the 886-line limit).
//!
//! The barrier's whole job is the **remembered-set invariant**: every old→young reference the
//! mutator creates must leave its owner — an object or an array (object model R6 / R9) — in the
//! remembered set, because the minor mark phase re-roots from it and from nothing else that
//! reaches into the old generation. A young *owner* needs no record — it is found by scanning
//! the young generation — which is exactly the asymmetry that makes changing the definition of
//! "old" a delicate operation (see `ArcMagrGC::apply_promotion_age`).

use crate::metadata::Value;
use crate::gc::ArrRef;
use crate::gc::obj_ref::ObjRef;
use crate::gc::heap::MagrGC;

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-generational-gc P1 **: cross-gen detection for the write-barrier
    /// override. Remembers the owner when it is old (`gen_age >= promotion age`) AND `new` is
    /// young. Same routine for field and array-element barriers; non-heap / stack owners → no-op.
    pub(super) fn maybe_remember_owner(&self, owner: &Value, new: &Value) {
        let new_age = match new {
            Value::Object(gc) => ObjRef::gen_age(gc),
            // add-boxed-struct-identity (P4b): a boxed struct is a shared object — a young box
            // stored into an old owner MUST be recorded, else it is missed by minor GC and freed
            // prematurely.
            Value::BoxedStruct(gc) => ObjRef::gen_age(gc),
            Value::Array(gc)  => ArrRef::gen_age(gc),
            // fix-minor-gc-skips-var-region (#533) gave the closure block its own age, and
            // `gen_age_of` — the judge the minor mark phase actually uses — reads *that*.
            // This barrier was still reading the `env` array's age, so the two could disagree
            // (`env` is allocated before the block, hence never younger): an old-looking
            // closure stored into an old owner would skip the card while the block itself was
            // still young. Read the same age the mark phase reads.
            Value::Closure(c) => c.gen_age(),
            // fix-gen-barrier-strings: strings / func-refs are var blocks with a real age too
            // (#533 made `gen_age_of` report it and the minor sweep reclaim young var blocks).
            // This barrier still let them fall through to `_ => return`, so a **young string
            // stored into an old object / array** never dirtied a card: the next minor did not
            // re-scan the old owner, the string went unmarked and was swept while still
            // referenced (use-after-free → garbage strings / failed map lookups). Seen as
            // intermittent failures of the nursery-1M compiler build under `--jobs 4`.
            Value::Str(s) => s.gen_age(),
            // make-value-copy: a `Ref` handle never escapes into a heap slot (is_heap_ref
            // = false), so a write barrier here is unreachable for it; its target's age is
            // handled via the transient-arena root scan.
            _ => return,
        };
        // Only old→young is recorded. Young→young is in-young scan already; old→old won't reach
        // young.
        if new_age >= self.promotion_age() {
            return;
        }
        // The region has no card table: an old owner joins the remembered set (one header
        // `fetch_or`, and a push only the first time; no region lock on this path). See
        // `remembered.rs`. A boxed struct owner is an object too (reflection `SetValue` writes a
        // ref leaf into its blob).
        let owner_block = match owner {
            Value::Object(gc) | Value::BoxedStruct(gc) => gc.var_ref(),
            Value::Array(gc) => gc.var_ref(),
            _ => return, // non-heap owners — nothing to remember
        };
        if owner_block.gen_age() < self.promotion_age() {
            return;
        }
        self.remember_block(owner_block);
    }

    #[allow(unused_variables)]
    pub(super) fn write_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        #[cfg(test)]
        self.fire_barrier_field(owner, slot, new);

        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => {} // no-op (one generation → no cross-gen edge)
            crate::gc::GcMode::GenerationalMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_field caller must filter primitives via Value::is_heap_ref"
                );
                // **add-generational-gc P1 **: cross-gen detection. If the owner is old
                // AND `new` is young, the owner is remembered so the upcoming minor re-roots from
                // it (the young target would otherwise be missed).
                self.maybe_remember_owner(owner, new);
            }
        }
    }

    #[allow(unused_variables)]
    pub(super) fn write_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
        #[cfg(test)]
        self.fire_barrier_array_elem(arr, idx, new);

        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => {}
            crate::gc::GcMode::GenerationalMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_array_elem caller must filter primitives via Value::is_heap_ref"
                );
                // add-generational-gc P1: same cross-gen check.
                self.maybe_remember_owner(arr, new);
            }
        }
    }
}
