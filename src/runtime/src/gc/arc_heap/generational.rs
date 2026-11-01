//! `ArcMagrGC` 分代 GC：minor / major / promotion + gen_age（写屏障在 `barrier.rs`，记忆集在
//! `remembered.rs`）。从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use crate::object::header::marks::MarkKind;
use crate::object::Value;
use crate::object::ArrRef;
use crate::object::header::obj_ref::ObjRef;
use crate::gc::phase_timer::PhaseTimer;

/// What one minor collection reclaimed. `reclaimed_entries` is what the escalation heuristic
/// needs: **the number of young blocks the sweep actually tombstoned**. Survival can only be read off that — see `collect_cycles_with_context`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MinorSweepResult {
    pub(crate) freed_bytes: u64,
    pub(crate) reclaimed_entries: usize,
    /// Bytes this minor moved into the old generation (add-bounded-nursery).
    pub(crate) promoted_bytes: u64,
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// Young blocks listed — what the next minor will have to scan before it sees a single new
    /// allocation (add-pause-budget-nursery).
    pub(super) fn young_count(&self) -> usize {
        self.region_var.lock().young_count()
    }

    /// The heap's current promotion age. A relaxed load: it only changes inside a STW
    /// sweep, so every reader either sees the whole old value or the whole new one.
    #[inline]
    pub(super) fn promotion_age(&self) -> u8 {
        self.promotion_age.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// **adaptive-promotion **: put a new promotion age in force across the heap and
    /// the region's cached copy, and remember the cohort the new line made old.
    ///
    /// The **major** path's switch (an open cycle closing, or a one-shot major); a minor applies it
    /// with targeted bookkeeping instead — [`Self::lower_line_after_minor`]. Both halves here are
    /// load-bearing:
    ///
    /// - **A major is the only self-consistent moment.** Lowering the line redefines which
    ///   blocks are old, and every invariant a minor leans on — the remembered set, the mark
    ///   bits, which blocks are listed young — was built under the old definition. A major
    ///   has just marked the whole heap from the roots, swept every block and re-aged every
    ///   survivor, so nothing is left that was derived from the old line. Doing it inside a
    ///   minor sweep instead is measurably wrong: live `Z42.Semantics.MethodSymbol`s and
    ///   `Z42.IR.StrMap` backings get reclaimed, and z42c then reports `E0401: no method` or
    ///   dies on `ArrayGet: expected array, got Null`.
    /// - **The remembered set has to be topped up even so.** It records old→young edges, and
    ///   the write barrier deliberately records *nothing* for a young owner (a young owner is
    ///   found by scanning the young generation). The new line reclassifies a cohort as old, so
    ///   every edge those blocks wrote while they counted as young is an old→young edge nothing
    ///   recorded — the cohort's owners with a young child are remembered here.
    fn apply_promotion_age(&self, age: u8) {
        self.promotion_age.store(age, std::sync::atomic::Ordering::Relaxed);
        self.region_var.lock().set_promotion_age(age);
        self.remember_lowered_cohort();
    }

    /// Apply a lower promotion line the minor that just finished settled, **without a major**.
    /// The caller holds the pause and has completed the minor (mark and both sweeps).
    ///
    /// What the major used to provide is a moment where nothing derived from the old line is
    /// inconsistent; the end of a complete minor is such a moment too, as long as no cycle is open:
    ///
    /// - **Young set**: every listed entry survived this minor and was just aged; the ones at or
    ///   past the new line are delisted here (no aging — they have aged once already).
    /// - **Marks**: the sweep cleared the minor mark of every young survivor; old entries carry
    ///   none (`fix-minor-stale-mark-on-old-roots`).
    /// - **Old → young edges**: only the delisted cohort can hold an unrecorded one (an entry old
    ///   under the old line had its edges recorded by the barrier or by promotion, and a child
    ///   young under the new line was young under the old one). So the cohort gets exactly the
    ///   promotion barrier — a remembered-set entry per object / array that refers to something
    ///   young.
    ///
    /// With a cycle open the switch waits for the cycle's close (`close_incremental_cycle`): a
    /// sweeping cycle requires everything that leaves the young set to carry its epoch, which the
    /// close does not have to reason about.
    fn lower_line_after_minor(&self) {
        if !crate::config::runtime_config().gc_adaptive_promotion
            || self.major_cycle_active()
            || !self.promotion_policy.apply()
        {
            return;
        }
        let lowered = self.configured_promotion_age().saturating_sub(1).max(1);
        let t = PhaseTimer::start("promotion/lower after minor");
        self.promotion_age.store(lowered, std::sync::atomic::Ordering::Relaxed);
        self.region_var.lock().set_promotion_age(lowered);
        let owners = self.region_var.lock().delist_past_line();
        self.remember_newly_old(&owners);
        t.count(owners.len());
        crate::gc::phase_timer::note(format_args!(
            "promotion  age {} -> {lowered} after a minor  (cohort: {} objects / arrays)",
            self.configured_promotion_age(), owners.len()));
    }

    /// [`Self::apply_promotion_age`] for tests that lower the line by hand.
    #[cfg(test)]
    pub(crate) fn apply_promotion_age_for_test(&self, age: u8) {
        self.apply_promotion_age(age);
    }

    /// The age this heap was configured with — the line the adaptive policy lowers *from*.
    #[inline]
    pub(super) fn configured_promotion_age(&self) -> u8 {
        self.configured_promotion_age
    }

    /// **add-generational-gc P2 **: read the gen_age of
    /// any `Value`. Returns 0 for primitives + stack refs (irrelevant
    /// to generational dispatch — mark/sweep already handles those).
    pub(super) fn gen_age_of(v: &Value) -> u8 {
        match v {
            Value::Object(gc) => ObjRef::gen_age(gc),
            Value::Array(gc)  => ArrRef::gen_age(gc),
            // fix-minor-gc-skips-var-region : the closure block carries its own
            // age now, so report *its* age rather than its `env` array's.
            //
            // This is load-bearing, not tidiness. Reporting 0 for a block the minor sweep
            // never visits left a **stale mark bit** on it: the next minor popped the same
            // closure, its `mark()` CAS failed, `just_marked` came back false — and its
            // children were therefore never traced. A still-young `env` array reachable only
            // through that closure went unmarked and was swept out from under it.
            Value::Closure(c) => c.gen_age(),
            // Strings are var blocks too. They used to fall through to `_ => 0`
            // (always "young"), so every reachable string was re-marked at every minor and
            // kept a stale mark. They are leaves, so that only cost floating garbage rather
            // than correctness — but there is no reason to pay it now that the age is real.
            Value::Str(s) => s.gen_age(),
            // add-boxed-struct-identity (P4b): gen-age of the boxed struct's shared object.
            Value::BoxedStruct(gc) => ObjRef::gen_age(gc),
            // make-value-copy: `Ref` handle carries no direct heap allocation to age
            // (its target is aged via the arena root scan) → 0.
            _ => 0,
        }
    }

    /// **add-generational-gc P2 **: mark phase for minor GC.
    ///
    /// Roots = pinned roots + external_root_scanner output + the remembered set (every old
    /// object / array that has received a young reference, or became old holding one, since it
    /// last stopped reaching anything young).
    ///
    /// BFS marks every reachable young block. When tracing children, only young children
    /// (gen_age < promotion age) are pushed to the queue. Old children are skipped — either they
    /// have no young descendants (otherwise the barrier / promotion would have remembered them),
    /// or their old→young paths are seeded from the remembered set. This bounds minor mark work at
    /// O(young + |remembered|).
    pub(super) fn mark_phase_minor(&self) -> usize {
        self.maybe_verify_cards_before_minor();
        let threshold = self.promotion_age();
        let mut queue: Vec<Value> = Vec::new();

        // Pinned roots + strong GC handles + external scanner.
        {
            let i = self.inner.lock();
            queue.extend(i.roots.values().cloned());
            // fix-strong-handles-are-not-roots: a strong handle anchors its target, so it is a
            // root here exactly as a pinned root is. Old targets are traced-through rather than
            // marked, same as any other old root (see the note in the BFS below).
            queue.extend(i.handle_slab.strong_targets());
        }
        // add-incremental-major-gc M2a: a major mark's grey set and SATB records are roots for a
        // minor that runs while that mark is outstanding — otherwise a young object the barrier
        // recorded could be swept here and handed back to the marker as a dangling handle.
        // trim-minor-cycle-roots: **only the young ones**, see [`Self::seed_cycle_queue`].
        // add-pause-budget-nursery: a minor's pause is only *buyable* with the nursery to the
        // extent that its work is proportional to the young set. These notes say how much of it
        // is not — see [`super::pause_budget`].
        let pinned_n = queue.len();
        let satb_old = Self::seed_cycle_queue(&mut queue, &self.satb_queue.lock(), threshold);
        let satb_n = queue.len() - pinned_n;
        let grey_old = Self::seed_cycle_queue(&mut queue, &self.mark_queue.lock(), threshold);
        let grey_n = queue.len() - pinned_n - satb_n;
        crate::gc::phase_timer::note(format_args!(
            "  minor roots  pinned+handles {pinned_n}, satb {satb_n} (skipped old {satb_old}), \
grey {grey_n} (skipped old {grey_old})"));
        {
            let scanner = self.external_root_scanner.lock();
            if let Some(scan) = scanner.as_ref() {
                scan(&mut |v| queue.push(v.clone()));
            }
        }

        // The remembered set: every old owner of a young reference is traced through here (its
        // young children queued), and dropped from the set once it reaches nothing young.
        let remembered = self.seed_from_remembered(&mut queue, threshold);
        crate::gc::phase_timer::note(format_args!("  remembered seed  {remembered} blocks"));

        let mut marked = 0usize;
        let (mut visited, mut children, mut old_traced) = (0usize, 0usize, 0usize);
        // Each value waits in the prefetching ring (`mark_ring`) before it is marked and traced,
        // so `RING` header misses are in flight at a time.
        let mut ring = super::mark_ring::MarkRing::new();
        loop {
            let v = match queue.pop() {
                Some(v) => match ring.push(v) {
                    Some(v) => v,
                    None => continue,
                },
                None => match ring.pop_oldest() {
                    Some(v) => v,
                    None => break,
                },
            };
            visited += 1;
            // fix-minor-stale-mark-on-old-roots : **only young entries are
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
            // remembered set) and old *children* are never enqueued below, so tracing them
            // unmarked still terminates; the only cost is re-tracing an old object that
            // appears in the root set twice, which is O(its fields), not O(its subgraph).
            let young = Self::gen_age_of(&v) < threshold;
            if young {
                if !Self::mark_if_unmarked(&v, MarkKind::Minor) { continue; }
                marked += 1;
            } else {
                old_traced += 1;
            }

            v.visit_gc_children(young.then_some(MarkKind::Minor), &mut |child| {
                children += 1;
                // Only enqueue **young heap references**. Old children that need re-rooting are
                // already covered by the remembered set.
                //
                // **fix-primitives-count-as-young **: `is_heap_ref()` is load-bearing,
                // not tidiness. `gen_age_of` answers 0 for everything that is not a GC reference
                // — `Null`, `I64`, stack handles — and 0 is `< threshold`, so **every primitive
                // field and every empty slot counted as "a young object"** and was cloned onto
                // the mark queue. Measured on `z42c.semantics`: 1 254 047 of the 1 254 097 values
                // this loop enqueued per minor were primitives, all of which `mark_if_unmarked`
                // then rejected. `is_heap_ref` is the same predicate the write barrier asserts on.
                if child.is_heap_ref() && Self::gen_age_of(child) < threshold {
                    queue.push(child.clone());
                }
            });
        }
        crate::gc::phase_timer::note(format_args!(
            "  minor bfs  popped {visited} (old {old_traced}), children {children}, marked {marked}"));
        marked
    }

    /// **trim-minor-cycle-roots **: seed the **young** entries of one of an open
    /// cycle's queues (`mark_queue` / `satb_queue`) as minor roots, and report how many old ones
    /// were skipped.
    ///
    /// M2a made both queues minor roots wholesale, which is more than the rule needs. What the
    /// rule protects is a *young* entry: a minor reclaims young entries, so one the marker is
    /// already holding would be swept out from under it. An **old** entry cannot be reclaimed
    /// here at all, so rooting it buys exactly one thing — one level of tracing — and that level
    /// is already covered. For any old entry `X`, exactly one of:
    ///
    /// - **`X` is remembered** → [`Self::seed_from_remembered`] traces `X` and queues its young
    ///   children (`seed_old_owner`'s old arm). The grey push is *duplicate work*, and it is not
    ///   cheap: measured on `13_gc_large_heap --large`, **118 690** old entries copied and re-traced
    ///   on *every* minor, 466 629 of the 683 092 grey roots across the run.
    /// - **`X` is not remembered** → an owner leaves the set only once it reaches nothing young,
    ///   and both paths that can create an old→young edge put it back (`maybe_remember_owner` on
    ///   the write, `remember_newly_old` on promotion). So `X` has no young children and tracing
    ///   it produces nothing.
    ///
    /// This is the **same invariant the BFS below already runs on** — it is why old *children* are
    /// never enqueued. Two premises it rests on, both load-bearing:
    ///
    /// 1. *A minor never reclaims a block that is old by age.* `VarRegion::sweep_young_with` walks
    ///    the young set only, and guards the case of a listed block already past a lowered line
    ///    explicitly (`fix-old-block-left-in-young-list`).
    /// 2. *Only objects and arrays are remembered* (`maybe_remember_owner`), so premise 1 of the
    ///    argument above is checked for the other block kinds instead: `Str` is a leaf (`FuncRef`
    ///    is not a heap value); a closure's `env` is fixed at construction and allocated before it,
    ///    so it ages in lockstep (every minor that reaches the closure marks the env) and an old
    ///    closure's env is old — and the env is an *array*, which is remembered like any other.
    ///
    /// The queues themselves are untouched — the marker still gets every entry it recorded, so
    /// the SATB snapshot promise is unaffected. This only narrows what the *minor* seeds.
    fn seed_cycle_queue(queue: &mut Vec<Value>, cycle: &[Value], threshold: u8) -> usize {
        let mut skipped_old = 0usize;
        for v in cycle {
            if Self::gen_age_of(v) < threshold {
                queue.push(v.clone());
            } else {
                skipped_old += 1;
            }
        }
        skipped_old
    }

    /// Seed one remembered owner, and report whether it still reaches anything young (which is
    /// what decides if it keeps its place in the remembered set).
    ///
    /// - A **young** owner (one the set still lists after a tenure / lowered line made it old and
    ///   back — or a test) is pushed as a root, and its own age already says it has something
    ///   young in it.
    /// - An **old** owner is traced straight through, its young children queued — exactly what the
    ///   BFS does with an old root anyway (`fix-minor-stale-mark-on-old-roots`, #537: old entries
    ///   are traced through, never marked), minus a push and a pop, and it is what makes the
    ///   verdict available at all.
    pub(super) fn seed_old_owner(v: Value, queue: &mut Vec<Value>, threshold: u8) -> bool {
        if Self::gen_age_of(&v) < threshold {
            // Young: root it, and let the BFS trace it.
            queue.push(v);
            return true;
        }
        let mut found = false;
        // Old entry: traced through, no marks (see the note in `mark_phase_minor`).
        v.visit_gc_children(None, &mut |child| {
            // fix-primitives-count-as-young : heap references only — see the note in
            // `mark_phase_minor`. Without the `is_heap_ref` guard a `Null` slot made `found` true,
            // so the owner never left the set and was rescanned at every minor forever.
            if child.is_heap_ref() && Self::gen_age_of(child) < threshold {
                queue.push(child.clone());
                found = true;
            }
        });
        found
    }

    // **fix-promotion-creates-uncarded-old-to-young **: the write barrier records
    // an old→young edge **at the moment of the write**. Promotion creates such edges with no
    // write at all: a parent allocated before its child ages out first, and the instant it
    // crosses the promotion age it is an old block holding a young one — unremembered, because
    // the store that put the child there was young→young. Measured shape: an old
    // `Z42.IR.StrMap` (age 2) holding the bucket array it grew into later (age 1), swept while
    // still referenced (`__str_hash_code: arg 0 expected string, got Null`). So promotion does the
    // barrier's job: an object or array that just became old and still refers to something young
    // is remembered (`remember_newly_old`), exactly as a write would have done.

    /// Whether `v` has at least one child the minor GC would consider young. Stops at the
    /// first hit — this runs once per entry that crosses the promotion threshold.
    pub(super) fn refers_to_young(&self, v: &Value) -> bool {
        let threshold = self.promotion_age();
        let mut found = false;
        // Inspection only — no marks.
        v.visit_gc_children(None, &mut |child| {
            // fix-primitives-count-as-young : heap references only.
            // `gen_age_of(Value::Null)` is 0, which is `< threshold`, so without the guard any owner
            // with a single empty or primitive slot "refers to something young" and the old→young
            // record never shrinks (measured 33 001 records covering 203 884 owners per minor, to
            // find ~100 young objects; with the guard, 1–3).
            if !found && child.is_heap_ref() && Self::gen_age_of(child) < threshold {
                found = true;
            }
        });
        found
    }

    /// **add-generational-gc P2 **: sweep phase for minor GC. Walks the young set;
    /// for each block:
    /// - marked → clear the mark, age it; at the promotion age it leaves the young set;
    /// - unmarked → tombstone it (its finalizer fires through the flagged-dead list).
    ///
    /// Old blocks are not visited — a major handles them. The remembered set is left alone (a
    /// stable old→young reference keeps its owner remembered until the child is promoted).
    pub(super) fn sweep_phase_young_only(&self) -> MinorSweepResult {
        let mut freed_bytes: u64 = 0;
        let mut reclaimed_entries: usize = 0;
        let mut promoted_bytes: u64 = 0;
        // **adaptive-promotion **: the age in force. The switch itself happens on
        // top of a major (see `promotion_policy`), never here — a sweep is the wrong place to
        // redefine which entries are old.
        let configured = self.configured_promotion_age();
        let threshold = self.promotion_age();
        let observed_age = self.promotion_policy.observed_age(configured);
        // The young generation is the minor's: only what it reached survives, whatever epoch an
        // entry carries (P1-7). While the cycle sweeps, everything it promotes must carry the
        // epoch — checked in debug builds (`promote_black`).
        let promote_black = self.doomed_unless_marked();

        // One walk of the young set scans, promotes and tombstones every young block — objects
        // and arrays feed the promotion policy their survival sample, and the ones that just
        // became old come back (they may need remembering).
        {
            let t = PhaseTimer::start("minor/var sweep");
            let var = self.region_var.lock().sweep_young_with(
                promote_black,
                observed_age,
                |marked| self.promotion_policy.observe(marked),
            );
            t.count(var.reclaimed);
            freed_bytes += var.credited;
            reclaimed_entries += var.reclaimed;
            promoted_bytes += var.promoted_bytes;
            self.remember_newly_old(&var.newly_old);
        }

        // **add-bounded-nursery **: chunk-level reclaim at the minor tail.
        //
        // Without this a minor tombstones blocks but never gives a chunk back, and only a
        // major (`sweep_phase`) did. The TLAB hands each mutator a whole chunk at a
        // time, so a chunk filled with one burst of short-lived objects usually dies whole —
        // exactly the shape a minor produces, and exactly what these three calls recover.
        //
        // This, not the major-collection rate, is what made generational mode's footprint
        // explode: measured on `z42c.semantics --release --no-incremental` at a 128M budget,
        // peak RSS 986.3 MB without the reclaim and **607.9 MB with it** (plain STW:
        // 606.7 MB), at the same 15 minors / 1 major.
        //
        // ⚠️ It is also the minor's dominant cost: the pass is O(heap), not O(young), so it
        // takes the median minor pause from ~30 ms to ~75 ms and puts a floor under what the
        // nursery size can buy. Making it incremental (only chunks this minor touched) is the
        // next lever — see the change's design notes.
        {
            let _t = PhaseTimer::start("minor/chunk reclaim");
            self.reclaim_dead_chunks_and_measure(false);
        }
        let (live, total, lowered) = self.promotion_policy.snapshot();
        crate::gc::phase_timer::note(format_args!(
            "promotion  age {threshold}  tier {observed_age} survived {live}/{total}  lowered {lowered}"));
        self.promotion_policy.settle();
        self.promoted_bytes_since_major
            .fetch_add(promoted_bytes, std::sync::atomic::Ordering::Relaxed);
        MinorSweepResult { freed_bytes, reclaimed_entries, promoted_bytes }
    }

    /// **add-generational-gc P2 **: full minor GC cycle.
    /// Mark phase (young + remembered set) → sweep phase (young only) → returns the freed bytes.
    pub(super) fn run_cycle_collection_minor(&self) -> MinorSweepResult {
        // add-gc-tlab (stage 2, D5): retire the collecting thread's own TLAB so
        // its freshly-allocated (young, still-borrowed) chunk is merged into the
        // region before the minor mark/sweep — otherwise those young objects sit
        // in a borrowed chunk skipped by iteration. Idempotent when unbound.
        self.retire_thread_tlab();
        let _newly_marked = {
            let t = PhaseTimer::start("minor mark");
            let n = self.mark_phase_minor();
            t.count(n);
            n
        };
        let swept = {
            let _t = PhaseTimer::start("minor sweep");
            self.sweep_phase_young_only()
        };
        // adaptive-promotion: a lower line this minor settled goes into force now, not at a major.
        self.lower_line_after_minor();
        swept
    }

    /// **add-generational-gc P3 **: full major GC cycle.
    /// Same as `run_cycle_collection_stw` (mark the whole heap from the roots; sweep every block),
    /// then the young survivors are aged.
    pub(super) fn run_cycle_collection_major(&self) -> u64 {
        let freed = self.run_cycle_collection_stw();
        // **fix-minor-and-major-in-one-pause **: age the young survivors, exactly as
        // the minor sweep does. A major is a superset collection, so surviving one is as much
        // evidence of longevity as surviving a minor — and aging is the only thing that drains
        // the young list. Before escalation was deferred a major always had a minor in front of
        // it, which did the aging; now that a major can run alone, it has to do it itself, or
        // every live entry stays young and the **next** minor re-marks the whole heap
        // (measured on `09_alloc_ctorless`: 1 498 866 entries, 192.6 ms).
        //
        // **adaptive-promotion **: the switch goes **here** — after the whole-heap
        // mark and sweep, and **before** the aging. See [`Self::apply_promotion_age`] for why
        // a major is the only self-consistent moment, and why the aging has to be the first
        // thing to run under the new line: it is what drains the entries the new line makes
        // old out of the young set (and remembers them). Applying it *after* the aging
        // instead leaves those entries listed young while the mark phase treats them as old —
        // measured, that is five different `got Null` failures across a cold `package sdk`.
        self.apply_adaptive_promotion_after_major(false);
        {
            let _t = PhaseTimer::start("age survivors");
            self.age_survivors_after_major();
        }
        // The remembered set is kept across the major (it is a superset of what a rebuild would
        // produce: the barrier and promotion maintain it, and a minor drops every owner that no
        // longer reaches anything young), and the aging above remembers what it makes old.
        // add-bounded-nursery: the old generation was just fully swept, so the budget that
        // decides when to sweep it again starts over.
        self.promoted_bytes_since_major
            .store(0, std::sync::atomic::Ordering::Relaxed);
        freed
    }

    /// Put a promotion-age switch the adaptive policy has asked for into force on top of the major
    /// that just completed (see [`Self::apply_promotion_age`]).
    ///
    /// `age_now`: the incremental major has no aging pass of its own (minors age between its
    /// slices), but the switch still has to be followed by one — it is what drains the entries the
    /// lowered line makes old out of the young lists. The one-shot major ages right after anyway.
    pub(super) fn apply_adaptive_promotion_after_major(&self, age_now: bool) {
        if !(crate::config::runtime_config().gc_adaptive_promotion
            && self.promotion_policy.apply())
        {
            return;
        }
        let lowered = self.configured_promotion_age().saturating_sub(1).max(1);
        crate::gc::phase_timer::note(format_args!(
            "promotion  age {} -> {lowered}", self.promotion_age()));
        self.apply_promotion_age(lowered);
        if age_now {
            self.age_survivors_after_major();
        }
    }

    /// **fix-minor-and-major-in-one-pause **: the major's counterpart of the aging
    /// half of [`Self::sweep_phase_young_only`]. Entries were already swept by `sweep_phase`;
    /// this only bumps the survivors' ages and lets those that cross the threshold leave the
    /// young set. Newly-old objects and arrays still holding young references are remembered,
    /// the same way promotion does in a minor (`fix-promotion-creates-uncarded-old-to-young`).
    fn age_survivors_after_major(&self) {
        // **one-pass-major-aging **: one walk of the young set.
        let newly_old = self.region_var.lock().age_young_survivors();
        self.remember_newly_old(&newly_old);
    }

    /// **add-generational-gc P3 **: escalation threshold.
    /// If the fraction of young entries surviving a minor GC exceeds
    /// this, the next collect is escalated to major immediately.
    /// Default 0.75 from [`RuntimeConfig::gc_minor_threshold`]; override
    /// via `Z42_GC_MINOR_THRESHOLD`.
    ///
    /// runtime-config-phase2 : centralised through
    /// `crate::config::runtime_config()`; previous per-callsite
    /// `OnceLock<f32>` retired.
    #[allow(dead_code)] // wired in collect_cycles_with_context below
    pub(super) fn minor_escalation_threshold() -> f32 {
        crate::config::runtime_config().gc_minor_threshold
    }
}
