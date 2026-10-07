//! `ArcMagrGC` 环回收编排与控制 API：run_cycle_collection(_stw) + collect/force + finalize + soft-ref。
//! mark/sweep 原语见 `collect.rs`（refactor-arc-heap-modularization）。

use crate::gc::heap::MagrGC;
use crate::metadata::{ScriptObject, Value};
use crate::metadata::types::{ArrayObj};
use crate::gc::refs::{GcRef};
use crate::gc::types::{CollectStats, GcEvent, GcKind};
use crate::gc::phase_timer::PhaseTimer;
use super::incremental::{GenWork, SliceBudget};

impl crate::gc::arc_heap::ArcMagrGC {
    /// Cycle collection — mark-sweep.
    ///
    /// 1. **Mark**：BFS from pinned roots + external scanner, setting
    ///    `marked = 1` on every reachable `GcAllocation`.
    /// 2. **Sweep**：snapshot live objects from registry; reset marks on
    ///    survivors, break internal refs of unmarked allocations so that
    ///    when the snapshot `Vec` drops the Arc strong counts can reach
    ///    zero and chain-drop fires finalizers.
    ///
    /// Returns the estimated `freed_bytes` (sum of `object_size_bytes`
    /// for broken cycle nodes).
    ///
    /// **add-mark-sweep-collector P3 (2026-05-21)**: replaced the
    /// previous trial-deletion (Bacon-Rajan simplified) implementation.
    /// O(N²) → O(reachable). The pure tracing contract: Rust-local
    /// `Value` strong refs are NOT roots — embedders must `pin_root`
    /// anything they want preserved across collect.
    ///
    /// Dispatches on `self.mode()`.
    pub(super) fn run_cycle_collection(&self) -> u64 {
        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => self.run_cycle_collection_stw(),
            crate::gc::GcMode::GenerationalMarkSweep => {
                // add-generational-gc P2: minor GC by default. Major
                // GC requires the VmContext-aware entry
                // (`collect_cycles_with_context`) for pause coord;
                // direct callers of `collect_cycles` (which go through
                // `force_collect`) get a minor cycle here. Major is
                // P3's expansion (auto-collect young pressure trigger
                // + escalation heuristic).
                self.run_cycle_collection_minor().freed_bytes
            }
        }
    }

    /// STW mark-sweep collect — the one-shot whole-heap path. Called when
    /// `mode() == StwMarkSweep`.
    ///
    /// Assumes a clean slate at start: a fresh epoch (every slot white) and an
    /// empty `mark_queue`. A pre-marked entry would fail the mark CAS, so its
    /// children would go untraced and could be swept while still referenced.
    pub(super) fn run_cycle_collection_stw(&self) -> u64 {
        // add-gc-tlab (stage 2, D5): retire the collecting thread's own TLAB
        // before marking, so its just-allocated (still-borrowed) chunk is merged
        // into the region and participates in mark/sweep + mark-clearing. Other
        // mutators retired at their safepoint park; in the no-context / force /
        // cargo-direct paths (no safepoint) this is the sole retire that keeps a
        // borrowed chunk from being skipped by sweep. Idempotent when unbound.
        self.retire_thread_tlab();
        // add-incremental-major-gc M2b: a one-shot cycle cannot start inside an open incremental one
        // (both would own the epoch and the SATB registration) — only reachable across a mode change.
        self.finish_major_cycle("one-shot major requested");
        // add-incremental-major-gc M1: no reset pass — a new epoch whitens every slot, including
        // anything marked outside a cycle (see `MarkKind`).
        self.open_major_cycle();
        self.mark_queue.lock().clear();
        let _newly_marked = {
            let t = PhaseTimer::start("full mark");
            let n = self.mark_phase();
            t.count(n);
            n
        };
        // add-incremental-major-gc M2a: grey what the SATB barrier recorded; stop recording.
        self.close_major_marking();
        // **add-gc-softref (2026-05-26)**: revive soft-ref targets that
        // are unmarked but below the pressure threshold.
        self.revive_soft_refs();
        // **add-lazy-context-unload (2026-08-05)**: after mark (marks set),
        // before sweep (which clears them), scan marked objects for retained
        // collectible contexts. Gated on `is_unloading` → zero cost normally.
        let ctx_snapshot = {
            let g = self.context_reclaimer.lock();
            match g.as_ref() {
                Some(r) if r.is_unloading() => Some(r.snapshot()),
                _ => None,
            }
        };
        let live_contexts = ctx_snapshot.as_ref().map(|s| self.scan_marked_contexts(s));
        let freed = {
            let _t = PhaseTimer::start("sweep");
            self.sweep_phase()
        };
        // Reclaim Unloading contexts with no live references (post-sweep, STW).
        if let Some(live) = live_contexts {
            if let Some(r) = self.context_reclaimer.lock().as_ref() {
                r.reclaim(&live);
            }
        }
        // Prune dead soft-ref entries after sweep.
        self.inner.lock().soft_registry.prune_dead();
        freed
    }

    /// What [`Self::run_cycle_collection`] runs in the current mode: a generational heap's
    /// one-shot entry runs a **minor** (see there), every other mode a whole-heap collection.
    fn one_shot_kind(&self) -> GcKind {
        if self.mode() == crate::gc::GcMode::GenerationalMarkSweep { GcKind::Minor } else { GcKind::Major }
    }

    /// Count one completed pause of `kind` (plus, for a generational `force_collect`, the cycle it
    /// finished on the way).
    fn count_collection(stats: &mut crate::gc::types::HeapStats, kind: GcKind, finished_cycle: bool) {
        stats.gc_cycles += 1;
        match kind {
            GcKind::Minor => stats.minor_collections += 1,
            GcKind::Major => stats.major_collections += 1,
            GcKind::Slice => {}
        }
        if finished_cycle && kind != GcKind::Major {
            stats.major_collections += 1;
        }
    }

    pub(super) fn collect_cycles(&self) {
        if self.inner.lock().pause_count > 0 { return; }
        let start = Self::now_us();
        let used_before = self.used_bytes_atomic(); // add-gc-tlab (option B)
        let kind = self.one_shot_kind();
        self.fire_event(GcEvent::BeforeCollect { kind, used_bytes: used_before });
        let freed_bytes = self.run_cycle_collection();
        {
            let mut i = self.inner.lock();
            Self::count_collection(&mut i.stats, kind, false);
            i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
            self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
        }
        // Phase 3d: 若 used 已降到 90% 阈值以下，重置 near_limit_warned
        self.maybe_reset_near_limit_warned();
        let pause_us = Self::now_us().saturating_sub(start);
        self.pause_histogram.lock().record(pause_us);
        self.fire_event(GcEvent::AfterCollect { kind, freed_bytes, pause_us });
        // **add-gc-debug-invariants P1 (2026-05-22)**: post-collect
        // invariant check. Release builds compile this out entirely.
        #[cfg(debug_assertions)]
        self.debug_validate_invariants();
    }

    pub(super) fn force_collect(&self) -> CollectStats {
        if self.inner.lock().pause_count > 0 {
            return CollectStats::default();
        }
        let start = Self::now_us();
        let used_before = self.used_bytes_atomic(); // add-gc-tlab (option B)
        // P0-16: report what runs. Under the generational collector that is a minor, plus — when an
        // incremental cycle is open — the rest of that cycle, which completes a major.
        let finishing = self.major_cycle_active();
        let kind = if finishing { GcKind::Major } else { self.one_shot_kind() };
        self.fire_event(GcEvent::BeforeCollect { kind, used_bytes: used_before });
        // add-incremental-major-gc M2b: a forced collection promises everything unreachable is
        // gone when it returns — which an open incremental cycle would otherwise leave for later.
        let finish = self.finish_major_cycle("force_collect");
        let freed_bytes = finish.freed_bytes + self.run_cycle_collection();
        {
            let mut i = self.inner.lock();
            Self::count_collection(&mut i.stats, self.one_shot_kind(), finish.finished);
            i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
            self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
        }
        self.maybe_reset_near_limit_warned();
        let pause_us = Self::now_us().saturating_sub(start);
        self.pause_histogram.lock().record(pause_us);
        self.fire_event(GcEvent::AfterCollect { kind, freed_bytes, pause_us });
        CollectStats { freed_bytes, pause_us, kind: Some(kind) }
    }

    pub(super) fn collect_cycles_with_context(&self, ctx: &crate::vm_context::VmContext) {
        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => {
                if let Some(_pause) = crate::gc::safepoint::request_gc_pause(ctx) {
                    self.take_collect_request();
                    self.collect_cycles();
                }
            }
            crate::gc::GcMode::GenerationalMarkSweep => {
                // add-generational-gc P3 (2026-05-22): minor + escalation.
                // fix-minor-and-major-in-one-pause (2026-09-10): one cycle runs a minor **or**
                // a major, never both — escalation now asks for the major on the *next* cycle
                // (see the note further down).
                let _pause = match crate::gc::safepoint::request_gc_pause(ctx) {
                    Some(p) => p,
                    None => return,
                };
                self.take_collect_request();
                if self.inner.lock().pause_count > 0 { return; }

                let start = Self::now_us();
                let used_before = self.used_bytes_atomic(); // add-gc-tlab (option B)

                // Measure young population pre-minor for escalation calc.
                let young_before = {
                    let r_obj = self.region_object.lock();
                    let r_arr = self.region_array.lock();
                    let r_var = self.region_var.lock();
                    // fix-minor-gc-skips-var-region: the var region is a minor participant
                    // now, so it belongs in the denominator too. Leaving it out made the
                    // survival ratio systematically wrong — it measured two of the three
                    // regions the sweep actually touches, and escalation to major fired off
                    // that partial view.
                    r_obj.young_count() + r_arr.young_count() + r_var.young_count()
                };

                // add-bounded-nursery (2026-09-08): the auto-collect policy decides the kind
                // (the deferred safepoint path only knows "collect") and hands it over here.
                // A promoted-byte gate trip means the old generation needs sweeping, which a
                // minor cannot do at all.
                let want_major = self
                    .pending_major
                    .swap(false, std::sync::atomic::Ordering::AcqRel)
                    // **adaptive-promotion (2026-09-12)**: the policy has decided to drop a
                    // promotion tier, and the switch may only be applied on top of a major —
                    // see `ArcMagrGC::apply_promotion_age` for why. So ask for one.
                    || (crate::config::runtime_config().gc_adaptive_promotion
                        && self.promotion_policy.wants_major());

                // **fix-minor-and-major-in-one-pause (2026-09-10)**: a major marks the whole
                // heap from the roots and sweeps every region — everything a minor reclaims and
                // more. Running the minor *first* and the major on top of it, in one pause, pays
                // for the young set twice and reclaims nothing extra.
                //
                // The only thing the skipped minor would have done that the major does not is
                // **promotion** (`gen_age` bumps, young-list removal). Aging is driven by minors
                // by design, so the survivors simply age on the next one; nothing is promoted
                // merely because a major happened, which is the more defensible rule anyway.
                //
                // Measured on `src/bench/scenarios/09_alloc_ctorless` (a 100%-survival
                // allocation loop, where escalation fires on *every* cycle): each cycle was
                // `minor 156.7 ms + major 188.5 ms` — 370 ms of pause to free 0 bytes.
                // add-incremental-major-gc M2b: a major is a sequence of bounded slices now; the
                // policy's flags (and whether a cycle is open) decide what this one pause does.
                let freed_bytes: u64;
                let (mut did_major, mut did_minor) = (false, false);
                let work = self.choose_generational_work(want_major);
                // P0-16: the events name the work this pause does, not the entry point.
                let kind = match work {
                    GenWork::Minor => GcKind::Minor,
                    GenWork::Slice => GcKind::Slice,
                    GenWork::Major | GenWork::Finish => GcKind::Major,
                };
                self.fire_event(GcEvent::BeforeCollect { kind, used_bytes: used_before });
                if work == GenWork::Major {
                    freed_bytes = self.run_cycle_collection_major();
                    did_major = true;
                } else if work == GenWork::Slice || work == GenWork::Finish {
                    let o = if work == GenWork::Slice {
                        self.run_major_slice(&mut SliceBudget::from_config())
                    } else {
                        self.finish_major_cycle("explicit collection or soft cap")
                    };
                    freed_bytes = o.freed_bytes;
                    did_major = o.finished;
                } else {
                    did_minor = true;
                    let minor = self.run_cycle_collection_minor();
                    freed_bytes = minor.freed_bytes;

                    // fix-minor-survival-counts-promotion-as-death (2026-09-08): survival is
                    // `1 - reclaimed / young_before` — the fraction of the young set the sweep
                    // did **not** tombstone.
                    //
                    // It used to be `young_after / young_before`, reading the young list's size
                    // after the sweep. But a survivor that reaches `PROMOTION_THRESHOLD` leaves
                    // the young list, so that ratio counted **promotion as death**. With the
                    // threshold at 2, most survivors of any given minor are promoted out, so the
                    // ratio sat far below `Z42_GC_MINOR_THRESHOLD` no matter how little was
                    // actually collected — escalation never fired. Measured on
                    // `z42c.semantics --release --no-incremental` at 128M: **18 minors,
                    // 0 majors**, old garbage never collected, peak RSS 1034.6 MB against
                    // 902.9 MB for not collecting at all and 606.6 MB for plain STW.
                    if young_before > 0 {
                        let survival =
                            1.0 - (minor.reclaimed_entries as f32 / young_before as f32);
                        if survival >= Self::minor_escalation_threshold() {
                            // The nursery is not producing garbage, so the garbage — if any —
                            // is in the old generation. Ask for a major on the **next** trip
                            // rather than running one on top of the minor just done (see the
                            // note above); the old generation waits at most one more nursery.
                            self.pending_major
                                .store(true, std::sync::atomic::Ordering::Release);
                        }
                    }
                }

                {
                    let mut i = self.inner.lock();
                    i.stats.gc_cycles += 1;
                    // fix-minor-and-major-in-one-pause: the two are exclusive now — a cycle
                    // that wanted a major skips the minor entirely, so counting both would
                    // report a minor that never ran.
                    if did_major {
                        i.stats.major_collections += 1;
                    } else if did_minor {
                        i.stats.minor_collections += 1;
                    }
                    i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
                    self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
                }
                if did_minor {
                    self.after_minor_in_cycle();
                }
                self.maybe_reset_near_limit_warned();
                let pause_us = Self::now_us().saturating_sub(start);
                // add-pause-budget-nursery: only a **minor** teaches the cost model — a slice is
                // bounded by its own budget, and a one-shot major is not what the nursery sizes.
                if did_minor {
                    self.observe_minor_pause(super::pause_budget::MinorSample {
                        pause_us,
                        scanned: young_before as u64,
                        survivors: self.young_count() as u64,
                        used_before,
                        used_after: used_before.saturating_sub(freed_bytes),
                    });
                }
                self.pause_histogram.lock().record(pause_us);
                self.fire_event(GcEvent::AfterCollect { kind, freed_bytes, pause_us });
                #[cfg(debug_assertions)]
                self.debug_validate_invariants();
            }
        }
    }

    pub(super) fn finalize_now(&self, value: &Value) -> bool {
        // add-gc-tlab (stage 2): merge this thread's TLAB first so the target is
        // an ordinary (retired) region slot before tombstone_via_entry pushes it
        // to free_list — otherwise a just-allocated, still-borrowed slot could
        // enter free_list while its chunk is borrowed (slot double-use). Rare
        // explicit-finalize path, so the retire cost is negligible.
        self.retire_thread_tlab();
        match value {
            Value::Object(gc) => {
                let entry_ptr = gc.entry_ptr();
                // SAFETY: GcRef contract guarantees entry pointer is
                // valid for the lifetime of the GcRef. We're under
                // the trait dispatch path; caller's Value parameter
                // keeps the GcRef alive throughout.
                let entry: &crate::gc::region::RegionEntry<ScriptObject> = unsafe { entry_ptr.as_ref() };
                let fin = entry.take_finalizer();
                let fired = fin.is_some();
                if let Some(f) = fin { f(); }
                let mut region = self.region_object.lock();
                region.tombstone_via_entry(entry);
                fired
            }
            Value::Array(gc) => {
                let entry_ptr = gc.entry_ptr();
                let entry: &crate::gc::region::RegionEntry<ArrayObj> = unsafe { entry_ptr.as_ref() };
                let fin = entry.take_finalizer();
                let fired = fin.is_some();
                if let Some(f) = fin { f(); }
                let mut region = self.region_array.lock();
                region.tombstone_via_entry(entry);
                fired
            }
            _ => false,
        }
    }

    pub(super) fn register_soft_ref(&self, value: &Value) -> u64 {
        use crate::gc::soft_registry::ErasedSoftEntry;
        let (entry, key) = match value {
            Value::Object(gc) => {
                let ptr = gc.entry_ptr();
                let generation = {
                    // SAFETY: entry pointer stable; we only read the generation atomic.
                    unsafe { ptr.as_ref() }.generation.load(std::sync::atomic::Ordering::Acquire)
                };
                unsafe { ptr.as_ref() }.inc_soft_ref_count();
                let key = ptr.as_ptr() as u64;
                (ErasedSoftEntry::from_object(ptr, generation), key)
            }
            Value::Array(gc) => {
                let ptr = gc.entry_ptr();
                let generation = unsafe { ptr.as_ref() }.generation.load(std::sync::atomic::Ordering::Acquire);
                unsafe { ptr.as_ref() }.inc_soft_ref_count();
                let key = ptr.as_ptr() as u64;
                (ErasedSoftEntry::from_array(ptr, generation), key)
            }
            _ => return 0,
        };
        self.inner.lock().soft_registry.insert(entry);
        key
    }

    pub(super) fn soft_ref_get(&self, key: u64) -> Value {
        let v = self.soft_ref_get_unshaded(key);
        // add-incremental-major-gc M2a/M2b: a soft read can revive an object (shade it), and must
        // not revive one an incremental sweep has already judged dead.
        if self.admit_resurrected(&v) { v } else { Value::Null }
    }

    fn soft_ref_get_unshaded(&self, key: u64) -> Value {
        // Snapshot under lock, then work outside.
        let entries = self.inner.lock().soft_registry.snapshot_entries();
        let key_usize = key as usize;
        for e in &entries {
            if e.ptr_key() != key_usize { continue; }
            if !e.is_alive() { return Value::Null; }
            // e.is_alive() confirmed: alive=true AND generation == snapshot.
            return Self::soft_entry_value(e);
        }
        Value::Null
    }

    /// The value a soft-registry entry refers to, rebuilt with the entry's snapshot generation
    /// (safe against slot reuse). The caller has checked the entry is alive.
    pub(super) fn soft_entry_value(e: &crate::gc::soft_registry::ErasedSoftEntry) -> Value {
        let key_usize = e.ptr_key();
        match e.kind {
            crate::gc::soft_registry::ErasedKind::Object => {
                let ptr = key_usize as *mut crate::gc::region::RegionEntry<crate::metadata::ScriptObject>;
                let nn = unsafe { std::ptr::NonNull::new_unchecked(ptr) };
                Value::Object(unsafe { GcRef::from_region_entry(nn, e.generation_snapshot()) })
            }
            crate::gc::soft_registry::ErasedKind::Array => {
                let ptr = key_usize as *mut crate::gc::region::RegionEntry<ArrayObj>;
                let nn = unsafe { std::ptr::NonNull::new_unchecked(ptr) };
                Value::Array(unsafe { GcRef::from_region_entry(nn, e.generation_snapshot()) })
            }
        }
    }

    pub(super) fn unregister_soft_ref(&self, key: u64) {
        let key_usize = key as usize;
        // Find the entry kind before removing (need it to decrement the right type).
        let kind = {
            let inner = self.inner.lock();
            inner.soft_registry.snapshot_entries()
                .into_iter()
                .find(|e| e.ptr_key() == key_usize)
                .map(|e| e.kind)
        };
        self.inner.lock().soft_registry.remove_one(key_usize);
        // Decrement soft_ref_count on the backing RegionEntry.
        if let Some(kind) = kind {
            match kind {
                crate::gc::soft_registry::ErasedKind::Object => {
                    // SAFETY: pointer came from a live RegionEntry; we only
                    // touch the atomic soft_ref_count field.
                    let ptr = key_usize as *mut crate::gc::region::RegionEntry<crate::metadata::ScriptObject>;
                    unsafe { (*ptr).dec_soft_ref_count(); }
                }
                crate::gc::soft_registry::ErasedKind::Array => {
                    let ptr = key_usize as *mut crate::gc::region::RegionEntry<ArrayObj>;
                    unsafe { (*ptr).dec_soft_ref_count(); }
                }
            }
        }
    }
}
