//! `ArcMagrGC` 环回收编排与控制 API：run_cycle_collection(_stw) + collect/force + finalize + soft-ref。
//! mark/sweep 原语见 `collect.rs`（refactor-arc-heap-modularization）。

use crate::gc::heap::MagrGC;
use crate::metadata::{ScriptObject, Value};
use crate::metadata::types::{ArrayObj};
use crate::gc::refs::{GcRef};
use crate::gc::types::{CollectStats, GcEvent, GcKind};

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
    /// **add-concurrent-gc P0 (2026-05-22)**: dispatches on `self.mode()`.
    /// Both arms currently route to the STW path; P4 fills in the
    /// concurrent arm with `run_cycle_collection_concurrent`.
    pub(super) fn run_cycle_collection(&self) -> u64 {
        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => self.run_cycle_collection_stw(),
            crate::gc::GcMode::ConcurrentMarkSweep => {
                // P0 stub: concurrent arm currently routes to STW path —
                // proves dispatch wiring without changing behavior. P4
                // replaces this with `run_cycle_collection_concurrent`.
                self.run_cycle_collection_stw()
            }
            crate::gc::GcMode::GenerationalMarkSweep => {
                // add-generational-gc P2: minor GC by default. Major
                // GC requires the VmContext-aware entry
                // (`collect_cycles_with_context`) for pause coord;
                // direct callers of `collect_cycles` (which go through
                // `force_collect`) get a minor cycle here. Major is
                // P3's expansion (auto-collect young pressure trigger
                // + escalation heuristic).
                self.run_cycle_collection_minor()
            }
        }
    }

    /// STW mark-sweep collect — the proven path. Called directly when
    /// `mode() == StwMarkSweep`, or as a fallback by the concurrent path.
    ///
    /// **add-gc-stress-test (2026-05-22)**: defensive cleanup at the
    /// boundaries. The concurrent barrier override (under
    /// `GcMode::ConcurrentMarkSweep`) leaves `marked = 1` on shaded
    /// objects + entries on `mark_queue`. The no-context `force_collect`
    /// path falls back to STW, which assumes a clean slate at start
    /// (all marks 0, queue empty). Without clearing, mark_phase
    /// observes pre-marked entries — CAS fails → `just_marked == false`
    /// → children NOT traced. Sweep then retains those entries
    /// (marked=1) even though they may be unreachable, AND their
    /// children (pointed to via slots) may be unmarked and swept,
    /// leaving stale Values inside slots → next collect's mark BFS
    /// hits entry_ref panic (use-after-finalize). Caught by stress
    /// test + C1 validator.
    pub(super) fn run_cycle_collection_stw(&self) -> u64 {
        // add-gc-tlab (stage 2, D5): retire the collecting thread's own TLAB
        // before marking, so its just-allocated (still-borrowed) chunk is merged
        // into the region and participates in mark/sweep + mark-clearing. Other
        // mutators retired at their safepoint park; in the no-context / force /
        // cargo-direct paths (no safepoint) this is the sole retire that keeps a
        // borrowed chunk from being skipped by sweep. Idempotent when unbound.
        self.retire_thread_tlab();
        // Defensive reset: ensure clean state for STW mark.
        self.reset_all_marks_in_regions();
        self.mark_queue.lock().clear();
        let _newly_marked = self.mark_phase();
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
        let freed = self.sweep_phase();
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

    pub(super) fn collect_cycles(&self) {
        if self.inner.lock().pause_count > 0 { return; }
        let start = Self::now_us();
        let used_before = self.used_bytes_atomic(); // add-gc-tlab (option B)
        self.fire_event(GcEvent::BeforeCollect {
            kind: GcKind::CycleCollector, used_bytes: used_before,
        });
        let freed_bytes = self.run_cycle_collection();
        {
            let mut i = self.inner.lock();
            i.stats.gc_cycles += 1;
            i.stats.major_collections += 1;
            i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
            self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
        }
        // Phase 3d: 若 used 已降到 90% 阈值以下，重置 near_limit_warned
        self.maybe_reset_near_limit_warned();
        let pause_us = Self::now_us().saturating_sub(start);
        self.pause_histogram.lock().record(pause_us);
        self.fire_event(GcEvent::AfterCollect {
            kind: GcKind::CycleCollector, freed_bytes, pause_us,
        });
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
        self.fire_event(GcEvent::BeforeCollect {
            kind: GcKind::Full, used_bytes: used_before,
        });
        let freed_bytes = self.run_cycle_collection();
        {
            let mut i = self.inner.lock();
            i.stats.gc_cycles += 1;
            i.stats.major_collections += 1;
            i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
            self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
        }
        self.maybe_reset_near_limit_warned();
        let pause_us = Self::now_us().saturating_sub(start);
        self.pause_histogram.lock().record(pause_us);
        self.fire_event(GcEvent::AfterCollect {
            kind: GcKind::Full, freed_bytes, pause_us,
        });
        CollectStats {
            freed_bytes, pause_us, kind: Some(GcKind::Full),
        }
    }

    pub(super) fn collect_cycles_with_context(&self, ctx: &crate::vm_context::VmContext) {
        match self.mode() {
            crate::gc::GcMode::StwMarkSweep => {
                if let Some(_pause) = crate::gc::safepoint::request_gc_pause(ctx) {
                    self.collect_cycles();
                }
            }
            crate::gc::GcMode::ConcurrentMarkSweep => {
                let pause = match crate::gc::safepoint::request_gc_pause(ctx) {
                    Some(p) => p,
                    None => return, // another collector active; park-as-mutator done
                };
                if self.inner.lock().pause_count > 0 { return; }
                let start = Self::now_us();
                let used_before = self.used_bytes_atomic(); // add-gc-tlab (option B)
                self.fire_event(GcEvent::BeforeCollect {
                    kind: GcKind::CycleCollector, used_bytes: used_before,
                });

                // Phase 1: STW root snapshot (still holding initial pause).
                self.snapshot_roots_into_mark_queue();

                // Phase 2: Yield to ConcurrentMarking — mutators resume.
                pause.yield_to_concurrent_marking();

                // Phase 3: Background mark (this thread = collector; barrier
                // writes from mutators land in mark_queue concurrently).
                self.drain_mark_queue();

                // Phase 4: STW handshake — re-park mutators for final drain.
                pause.request_handshake_pause();

                // Phase 5: Residual drain — any barrier writes between
                // drain-empty-check and handshake-acquire are now safely
                // captured in mark_queue.
                self.drain_mark_queue();
                #[cfg(debug_assertions)]
                {
                    let after_p5 = self.mark_queue.lock().len();
                    assert_eq!(after_p5, 0, "BUG: mark_queue not empty after Phase 5 drain ({after_p5} items)");
                }

                // Phase 6: STW sweep (mutators still parked).
                let freed_bytes = self.sweep_phase();
                #[cfg(debug_assertions)]
                {
                    let post_sweep = self.mark_queue.lock().len();
                    assert_eq!(post_sweep, 0,
                        "BUG: mark_queue non-empty after sweep ({post_sweep} items) — something during sweep pushed to queue");
                }
                {
                    let mut i = self.inner.lock();
                    i.stats.gc_cycles += 1;
                    i.stats.major_collections += 1;
                    i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
                    self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
                }
                self.maybe_reset_near_limit_warned();
                let pause_us = Self::now_us().saturating_sub(start);
                self.pause_histogram.lock().record(pause_us);
                self.fire_event(GcEvent::AfterCollect {
                    kind: GcKind::CycleCollector, freed_bytes, pause_us,
                });
                #[cfg(debug_assertions)]
                {
                    let post_events = self.mark_queue.lock().len();
                    assert_eq!(post_events, 0,
                        "BUG: mark_queue non-empty after fire_event ({post_events} items) — observer pushed to queue");
                }

                // Validate heap invariants while world is still stopped
                // (before pause Drop wakes workers and write-barriers resume).
                #[cfg(debug_assertions)]
                self.debug_validate_invariants();

                // pause Drop releases the world.
                drop(pause);
            }
            crate::gc::GcMode::GenerationalMarkSweep => {
                // add-generational-gc P3 (2026-05-22): minor + escalation.
                // Run a minor first; if survival rate >= threshold,
                // escalate to major in the same STW pause window.
                let _pause = match crate::gc::safepoint::request_gc_pause(ctx) {
                    Some(p) => p,
                    None => return,
                };
                if self.inner.lock().pause_count > 0 { return; }

                let start = Self::now_us();
                let used_before = self.used_bytes_atomic(); // add-gc-tlab (option B)
                self.fire_event(GcEvent::BeforeCollect {
                    kind: GcKind::CycleCollector, used_bytes: used_before,
                });

                // Measure young population pre-minor for escalation calc.
                let young_before = {
                    let r_obj = self.region_object.lock();
                    let r_arr = self.region_array.lock();
                    r_obj.young_count() + r_arr.young_count()
                };

                let mut freed_bytes = self.run_cycle_collection_minor();
                let mut did_major = false;

                // Survival rate: how much of young survived (not
                // tombstoned) AND was promoted out. Easier measured:
                // 1 - tombstoned_fraction; even easier: post young_count
                // / young_before. High survival → escalate.
                let young_after = {
                    let r_obj = self.region_object.lock();
                    let r_arr = self.region_array.lock();
                    r_obj.young_count() + r_arr.young_count()
                };

                if young_before > 0 {
                    // survival_rate = young_after / young_before (the
                    // entries still classed as young after minor —
                    // promoted entries also "survive" but leave
                    // young_list, so this is roughly the
                    // "not-tombstoned-and-not-promoted" rate).
                    let survival = young_after as f32 / young_before as f32;
                    if survival >= Self::minor_escalation_threshold() {
                        // Major in same pause window.
                        freed_bytes += self.run_cycle_collection_major();
                        did_major = true;
                    }
                }

                {
                    let mut i = self.inner.lock();
                    i.stats.gc_cycles += 1;
                    i.stats.minor_collections += 1;
                    if did_major { i.stats.major_collections += 1; }
                    i.stats.reclaimed_bytes = i.stats.reclaimed_bytes.saturating_add(freed_bytes);
                    self.sub_used_bytes(freed_bytes); // add-gc-tlab (option B): atomic used_bytes
                }
                self.maybe_reset_near_limit_warned();
                let pause_us = Self::now_us().saturating_sub(start);
                self.pause_histogram.lock().record(pause_us);
                self.fire_event(GcEvent::AfterCollect {
                    kind: GcKind::CycleCollector, freed_bytes, pause_us,
                });
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
        // Snapshot under lock, then work outside.
        let entries = self.inner.lock().soft_registry.snapshot_entries();
        let key_usize = key as usize;
        for e in &entries {
            if e.ptr_key() != key_usize { continue; }
            if !e.is_alive() { return Value::Null; }
            // e.is_alive() confirmed: alive=true AND generation == snapshot.
            // Reconstruct GcRef using the snapshot generation (safe against slot reuse).
            return match e.kind {
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
            };
        }
        Value::Null
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
