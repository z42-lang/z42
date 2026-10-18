//! `ArcMagrGC` mark-sweep 原语：mark/sweep 阶段 + soft-ref 复活 + live 快照。
//! 编排/控制 API 见 `control.rs`（refactor-arc-heap-modularization）。

use crate::gc::refs::MarkKind;
use crate::metadata::Value;
use crate::gc::ArrRef;
use crate::gc::obj_ref::ObjRef;
use crate::gc::phase_timer::PhaseTimer;

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-mark-sweep-collector P3 (2026-05-21)**: mark phase of the
    /// mark-sweep collector (now the default).
    ///
    /// BFS from roots (pinned + external scanner) → sets `marked = 1` on
    /// every reachable `GcAllocation`. Idempotent within one cycle: the
    /// `ArrRef::mark` CAS guarantees each object enqueues children exactly
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

    /// Attempt to mark `v` via CAS. Returns `true` iff this call
    /// transitioned the allocation from unmarked to marked (i.e. caller is
    /// responsible for tracing children). Returns `false` for primitives +
    /// already-marked + non-heap refs (Stack ref kinds). Single source of
    /// truth for "mark this value" — used by `mark_phase`, the minor mark,
    /// the root snapshot and the incremental mark slices.
    pub(super) fn mark_if_unmarked(v: &Value, kind: MarkKind) -> bool {
        match v {
            Value::Object(gc) => ObjRef::mark(gc, kind),
            Value::Array(gc)  => ArrRef::mark(gc, kind),
            // unify-gc-heap PR-2: mark the closure block (region_var); env marked via trace.
            Value::Closure(c) => c.mark(kind),
            // unify-gc-heap PR-4: mark the string block (leaf).
            Value::Str(s) => s.mark(kind),
            // add-boxed-struct-identity (P4b, 路 B2): mark the boxed struct's shared ScriptObject.
            Value::BoxedStruct(gc) => ObjRef::mark(gc, kind),
            // make-value-copy: `Ref` / `StructRefHeap` handles mark nothing here — their
            // payload GcRefs are seeded by `TransientArena::scan_roots` (GC root).
            _ => false,
        }
    }

    /// Drain the gray-set (`mark_queue`) until empty. Trace each popped
    /// value's children and shade newly-discovered heap refs gray (mark +
    /// enqueue). Runs inside a pause (mutators parked), so emptiness is
    /// final.
    ///
    /// Returns the count of objects marked during this drain (useful
    /// for tests + diagnostics). 0 on already-empty queue.
    pub(super) fn drain_mark_queue(&self) -> usize {
        let kind = self.major_mark();
        let mut traced = 0usize;
        // Children wait in the prefetching ring (`mark_ring`) before they are marked, so `RING`
        // header misses are in flight at a time; what is left in it is marked at the end of the
        // round.
        let mut ring = super::mark_ring::MarkRing::new();
        loop {
            // Take ownership of the current queue contents in one swap;
            // tracing below pushes newly-marked children for the next round.
            let local: Vec<Value> = std::mem::take(&mut *self.mark_queue.lock());
            if local.is_empty() {
                break;
            }
            let mut grey = Vec::new();
            for v in &local {
                traced += 1;
                v.trace_children(kind, &mut |child| {
                    if !child.is_heap_ref() {
                        return;
                    }
                    if let Some(c) = ring.push(*child) {
                        if Self::mark_if_unmarked(&c, kind) {
                            grey.push(c);
                        }
                    }
                });
            }
            while let Some(c) = ring.pop_oldest() {
                if Self::mark_if_unmarked(&c, kind) {
                    grey.push(c);
                }
            }
            self.mark_queue.lock().append(&mut grey);
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
    /// Walks the region directly: every alive block without this cycle's epoch is tombstoned (its
    /// finalizer, if any, fires through the flagged-dead list before any chunk is reclaimed);
    /// marked blocks keep their epoch (the next cycle's advance whitens them).
    ///
    /// Returns the freed bytes (the allocation charge of the tombstoned blocks).
    ///
    /// Finalizer-timing contract (D3): firings happen here only. The
    /// `Std.GC.Finalize(x)` builtin (added by P2) provides a separate
    /// path for prompt resource release outside sweep.
    pub(super) fn sweep_phase(&self) -> u64 {
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: sweep_phase entered with non-empty mark_queue ({q} items) — marking was not drained");
        }
        let mut freed_bytes: u64 = 0;
        let major = self.major_mark();

        // The variable-length region — objects, arrays, strings, closures. `VarRegion::sweep`
        // mark-checks + tombstones every unmarked live block, running the injected drop glue (an
        // object block drops its header, a struct array its layout; the rest are POD). Flagged
        // dead blocks (finalizers, soft refs, remembered) are handed to `process_flagged_dead` by
        // the chunk reclaim below.
        {
            // fix-var-sweep-accounting: `sweep` returns the bytes that were actually charged to
            // `used_bytes` when these blocks were allocated (`VarRegion::alloc_charge_bytes`).
            let t = PhaseTimer::start("sweep/var");
            let (reclaimed, credited) = self.region_var.lock().sweep(major);
            t.count(reclaimed);
            freed_bytes += credited;
        }

        // **add-gc-tlab (stage 2, D7)**: chunk-level reclaim — move every
        // fully-dead chunk into the region's free chunk pool so `borrow_chunk`
        // recycles it (short-lived-object workloads like the compiler otherwise
        // grow chunks unboundedly, since the TLAB path bypasses slot-level
        // free_list reuse). Runs under STW at the sweep tail, after tombstoning.
        {
            let _t = PhaseTimer::start("sweep/chunk reclaim");
            self.reclaim_dead_chunks_and_measure(true);
        }

        freed_bytes
    }

    /// **add-gc-softref (2026-05-26)**: after mark_phase, re-mark alive
    /// soft-ref targets when heap pressure < `Z42_GC_SOFT_THRESHOLD`.
    /// Snapshots the registry entries under the lock, then calls
    /// `revive_if_unmarked` outside the lock (only touches RegionEntry
    /// atomics — no heap lock required).
    pub(super) fn revive_soft_refs(&self) {
        let used_bytes = self.occupied_bytes(); // pressure against the cap: true footprint
        let (entries, max_bytes) = {
            let inner = self.inner.lock();
            let entries = inner.soft_registry.snapshot_entries();
            let max  = inner.stats.max_bytes.unwrap_or(0);
            (entries, max)
        };
        // revive_pass on snapshot — no lock held; only atomic field access.
        //
        // Reviving marks each surviving soft target, but we MUST also trace its
        // children: an object / array reachable only through the
        // target is still unmarked after `mark_phase`, so the following
        // `sweep_phase` would reclaim it out from under the live target →
        // dangling handle → UAF (release) / generation-mismatch panic (debug). Collect the
        // revived targets and drain them through the mark queue, exactly as the incremental
        // major path does in `revive_soft_refs_into` (#701).
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
            Value::Object(gc) | Value::BoxedStruct(gc) => ObjRef::is_marked(gc, kind),
            Value::Array(gc) => ArrRef::is_marked(gc, kind),
            Value::Closure(c) => c.is_marked(kind),
            Value::Str(s) => s.is_marked(kind),
            _ => true,
        }
    }

    /// **add-incremental-major-gc M2a**: open a major cycle — a new epoch, and the SATB barrier
    /// starts recording for it. Callers: the STW cycle and the incremental cycle's first slice.
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
    /// Marks can be placed with it **between** cycles — a test may mark by hand. That is harmless
    /// precisely because the next `begin_major_mark` moves past it. The one way it could bite is
    /// if a stamp matched the *next* epoch: this is why the epoch starts at 1 rather than 0 (an
    /// initial 0 read as 1 would equal the first cycle's epoch, and every object marked before
    /// the first collection would count as already marked — children untraced, a live array's
    /// backing swept).
    #[inline]
    pub(super) fn major_mark(&self) -> MarkKind {
        MarkKind::Major(self.mark_epoch.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Snapshot every alive object and array (one walk of the region, each block once — the
    /// region is the authoritative store).
    pub(super) fn snapshot_live_from_registry(&self) -> Vec<Value> {
        // add-gc-tlab (stage 2): retire the calling thread's TLAB so its own
        // freshly-allocated (still-borrowed) objects are merged and visible in
        // the snapshot — otherwise `iterate_alive` skips the borrowed chunk.
        // This is the single choke point for `take_snapshot` / `stats` /
        // `iterate_live_objects`. (Other threads' in-flight allocations remain
        // out of a non-STW diagnostic snapshot, which is acceptable.)
        self.retire_thread_tlab();
        let mut alive: Vec<Value> = Vec::new();
        self.region_var.lock().iterate_alive(|h, header| {
            match header.block_type() {
                crate::gc::var_region::BlockType::Object => alive.push(Value::Object(ObjRef::from_var(h))),
                bt if bt.is_object_or_array() => alive.push(Value::Array(ArrRef::from_var(h))),
                _ => {}
            }
        });
        // add-incremental-major-gc M2b: this hands out every alive entry, including ones an
        // incremental cycle has already judged dead (still alive until its sweep reaches them).
        alive.retain(|v| self.admit_resurrected(v));
        alive
    }
}
