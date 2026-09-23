//! `ArcMagrGC` mark-sweep 原语：mark/sweep 阶段 + soft-ref 复活 + live 快照。
//! 编排/控制 API 见 `control.rs`（refactor-arc-heap-modularization）。

use crate::gc::refs::MarkKind;
use crate::metadata::Value;
use crate::gc::refs::{GcRef};
use crate::gc::types::{FinalizerFn};
use crate::gc::phase_timer::PhaseTimer;

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-mark-sweep-collector P3 (2026-05-21)**: mark phase of the
    /// mark-sweep collector (now the default).
    ///
    /// BFS from roots (pinned + external scanner) → sets `marked = 1` on
    /// every reachable `GcAllocation`. Idempotent within one cycle: the
    /// `GcRef::mark` CAS guarantees each object enqueues children exactly
    /// once even under root reuse. [`sweep_phase`](Self::sweep_phase)
    /// consumes the bit and resets marks on survivors.
    ///
    /// Returns the count of newly-marked allocations — used by unit tests
    /// to verify BFS visits the expected set.
    pub(super) fn mark_phase(&self) -> usize {
        // Initial roots: pinned + **strong GC handles** + external scanner output.
        let mut queue: Vec<Value> = {
            let i = self.inner.lock();
            i.roots.values().cloned().chain(i.handle_slab.strong_targets()).collect()
        };
        {
            let scanner_borrow = self.external_root_scanner.lock();
            if let Some(scan) = scanner_borrow.as_ref() {
                scan(&mut |v| {
                    queue.push(v.clone());
                });
            }
        }

        // add-incremental-major-gc M1: marks with the cycle's epoch, opened by the caller
        // (`run_cycle_collection_stw`) — which is what whitens the heap; there is no reset pass.
        let kind = self.major_mark();
        let mut newly_marked = 0usize;
        while let Some(v) = queue.pop() {
            // Mark the allocation backing this Value; if already marked (or not a heap
            // allocation at all, e.g. a primitive), skip.
            if !Self::mark_if_unmarked(&v, kind) { continue; }
            newly_marked += 1;

            v.trace_children(kind, &mut |child| {
                queue.push(child.clone());
            });
        }
        newly_marked
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: attempt to mark `v` via
    /// CAS. Returns `true` iff this call transitioned the allocation
    /// from unmarked to marked (i.e. caller is responsible for tracing
    /// children). Returns `false` for primitives + already-marked +
    /// non-heap refs (Stack ref kinds). Single source of truth for
    /// "mark this value" — used by both `mark_phase` (when refactored
    /// in P4) and the concurrent path (P3 barrier, P4 mark loop).
    pub(super) fn mark_if_unmarked(v: &Value, kind: MarkKind) -> bool {
        match v {
            Value::Object(gc) => GcRef::mark(gc, kind),
            Value::Array(gc)  => GcRef::mark(gc, kind),
            // unify-gc-heap PR-2: mark the closure block (region_var); env marked via trace.
            Value::Closure(c) => c.mark(kind),
            // unify-gc-heap PR-4: mark the string block (leaf). `FuncRef` carries a `Str`.
            Value::Str(s) => s.mark(kind),
            Value::FuncRef(s) => s.mark(kind),
            // add-boxed-struct-identity (P4b, 路 B2): mark the boxed struct's shared ScriptObject.
            Value::BoxedStruct(gc) => GcRef::mark(gc, kind),
            // make-value-copy: `Ref` / `StructRefHeap` handles mark nothing here — their
            // payload GcRefs are seeded by `TransientArena::scan_roots` (GC root).
            _ => false,
        }
    }

    /// **add-concurrent-gc P4a (2026-05-22)**: drain the gray-set
    /// (`mark_queue`) until empty. Trace each popped value's children
    /// and shade newly-discovered heap refs gray (mark + enqueue).
    ///
    /// **Termination invariant**: caller must ensure no new entries
    /// can be pushed concurrently before checking emptiness. In the
    /// concurrent path that means either:
    /// 1. Run during `ConcurrentMarking` phase — barriers + this drain
    ///    race; loop until both empty AND a final STW handshake
    ///    confirms no more writes can occur (P4b orchestrates this).
    /// 2. Run during `Marking` phase (handshake) — mutators parked,
    ///    no new barrier pushes possible, so emptiness is final.
    ///
    /// Returns the count of objects marked during this drain (useful
    /// for tests + diagnostics). 0 on already-empty queue.
    pub(super) fn drain_mark_queue(&self) -> usize {
        let kind = self.major_mark();
        let mut traced = 0usize;
        loop {
            // Take ownership of the current queue contents in one swap.
            // Mutators may push concurrently via barrier (under
            // ConcurrentMarking); we'll see those on the next iteration.
            let local: Vec<Value> = std::mem::take(&mut *self.mark_queue.lock());
            if local.is_empty() {
                break;
            }
            for v in &local {
                traced += 1;
                v.trace_children(kind, &mut |child| {
                    if Self::mark_if_unmarked(child, kind) {
                        self.mark_queue.lock().push(child.clone());
                    }
                });
            }
            // `local` drops here; any heap-ref values it held that are
            // also reachable elsewhere stay alive via those other refs.
        }
        traced
    }

    /// Sweep phase of the mark-sweep collector.
    ///
    /// **add-mark-sweep-collector P3 (2026-05-21)**: original
    /// implementation walked the Arc-backed `heap_registry` snapshot.
    ///
    /// **add-custom-allocator P1 (2026-05-22)**: rewritten to walk
    /// regions directly:
    /// 1. For each alive entry in `region_object` + `region_array`:
    ///    - `marked == 1` → reset to 0 (next cycle ready), retain
    ///    - `marked == 0` → fire registered finalizer (one-shot take),
    ///      tombstone the entry (alive=false, generation++, push slot
    ///      to free list); break inner refs so any cyclic references
    ///      no longer count toward "iterate_live_objects" reachability
    ///
    /// Returns estimated `freed_bytes` (sum of `object_size_bytes` for
    /// tombstoned entries).
    ///
    /// Finalizer-timing contract (D3): firings happen here only. The
    /// `Std.GC.Finalize(x)` builtin (added by P2) provides a separate
    /// path for prompt resource release outside sweep.
    pub(super) fn sweep_phase(&self) -> u64 {
        #[cfg(debug_assertions)]
        self.debug_stw_no_push.store(true, std::sync::atomic::Ordering::SeqCst);
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: sweep_phase entered with non-empty mark_queue ({q} items) — push happened between P5 drain and sweep start");
        }
        let mut freed_bytes: u64 = 0;
        let major = self.major_mark();

        // Object region — **one-pass-major-sweep (2026-09-13)**: scan and tombstone in a
        // single walk (see `Region::sweep_all_in_one_pass`). This used to stage the dead in a
        // `Vec<(handle, finalizer, size)>` and then, per entry, re-take the region lock to
        // break its edges and take it a **third** time to tombstone.
        let sweep_objects = PhaseTimer::start("sweep/objects");
        let (obj_freed, obj_reclaimed) = {
            let mut region = self.region_object.lock();
            region.sweep_all_in_one_pass(major, Self::prepare_dead_object)
        };
        sweep_objects.count(obj_reclaimed);
        freed_bytes += obj_freed;
        drop(sweep_objects);
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: mark_queue non-empty after object region sweep ({q} items)");
        }

        // Array region — same one pass.
        let sweep_arrays = PhaseTimer::start("sweep/arrays");
        let (arr_freed, arr_reclaimed) = {
            let mut region = self.region_array.lock();
            region.sweep_all_in_one_pass(major, Self::prepare_dead_array)
        };
        sweep_arrays.count(arr_reclaimed);
        freed_bytes += arr_freed;
        drop(sweep_arrays);

        // Variable-length region (unify-gc-heap PR-2: closures). `VarRegion::sweep` mark-checks
        // + tombstones every unmarked live block internally, running the injected drop-glue
        // (drops each reclaimed closure's `fn_name: String`). MUST run after the mark phase —
        // it does (sweep_phase is invoked post-mark). v1 is STW-only: the generational minor
        // sweep does not touch region_var, so closures are reclaimed at full GC (never freed
        // prematurely — safe).
        {
            // fix-var-sweep-accounting: `sweep` returns the bytes that were actually
            // charged to `used_bytes` when these blocks were allocated (variable-length
            // Str payloads at their true length; array element blocks at zero, since the
            // owning array header both charged and credits them). The old estimate —
            // reclaimed_count × sizeof(ClosureData) — was a constant applied to
            // variable-length blocks and double-counted array storage, so `freed` could
            // exceed `used_before` and the auto-collect budget read low.
            let t = PhaseTimer::start("sweep/var");
            let (reclaimed, credited) = self.region_var.lock().sweep(major);
            t.count(reclaimed);
            freed_bytes += credited;
        }

        // **add-gc-tlab (stage 2, D7)**: chunk-level reclaim — move every
        // fully-dead chunk into its region's `free_chunk_pool` so `borrow_chunk`
        // recycles it (short-lived-object workloads like the compiler otherwise
        // grow chunks unboundedly, since the TLAB path bypasses slot-level
        // free_list reuse). Runs under STW at the sweep tail, after tombstoning.
        {
            let _t = PhaseTimer::start("sweep/chunk reclaim");
            self.region_object.lock().reclaim_dead_chunks();
            self.region_array.lock().reclaim_dead_chunks();
            // stage 3: variable-length region chunk reclaim (fully-dead bump chunks → pool).
            self.region_var.lock().reclaim_dead_var_chunks();
        }

        #[cfg(debug_assertions)]
        self.debug_stw_no_push.store(false, std::sync::atomic::Ordering::SeqCst);
        freed_bytes
    }

    /// A major sweep's business with a dying object entry: its size estimate, breaking its
    /// reference edges, taking its finalizer. Shared by the one-shot and the incremental sweep.
    pub(super) fn prepare_dead_object(
        entry: &crate::gc::region::RegionEntry<crate::metadata::ScriptObject>,
    ) -> (Option<FinalizerFn>, u64) {
        let mut obj = entry.value.lock();
        let size = Self::script_object_size_estimate(&obj);
        // unify-object-byte-layout: break every strong reference edge — the side-table `refs`
        // AND (PR-3 chunk 2b) the object/array pointers byte-inlined in `bytes` — so no
        // tombstoned entry is left holding a handle into the region.
        //
        // Ahead of the finalizer, as on the minor side (#591): [`FinalizerFn`] takes no
        // arguments, so it has no way to read the object whose edges these are.
        for r in obj.refs_mut_raw().iter_mut() {
            *r = Value::Null;
        }
        obj.clear_inline_refs();
        drop(obj);
        (entry.take_finalizer(), size)
    }

    /// [`Self::prepare_dead_object`] for an array header.
    pub(super) fn prepare_dead_array(
        entry: &crate::gc::region::RegionEntry<crate::metadata::types::ArrayObj>,
    ) -> (Option<FinalizerFn>, u64) {
        let size = Self::array_size_estimate(&entry.value.lock());
        // unify-gc-heap PR-3: no eager element drop here — the array's element storage lives
        // in a `region_var` block (uniquely owned by this header), reclaimed by the var sweep
        // (drop-glue drops the boxed Values) in the same cycle. Tombstoning the header just
        // releases the slot.
        (entry.take_finalizer(), size)
    }

    /// **add-gc-softref (2026-05-26)**: after mark_phase, re-mark alive
    /// soft-ref targets when heap pressure < `Z42_GC_SOFT_THRESHOLD`.
    /// Snapshots the registry entries under the lock, then calls
    /// `revive_if_unmarked` outside the lock (only touches RegionEntry
    /// atomics — no heap lock required).
    pub(super) fn revive_soft_refs(&self) {
        let used_bytes = self.used_bytes_atomic(); // add-gc-tlab (option B)
        let (entries, max_bytes) = {
            let inner = self.inner.lock();
            let entries = inner.soft_registry.snapshot_entries();
            let max  = inner.stats.max_bytes.unwrap_or(0);
            (entries, max)
        };
        // revive_pass on snapshot — no lock held; only atomic field access.
        //
        // Reviving marks each surviving soft target, but we MUST also trace its
        // children: a field / array-backing block reachable only through the
        // target is still unmarked after `mark_phase`, so the following
        // `sweep_phase` would reclaim it out from under the live target →
        // dangling `GcRef` → UAF (release) / `generation/alive mismatch` panic
        // (debug). Collect the revived targets and drain them through the mark
        // queue, exactly as the incremental major path does in
        // `revive_soft_refs_into`. (Regression source: #701 added the
        // `on_revived` callback but wired it only into the incremental path,
        // leaving this STW / one-shot-major path with an empty `|_| {}` — array
        // backings live in `region_var` and are marked solely via the target's
        // `trace_children`, so even a primitive array's block leaked here.)
        let kind = self.major_mark();
        let mut revived: Vec<Value> = Vec::new();
        let _ = crate::gc::soft_registry::SoftRegistry::revive_snapshot(
            &entries,
            used_bytes,
            max_bytes,
            kind,
            |e| revived.push(Self::soft_entry_value(e)),
        );
        if !revived.is_empty() {
            self.mark_queue.lock().extend(revived);
            self.drain_mark_queue();
        }
    }

    /// **add-incremental-major-gc M2a**: whether `v` carries `kind`'s mark. Values that are not
    /// GC allocations count as marked — there is nothing for the SATB barrier to record.
    pub(crate) fn is_marked_value(v: &Value, kind: MarkKind) -> bool {
        match v {
            Value::Object(gc) | Value::BoxedStruct(gc) => GcRef::is_marked(gc, kind),
            Value::Array(gc) => GcRef::is_marked(gc, kind),
            Value::Closure(c) => c.is_marked(kind),
            Value::Str(s) | Value::FuncRef(s) => s.is_marked(kind),
            _ => true,
        }
    }

    /// **add-incremental-major-gc M2a**: open a major cycle — a new epoch, and the SATB barrier
    /// starts recording for it. Callers: the STW cycle and the concurrent cycle's Phase 1.
    pub(super) fn open_major_cycle(&self) -> MarkKind {
        let kind = self.begin_major_mark();
        if let MarkKind::Major(epoch) = kind {
            crate::gc::satb::begin_marking(self.epoch, epoch);
        }
        kind
    }

    /// **add-incremental-major-gc M2a**: finish the cycle's marking. Grey everything the SATB
    /// barrier (and the weak / soft read barrier) recorded, trace it, and repeat until a round turns
    /// up nothing new — only then stop recording. Every mutator has handed its buffer over by the
    /// time this runs (they retire their TLAB when they park); this thread hands over its own here.
    pub(super) fn close_major_marking(&self) {
        let kind = self.major_mark();
        loop {
            self.retire_thread_tlab();
            let recorded = std::mem::take(&mut *self.satb_queue.lock());
            if recorded.is_empty() {
                break;
            }
            {
                let mut queue = self.mark_queue.lock();
                for v in recorded {
                    if Self::mark_if_unmarked(&v, kind) {
                        queue.push(v);
                    }
                }
            }
            self.drain_mark_queue();
        }
        crate::gc::satb::end_marking(self.epoch);
    }

    /// **add-incremental-major-gc M2a**: the weak / soft read barrier. While this heap is marking,
    /// a value handed out by a weak or soft reference is recorded exactly like an overwritten one —
    /// it may have been only weakly reachable at the snapshot, and it is now in a register.
    pub(super) fn shade_if_marking(&self, v: &Value) {
        if let Some(kind) = crate::gc::satb::heap_is_marking(self.epoch) {
            if !Self::is_marked_value(v, kind) {
                self.satb_queue.lock().push(v.clone());
            }
        }
    }

    /// **add-incremental-major-gc M1 (2026-09-15)**: open a major mark — advance this heap's
    /// epoch and return the kind every mark of the cycle uses. This replaced the
    /// `reset_all_marks_in_regions` pass (a walk over every alive entry and block, 6~11 ms
    /// per major on `z42c.semantics`): a slot is major-marked only if it holds the *current*
    /// epoch, so advancing it whitens the whole heap at once. Why that is safe is on
    /// [`MarkKind`].
    pub(super) fn begin_major_mark(&self) -> MarkKind {
        let epoch = crate::gc::refs::next_epoch(self.mark_epoch.load(std::sync::atomic::Ordering::Relaxed));
        self.mark_epoch.store(epoch, std::sync::atomic::Ordering::Relaxed);
        MarkKind::Major(epoch)
    }

    /// The major kind of the cycle in progress (or of the last one).
    ///
    /// Marks can be placed with it **between** cycles — the concurrent-mode barrier shades
    /// outside a cycle, and a test may mark by hand. That is harmless precisely because the next
    /// `begin_major_mark` moves past it. The one way it could bite is if a stamp matched the
    /// *next* epoch: this is why the epoch starts at 1 rather than 0 (an initial 0 read as 1
    /// would equal the first cycle's epoch, and every object the barrier shaded before the first
    /// collection would count as already marked — children untraced, a live array's backing
    /// swept; `stress_seeded_concurrent_short` caught exactly that).
    #[inline]
    pub(super) fn major_mark(&self) -> MarkKind {
        MarkKind::Major(self.mark_epoch.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Snapshot all alive Values across the heap's regions. Order:
    /// object region first, then array region. Each entry visited
    /// exactly once (no de-dup required — regions are the authoritative
    /// store, every entry there represents one allocation).
    ///
    /// **add-custom-allocator P1 (2026-05-22)**: replaces the
    /// heap_registry-walking version. No more `Weak::upgrade` per
    /// entry; just a linear chunks walk with an alive-bit check.
    pub(super) fn snapshot_live_from_registry(&self) -> Vec<Value> {
        // add-gc-tlab (stage 2): retire the calling thread's TLAB so its own
        // freshly-allocated (still-borrowed) objects are merged and visible in
        // the snapshot — otherwise `iterate_alive` skips the borrowed chunk.
        // This is the single choke point for `take_snapshot` / `stats` /
        // `iterate_live_objects`. (Other threads' in-flight allocations remain
        // out of a non-STW diagnostic snapshot, which is acceptable.)
        self.retire_thread_tlab();
        let mut alive: Vec<Value> = Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_alive(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                // SAFETY: handle came from iterate_alive over a live entry;
                // generation matches the entry's current state.
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                alive.push(Value::Object(gc));
            });
        }
        {
            let region = self.region_array.lock();
            region.iterate_alive(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                alive.push(Value::Array(gc));
            });
        }
        // add-incremental-major-gc M2b: this hands out every alive entry, including ones an
        // incremental cycle has already judged dead (still alive until its sweep reaches them).
        alive.retain(|v| self.admit_resurrected(v));
        alive
    }
}
