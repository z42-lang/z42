//! `ArcMagrGC` 分代 GC：minor/major/promotion/card + gen_age + write barriers。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use crate::gc::heap::MagrGC;
use crate::metadata::{Value};
use crate::gc::refs::{GcRef};
use crate::gc::types::{FinalizerFn};

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-generational-gc P2 (2026-05-22)**: read the gen_age of
    /// any `Value`. Returns 0 for primitives + stack refs (irrelevant
    /// to generational dispatch — mark/sweep already handles those).
    pub(super) fn gen_age_of(v: &Value) -> u8 {
        match v {
            Value::Object(gc) => GcRef::gen_age(gc),
            Value::Array(gc)  => GcRef::gen_age(gc),
            // unify-gc-heap PR-2: the closure block (region_var) is non-generational (STW); the
            // generational-relevant object is its `env` array (region_array), as before.
            Value::Closure(c) => GcRef::gen_age(&crate::metadata::types::closure_data_of(c).env),
            // add-boxed-struct-identity (P4b): gen-age of the boxed struct's shared object.
            Value::BoxedStruct(gc) => GcRef::gen_age(gc),
            // make-value-copy: `Ref` handle carries no direct heap allocation to age
            // (its target is aged via the arena root scan) → 0.
            _ => 0,
        }
    }

    /// **add-generational-gc P2 (2026-05-22)**: mark phase for minor GC.
    ///
    /// Roots = pinned roots + external_root_scanner output + entries
    /// in dirty card chunks of both regions. The latter ensures any
    /// old object that has received a young-pointer write since the
    /// last major GC is treated as an additional root.
    ///
    /// BFS marks every reachable entry. When tracing children, only
    /// young children (gen_age < PROMOTION_THRESHOLD) are pushed to
    /// the queue. Old children are skipped — either they have no
    /// young descendants (otherwise they'd be in dirty cards via the
    /// barrier), or their old→young paths are seeded as separate
    /// dirty-card roots. This bounds minor mark work at O(young +
    /// |dirty-card entries|).
    pub(super) fn mark_phase_minor(&self) -> usize {
        let threshold = crate::gc::region::PROMOTION_THRESHOLD;
        let mut queue: Vec<Value> = Vec::new();

        // Pinned roots + external scanner.
        queue.extend(self.inner.lock().roots.values().cloned());
        {
            let scanner = self.external_root_scanner.lock();
            if let Some(scan) = scanner.as_ref() {
                scan(&mut |v| queue.push(v.clone()));
            }
        }

        // Dirty card roots — all entries in dirty chunks of both regions.
        {
            let region = self.region_object.lock();
            region.iterate_dirty_cards(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                // SAFETY: handle came from iterate_dirty_cards; entry
                // is alive + generation matches at iteration time.
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                queue.push(Value::Object(gc));
            });
        }
        {
            let region = self.region_array.lock();
            region.iterate_dirty_cards(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                queue.push(Value::Array(gc));
            });
        }

        let mut marked = 0usize;
        while let Some(v) = queue.pop() {
            let just_marked = Self::mark_if_unmarked(&v);
            if !just_marked { continue; }
            marked += 1;

            v.trace_children(&mut |child| {
                // Only enqueue young children. Old children that need
                // re-rooting are already covered via dirty cards.
                if Self::gen_age_of(child) < threshold {
                    queue.push(child.clone());
                }
            });
        }
        marked
    }

    /// **add-generational-gc P2 (2026-05-22)**: sweep phase for minor GC.
    ///
    /// Walks `young_list` in both regions; for each entry:
    /// - `is_marked == true` → clear mark, increment gen_age (promote
    ///   to next age tier); if reaches threshold, region.promote()
    ///   removes from young_list.
    /// - `is_marked == false` → fire finalizer, tombstone (alive=false,
    ///   generation++, push to free_list AND remove from young_list).
    ///
    /// Old entries are NOT visited — major GC handles them.
    /// card_dirty is NOT cleared by minor (stable old→young refs need
    /// to keep their cards dirty until major scans them).
    pub(super) fn sweep_phase_young_only(&self) -> u64 {
        let mut freed_bytes: u64 = 0;

        // Object region
        let mut tombstones_object: Vec<(crate::gc::region::RegionHandle, Option<FinalizerFn>, u64)> = Vec::new();
        let mut survivors_object: Vec<crate::gc::region::RegionHandle> = Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_young(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                    survivors_object.push(h);
                } else {
                    let size = {
                        let obj = entry.value.lock();
                        Self::script_object_size_estimate(&obj)
                    };
                    let fin = entry.finalizer.lock().take();
                    tombstones_object.push((h, fin, size));
                }
            });
        }
        // Promote survivors (may remove some from young_list at threshold).
        for h in survivors_object {
            self.region_object.lock().promote(h);
        }
        // Tombstone dead young entries.
        for (h, fin, size) in tombstones_object {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            {
                let region = self.region_object.lock();
                let entry = region.resolve(h);
                if entry.alive.load(std::sync::atomic::Ordering::Acquire) {
                    let mut obj = entry.value.lock();
                    // unify-object-byte-layout: break every strong reference edge — the
                    // side-table `refs` AND (PR-3 chunk 2b) the object/array pointers
                    // byte-inlined in `bytes`.
                    for r in obj.refs.iter_mut() {
                        *r = Value::Null;
                    }
                    obj.clear_inline_refs();
                }
            }
            self.region_object.lock().tombstone(h);
        }

        // Array region (parallel logic)
        let mut tombstones_array: Vec<(crate::gc::region::RegionHandle, Option<FinalizerFn>, u64)> = Vec::new();
        let mut survivors_array: Vec<crate::gc::region::RegionHandle> = Vec::new();
        {
            let region = self.region_array.lock();
            region.iterate_young(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                    survivors_array.push(h);
                } else {
                    let size = {
                        let arr = entry.value.lock();
                        Self::array_size_estimate(&arr)
                    };
                    let fin = entry.finalizer.lock().take();
                    tombstones_array.push((h, fin, size));
                }
            });
        }
        for h in survivors_array {
            self.region_array.lock().promote(h);
        }
        for (h, fin, size) in tombstones_array {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            // unify-gc-heap PR-3: no eager element drop here — the array's element
            // storage lives in a `region_var` block (uniquely owned by this header),
            // reclaimed by `region_var.sweep()` (drop-glue drops the boxed Values) in
            // the same cycle. Tombstoning the header just releases the region_array slot.
            self.region_array.lock().tombstone(h);
        }

        freed_bytes
    }

    /// **add-generational-gc P2 (2026-05-22)**: full minor GC cycle.
    /// Mark phase (young + dirty cards) → sweep phase (young only) →
    /// returns freed_bytes estimate. Card dirty bits are NOT cleared
    /// here — they accumulate across minors and are only cleared by
    /// the next major GC. This preserves correctness for stable
    /// old→young references (whose cards were dirtied at the time of
    /// the write but the target young object hasn't yet been promoted).
    pub(super) fn run_cycle_collection_minor(&self) -> u64 {
        let _newly_marked = self.mark_phase_minor();
        self.sweep_phase_young_only()
    }

    /// **add-generational-gc P3 (2026-05-22)**: full major GC cycle.
    /// Same as `run_cycle_collection_stw` (mark whole heap from
    /// roots; sweep all entries) PLUS clears `card_dirty` at the end
    /// (cross-gen references are now fully traced; cards can reset
    /// for the next round of minors).
    pub(super) fn run_cycle_collection_major(&self) -> u64 {
        let freed = self.run_cycle_collection_stw();
        // Major scanned the whole heap → cards no longer track
        // anything we don't already know. Clear so the next minor
        // starts with a fresh dirty set.
        self.region_object.lock().clear_card_dirty();
        self.region_array.lock().clear_card_dirty();
        freed
    }

    /// **add-generational-gc P3 (2026-05-22)**: escalation threshold.
    /// If the fraction of young entries surviving a minor GC exceeds
    /// this, the next collect is escalated to major immediately.
    /// Default 0.75 from [`RuntimeConfig::gc_minor_threshold`]; override
    /// via `Z42_GC_MINOR_THRESHOLD`.
    ///
    /// runtime-config-phase2 (2026-06-03): centralised through
    /// `crate::config::runtime_config()`; previous per-callsite
    /// `OnceLock<f32>` retired.
    #[allow(dead_code)] // wired in collect_cycles_with_context below
    pub(super) fn minor_escalation_threshold() -> f32 {
        crate::config::runtime_config().gc_minor_threshold
    }

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
            // unify-gc-heap PR-2: closure block non-generational; use its `env` array's age.
            Value::Closure(c) => GcRef::gen_age(&crate::metadata::types::closure_data_of(c).env),
            // make-value-copy: a `Ref` handle never escapes into a heap slot (is_heap_ref
            // = false), so a write barrier here is unreachable for it; its target's age is
            // handled via the transient-arena root scan.
            _ => return,
        };
        // Only old→young triggers a card. Young→young is in-young
        // scan already; old→old won't reach young.
        if new_age >= crate::gc::region::PROMOTION_THRESHOLD {
            return;
        }
        match owner {
            // add-boxed-struct-identity (P4b): a boxed struct owner is a region_object
            // entry too (reflection SetValue writes a ref leaf into its struct_refs).
            Value::Object(gc) | Value::BoxedStruct(gc) => {
                if GcRef::gen_age(gc) < crate::gc::region::PROMOTION_THRESHOLD { return; }
                // owner is old; mark its chunk in region_object dirty.
                let entry_ptr = gc.entry_ptr();
                // SAFETY: entry pointer valid for GcRef lifetime.
                let entry = unsafe { entry_ptr.as_ref() };
                let (ci, _) = entry.location;
                if ci != u32::MAX {
                    self.region_object.lock().mark_card_dirty(ci);
                }
            }
            Value::Array(gc) => {
                if GcRef::gen_age(gc) < crate::gc::region::PROMOTION_THRESHOLD { return; }
                let entry_ptr = gc.entry_ptr();
                let entry = unsafe { entry_ptr.as_ref() };
                let (ci, _) = entry.location;
                if ci != u32::MAX {
                    self.region_array.lock().mark_card_dirty(ci);
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
            crate::gc::GcMode::StwMarkSweep => {} // no-op (production default)
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
