//! `ArcMagrGC` 分代 GC：minor/major/promotion/card + gen_age + write barriers。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use crate::gc::heap::MagrGC;
use crate::metadata::{Value};
use crate::gc::refs::{GcRef};
use crate::gc::types::{FinalizerFn};

/// What one minor collection reclaimed. `reclaimed_entries` is what the escalation heuristic
/// needs: **the number of young entries the sweep actually tombstoned**, across all three
/// regions. Survival can only be read off that — see `collect_cycles_with_context`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MinorSweepResult {
    pub(crate) freed_bytes: u64,
    pub(crate) reclaimed_entries: usize,
    /// Bytes this minor moved into the old generation (add-bounded-nursery).
    pub(crate) promoted_bytes: u64,
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-generational-gc P2 (2026-05-22)**: read the gen_age of
    /// any `Value`. Returns 0 for primitives + stack refs (irrelevant
    /// to generational dispatch — mark/sweep already handles those).
    pub(super) fn gen_age_of(v: &Value) -> u8 {
        match v {
            Value::Object(gc) => GcRef::gen_age(gc),
            Value::Array(gc)  => GcRef::gen_age(gc),
            // fix-minor-gc-skips-var-region (2026-09-08): the closure block carries its own
            // age now, so report *its* age rather than its `env` array's.
            //
            // This is load-bearing, not tidiness. Reporting 0 for a block the minor sweep
            // never visits left a **stale mark bit** on it: the next minor popped the same
            // closure, its `mark()` CAS failed, `just_marked` came back false — and its
            // children were therefore never traced. A still-young `env` array reachable only
            // through that closure went unmarked and was swept out from under it.
            Value::Closure(c) => c.gen_age(),
            // Strings / func-refs are var blocks too. They used to fall through to `_ => 0`
            // (always "young"), so every reachable string was re-marked at every minor and
            // kept a stale mark. They are leaves, so that only cost floating garbage rather
            // than correctness — but there is no reason to pay it now that the age is real.
            Value::Str(s) | Value::FuncRef(s) => s.gen_age(),
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
        let threshold = self.promotion_age;
        let mut queue: Vec<Value> = Vec::new();

        // Pinned roots + external scanner.
        queue.extend(self.inner.lock().roots.values().cloned());
        {
            let scanner = self.external_root_scanner.lock();
            if let Some(scan) = scanner.as_ref() {
                scan(&mut |v| queue.push(v.clone()));
            }
        }

        // Dirty-card roots — the entries of every dirty card in both regions.
        //
        // **add-finer-card-granularity (2026-09-10)**: a card's entries are traced **here**
        // rather than pushed onto the queue, for two reasons. Only their *young* children are
        // worth queueing — the old ones are skipped by the filter in the BFS anyway, so
        // pushing the parents just made the loop re-derive that. And tracing here is what lets
        // a card be **cleaned**: a card whose entries no longer reach anything young has done
        // its job and should stop being a root.
        //
        // That second half is what stops the dirty set from snowballing. Nothing cleared cards
        // between majors, while both the write barrier and promotion (#539) kept adding to
        // them — measured on `z42c.semantics`, the late minors seeded **518 530** card roots
        // to find **76** young objects.
        self.seed_from_dirty_cards(&mut queue, threshold);

        let mut marked = 0usize;
        while let Some(v) = queue.pop() {
            // fix-minor-stale-mark-on-old-roots (2026-09-08): **only young entries are
            // marked.** A minor never sweeps old ones, so a mark on them buys nothing —
            // and it is actively wrong. `sweep_phase_young_only` clears the mark on *young*
            // survivors only; nothing else clears it before the next major. So the mark an
            // old root picked up in minor N was still set in minor N+1, `mark_if_unmarked`
            // returned `false`, and this loop `continue`d **without tracing its children**.
            // Every young object reachable only through that root then went unmarked and was
            // swept while still referenced — the `expected string, got Null` crash that made
            // `Z42_GC_MODE=generational` unusable past its second minor.
            //
            // Old roots are seeded from a finite set (pinned roots + external scanner +
            // dirty cards) and old *children* are never enqueued below, so tracing them
            // unmarked still terminates; the only cost is re-tracing an old object that
            // appears in the root set twice, which is O(its fields), not O(its subgraph).
            if Self::gen_age_of(&v) < threshold {
                if !Self::mark_if_unmarked(&v) { continue; }
                marked += 1;
            }

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
    /// **add-finer-card-granularity (2026-09-10)**: trace every dirty card in both fixed
    /// regions, queue the young children it reaches, and **clean the cards that reach none**.
    ///
    /// A card is a re-rooting hint, not a fact: it says "something in here *may* point at a
    /// young object". Once a scan shows it does not, keeping it dirty costs the next minor
    /// `ENTRIES_PER_CARD` traces for nothing. Cleaning here is safe because the write barrier
    /// re-dirties on the next cross-gen write, and `dirty_cards_for_newly_old_*` re-dirties
    /// for the edges promotion creates in this same sweep — the two other ways the invariant
    /// can be broken (see the card-table invariant in the book).
    fn seed_from_dirty_cards(&self, queue: &mut Vec<Value>, threshold: u8) {
        let mut clean_obj: Vec<(u32, u8)> = Vec::new();
        {
            let region = self.region_object.lock();
            let mut cur: Option<(u32, u8, bool)> = None;
            region.iterate_dirty_cards(|h, entry, card| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                // SAFETY: handle came from iterate_dirty_cards; entry is alive and its
                // generation matches at iteration time.
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                let found = Self::seed_card_entry(Value::Object(gc), queue, threshold);
                Self::note_card(&mut cur, &mut clean_obj, h.chunk_idx, card, found);
            });
            Self::flush_card(cur, &mut clean_obj);
        }
        let mut clean_arr: Vec<(u32, u8)> = Vec::new();
        {
            let region = self.region_array.lock();
            let mut cur: Option<(u32, u8, bool)> = None;
            region.iterate_dirty_cards(|h, entry, card| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                // SAFETY: see above.
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                let found = Self::seed_card_entry(Value::Array(gc), queue, threshold);
                Self::note_card(&mut cur, &mut clean_arr, h.chunk_idx, card, found);
            });
            Self::flush_card(cur, &mut clean_arr);
        }
        {
            let mut region = self.region_object.lock();
            for (ci, card) in clean_obj {
                region.clean_card(ci, card);
            }
        }
        {
            let mut region = self.region_array.lock();
            for (ci, card) in clean_arr {
                region.clean_card(ci, card);
            }
        }
    }

    /// Push `v`'s young children onto the mark queue. Returns whether it had any — which is
    /// what decides if the card covering `v` still earns its place.
    /// Seed one entry of a dirty card, and report whether it still reaches anything young
    /// (which is what decides if its card keeps earning its place — see [`Region::clean_card`]).
    ///
    /// The two cases are **not** symmetric, and collapsing them was measurably wrong:
    ///
    /// - A **young** entry is pushed as a root, exactly as before. It is not a root by rights
    ///   — nothing says a young object in a dirty card is reachable — but dropping it changes
    ///   *what gets collected*, and this is a performance change. Measured: seeding only the
    ///   children cut the median minor pause a further 24% but cost **+6.3% peak RSS**
    ///   (581 → 618 MB at an 8 MB nursery). Precision is not free here; that trade belongs to
    ///   its own change with its own evidence.
    /// - An **old** entry is traced straight through, its young children queued. This is
    ///   exactly what the BFS did with it anyway — `fix-minor-stale-mark-on-old-roots` (#537)
    ///   made the loop skip marking old entries and trace through them — so doing it here is
    ///   the same work, minus a push and a pop, and it is what makes the card's verdict
    ///   available at all.
    fn seed_card_entry(v: Value, queue: &mut Vec<Value>, threshold: u8) -> bool {
        if Self::gen_age_of(&v) < threshold {
            // Young: root it (unchanged), and let the BFS trace it. Its own age already says
            // the card has something young in it.
            queue.push(v);
            return true;
        }
        let mut found = false;
        v.trace_children(&mut |child| {
            if Self::gen_age_of(child) < threshold {
                queue.push(child.clone());
                found = true;
            }
        });
        found
    }

    /// Fold one entry's verdict into the card it belongs to. `iterate_dirty_cards` walks a
    /// card's entries consecutively, so a single-slot accumulator is enough to know whether a
    /// card is clean by the time the next one starts.
    fn note_card(
        cur: &mut Option<(u32, u8, bool)>,
        out: &mut Vec<(u32, u8)>,
        ci: u32,
        card: u8,
        found: bool,
    ) {
        match cur {
            Some((c, k, any)) if *c == ci && *k == card => *any |= found,
            other => {
                Self::flush_card(*other, out);
                *other = Some((ci, card, found));
            }
        }
    }

    /// Record a finished card as cleanable when nothing in it reached anything young.
    fn flush_card(cur: Option<(u32, u8, bool)>, out: &mut Vec<(u32, u8)>) {
        if let Some((ci, card, any_young)) = cur {
            if !any_young {
                out.push((ci, card));
            }
        }
    }

    /// **fix-promotion-creates-uncarded-old-to-young (2026-09-08)**: the write barrier records
    /// an old→young edge **at the moment of the write**. Promotion creates such edges with no
    /// write at all: a parent allocated before its child ages out first, and the instant it
    /// crosses `PROMOTION_THRESHOLD` it is an old object holding a young one — with a clean
    /// card, because the store that put the child there was young→young.
    ///
    /// Measured shape: an old `Z42.IR.StrMap` (age 2) holding the bucket array it grew into
    /// later (age 1). The next minor did not root the map, nothing else reached the array, and
    /// the array was swept while still referenced — surfacing as
    /// `__str_hash_code: arg 0 expected string, got Null`.
    ///
    /// So promotion has to do the barrier's job: an entry that just became old and still
    /// refers to something young gets its card dirtied, exactly as a write would have.
    /// Only entries that actually cross the threshold are examined, once each, and only those
    /// with a young child dirty a card — dirtying every promoted entry's chunk would turn the
    /// next minor into a full-heap scan (cards are chunk-granular, 256 entries wide).
    fn dirty_cards_for_newly_old_objects(&self, handles: &[crate::gc::region::RegionHandle]) {
        if handles.is_empty() { return; }
        let mut to_dirty = Vec::new();
        {
            let region = self.region_object.lock();
            for &h in handles {
                let entry = region.resolve(h);
                // SAFETY: `promote` returned true for this handle in this same STW window, so
                // the entry is alive and its generation matches.
                let gc = unsafe {
                    GcRef::from_region_entry(std::ptr::NonNull::from(entry), h.generation)
                };
                if self.refers_to_young(&Value::Object(gc)) {
                    to_dirty.push((h.chunk_idx, h.entry_idx));
                }
            }
        }
        let mut region = self.region_object.lock();
        for (ci, ei) in to_dirty {
            region.mark_card_dirty(ci, ei);
        }
    }

    /// Array-region twin of [`Self::dirty_cards_for_newly_old_objects`].
    fn dirty_cards_for_newly_old_arrays(&self, handles: &[crate::gc::region::RegionHandle]) {
        if handles.is_empty() { return; }
        let mut to_dirty = Vec::new();
        {
            let region = self.region_array.lock();
            for &h in handles {
                let entry = region.resolve(h);
                // SAFETY: see the object twin.
                let gc = unsafe {
                    GcRef::from_region_entry(std::ptr::NonNull::from(entry), h.generation)
                };
                if self.refers_to_young(&Value::Array(gc)) {
                    to_dirty.push((h.chunk_idx, h.entry_idx));
                }
            }
        }
        let mut region = self.region_array.lock();
        for (ci, ei) in to_dirty {
            region.mark_card_dirty(ci, ei);
        }
    }

    /// Whether `v` has at least one child the minor GC would consider young. Stops at the
    /// first hit — this runs once per entry that crosses the promotion threshold.
    fn refers_to_young(&self, v: &Value) -> bool {
        let threshold = self.promotion_age;
        let mut found = false;
        v.trace_children(&mut |child| {
            if !found && Self::gen_age_of(child) < threshold {
                found = true;
            }
        });
        found
    }

    pub(super) fn sweep_phase_young_only(&self) -> MinorSweepResult {
        let mut freed_bytes: u64 = 0;
        let mut reclaimed_entries: usize = 0;
        let mut promoted_bytes: u64 = 0;

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
                    let fin = entry.take_finalizer();
                    tombstones_object.push((h, fin, size));
                }
            });
        }
        // Promote survivors (may remove some from young_list at threshold).
        let mut newly_old_object = Vec::new();
        for h in survivors_object {
            if self.region_object.lock().promote(h) {
                newly_old_object.push(h);
            }
        }
        // add-bounded-nursery: everything that just crossed into the old generation counts
        // towards the next major's trigger — see `promoted_bytes_since_major`.
        promoted_bytes += self.promoted_size_of_objects(&newly_old_object);
        self.dirty_cards_for_newly_old_objects(&newly_old_object);
        // Tombstone dead young entries.
        reclaimed_entries += tombstones_object.len();
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
                    for r in obj.refs_mut().iter_mut() {
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
                    let fin = entry.take_finalizer();
                    tombstones_array.push((h, fin, size));
                }
            });
        }
        let mut newly_old_array = Vec::new();
        for h in survivors_array {
            if self.region_array.lock().promote(h) {
                newly_old_array.push(h);
            }
        }
        promoted_bytes += self.promoted_size_of_arrays(&newly_old_array);
        self.dirty_cards_for_newly_old_arrays(&newly_old_array);
        reclaimed_entries += tombstones_array.len();
        for (h, fin, size) in tombstones_array {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            // unify-gc-heap PR-3: no eager element drop here — the array's element
            // storage lives in a `region_var` block (uniquely owned by this header),
            // reclaimed by `region_var.sweep()` (drop-glue drops the boxed Values) in
            // the same cycle. Tombstoning the header just releases the region_array slot.
            self.region_array.lock().tombstone(h);
        }

        // fix-minor-gc-skips-var-region (2026-09-08): the variable-length region — strings,
        // closures and every array's element storage, ~45% of RSS — used to sit out every
        // minor and wait for a major. It sweeps here with the other two now.
        //
        // This also settles the phantom-accounting half of the bug. The array header above
        // credits `array_size_estimate`, which includes `elem_storage_bytes()` — bytes that
        // live in a `region_var` block. That credit was a lie only because the block itself
        // survived the cycle; now that it is reclaimed in the same sweep, the account and the
        // memory move together. (Per `VarRegion::alloc_charge_bytes`, array element blocks
        // are charged zero on their own, so nothing is double-counted here.)
        {
            let (reclaimed, credited) = self.region_var.lock().sweep_young();
            freed_bytes += credited;
            reclaimed_entries += reclaimed;
        }

        // **add-bounded-nursery (2026-09-08)**: chunk-level reclaim at the minor tail.
        //
        // Without this a minor tombstones entries but never gives a chunk back, and only a
        // major (`sweep_phase`) called these. The TLAB hands each mutator a whole chunk at a
        // time, so a chunk filled with one burst of short-lived objects usually dies whole —
        // exactly the shape a minor produces, and exactly what these three calls recover.
        //
        // This, not the major-collection rate, is what made generational mode's footprint
        // explode: measured on `z42c.semantics --release --no-incremental` at a 128M budget,
        // peak RSS 986.3 MB without these three lines and **607.9 MB with them** (plain STW:
        // 606.7 MB), at the same 15 minors / 1 major.
        //
        // ⚠️ It is also the minor's dominant cost: the pass is O(heap), not O(young), so it
        // takes the median minor pause from ~30 ms to ~75 ms and puts a floor under what the
        // nursery size can buy. Making it incremental (only chunks this minor touched) is the
        // next lever — see the change's design notes.
        self.region_object.lock().reclaim_dead_chunks();
        self.region_array.lock().reclaim_dead_chunks();
        self.region_var.lock().reclaim_dead_var_chunks();
        self.promoted_bytes_since_major
            .fetch_add(promoted_bytes, std::sync::atomic::Ordering::Relaxed);
        MinorSweepResult { freed_bytes, reclaimed_entries, promoted_bytes }
    }

    /// Bytes the just-promoted object entries carry into the old generation. Uses the same
    /// estimator the sweep credits back, so promotion and reclamation speak one unit.
    fn promoted_size_of_objects(&self, handles: &[crate::gc::region::RegionHandle]) -> u64 {
        if handles.is_empty() { return 0; }
        let region = self.region_object.lock();
        handles.iter().map(|&h| {
            let entry = region.resolve(h);
            Self::script_object_size_estimate(&entry.value.lock())
        }).sum()
    }

    /// Array-region twin of [`Self::promoted_size_of_objects`].
    fn promoted_size_of_arrays(&self, handles: &[crate::gc::region::RegionHandle]) -> u64 {
        if handles.is_empty() { return 0; }
        let region = self.region_array.lock();
        handles.iter().map(|&h| {
            let entry = region.resolve(h);
            Self::array_size_estimate(&entry.value.lock())
        }).sum()
    }

    /// **add-generational-gc P2 (2026-05-22)**: full minor GC cycle.
    /// Mark phase (young + dirty cards) → sweep phase (young only) →
    /// returns freed_bytes estimate. Card dirty bits are NOT cleared
    /// here — they accumulate across minors and are only cleared by
    /// the next major GC. This preserves correctness for stable
    /// old→young references (whose cards were dirtied at the time of
    /// the write but the target young object hasn't yet been promoted).
    pub(super) fn run_cycle_collection_minor(&self) -> MinorSweepResult {
        // add-gc-tlab (stage 2, D5): retire the collecting thread's own TLAB so
        // its freshly-allocated (young, still-borrowed) chunk is merged into the
        // region before the minor mark/sweep — otherwise those young objects sit
        // in a borrowed chunk skipped by iteration. Idempotent when unbound.
        self.retire_thread_tlab();
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
        // Major scanned the whole heap → every card the old set accumulated is stale. Clear,
        // then **rebuild from the surviving graph**.
        //
        // fix-promotion-creates-uncarded-old-to-young (2026-09-08): the blanket clear on its
        // own dropped every old→young edge that outlived the major. A major does not promote
        // (it only marks and sweeps), so young objects are still young afterwards and old
        // objects still point at them — with, after the clear, no card. The next minor then
        // had no way to re-root those owners and swept their children. Same defect as the
        // promotion one, through a different door: the card table's invariant is
        // **"an old entry referring to anything young has a dirty card"**, and it has to be
        // re-established here rather than assumed away.
        self.rebuild_card_table();
        // add-bounded-nursery: the old generation was just fully swept, so the budget that
        // decides when to sweep it again starts over.
        self.promoted_bytes_since_major
            .store(0, std::sync::atomic::Ordering::Relaxed);
        freed
    }

    /// Clear every card, then re-dirty the chunk of each live **old** entry that still refers
    /// to something young. Runs at the tail of a major, under STW.
    ///
    /// Cost is one `trace_children` per live old entry, once per major — majors are single
    /// digits per compiler build, against 10–20 minors that each get a minimal dirty set out
    /// of it. Keeping the pre-major cards instead would be correct but monotonic: the dirty
    /// set would only grow and minors would drift towards full-heap scans.
    fn rebuild_card_table(&self) {
        let threshold = self.promotion_age;
        let mut obj_chunks: Vec<(u32, u16)> = Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_alive(|h, e| {
                if e.gen_age() < threshold { return; }
                // SAFETY: `iterate_alive` only yields alive entries whose generation matches.
                let gc = unsafe {
                    GcRef::from_region_entry(std::ptr::NonNull::from(e), h.generation)
                };
                if self.refers_to_young(&Value::Object(gc)) {
                    obj_chunks.push((h.chunk_idx, h.entry_idx));
                }
            });
        }
        let mut arr_chunks: Vec<(u32, u16)> = Vec::new();
        {
            let region = self.region_array.lock();
            region.iterate_alive(|h, e| {
                if e.gen_age() < threshold { return; }
                // SAFETY: see above.
                let gc = unsafe {
                    GcRef::from_region_entry(std::ptr::NonNull::from(e), h.generation)
                };
                if self.refers_to_young(&Value::Array(gc)) {
                    arr_chunks.push((h.chunk_idx, h.entry_idx));
                }
            });
        }
        {
            let mut region = self.region_object.lock();
            region.clear_card_dirty();
            for (ci, ei) in obj_chunks {
                region.mark_card_dirty(ci, ei);
            }
        }
        {
            let mut region = self.region_array.lock();
            region.clear_card_dirty();
            for (ci, ei) in arr_chunks {
                region.mark_card_dirty(ci, ei);
            }
        }
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
        if new_age >= self.promotion_age {
            return;
        }
        match owner {
            // add-boxed-struct-identity (P4b): a boxed struct owner is a region_object
            // entry too (reflection SetValue writes a ref leaf into its struct_refs).
            Value::Object(gc) | Value::BoxedStruct(gc) => {
                if GcRef::gen_age(gc) < self.promotion_age { return; }
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
                if GcRef::gen_age(gc) < self.promotion_age { return; }
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
