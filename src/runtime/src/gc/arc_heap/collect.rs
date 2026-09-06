//! `ArcMagrGC` mark-sweep 原语：mark/sweep 阶段 + soft-ref 复活 + live 快照。
//! 编排/控制 API 见 `control.rs`（refactor-arc-heap-modularization）。

use crate::metadata::Value;
use crate::gc::refs::{GcRef};
use crate::gc::types::{FinalizerFn};

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
        // Initial roots: pinned + external scanner output.
        let mut queue: Vec<Value> = self.inner.lock().roots.values().cloned().collect();
        {
            let scanner_borrow = self.external_root_scanner.lock();
            if let Some(scan) = scanner_borrow.as_ref() {
                scan(&mut |v| {
                    queue.push(v.clone());
                });
            }
        }

        let mut newly_marked = 0usize;
        while let Some(v) = queue.pop() {
            // Mark the allocation backing this Value; if already marked
            // (or not a heap allocation at all, e.g. a primitive), skip.
            let just_marked = match &v {
                Value::Object(gc) => GcRef::mark(gc),
                Value::Array(gc)  => GcRef::mark(gc),
                // unify-gc-heap PR-2: mark the closure's `ClosureData` block in region_var;
                // `trace_children` then pushes its `env` array so the env stays marked.
                Value::Closure(c) => c.mark(),
                // unify-gc-heap PR-4: strings are GC blocks now — mark the `BlockType::Str`
                // block (leaf, no children to trace). `FuncRef` carries a `Str` name too.
                Value::Str(s) => s.mark(),
                Value::FuncRef(s) => s.mark(),
                // add-boxed-struct-identity (P4b, 路 B2): a boxed struct is a shared
                // `ScriptObject` in region_object — mark it like Object (trace_children
                // then scans its `struct_refs` reference leaves).
                Value::BoxedStruct(gc) => GcRef::mark(gc),
                // make-value-copy: `Ref` / `StructRefHeap` are transient-arena handles —
                // their payload's GcRefs are seeded as GC roots by `TransientArena::
                // scan_roots`, so the handle itself marks nothing here (falls to `false`).
                _ => false,
            };
            if !just_marked { continue; }
            newly_marked += 1;

            v.trace_children(&mut |child| {
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
    pub(super) fn mark_if_unmarked(v: &Value) -> bool {
        match v {
            Value::Object(gc) => GcRef::mark(gc),
            Value::Array(gc)  => GcRef::mark(gc),
            // unify-gc-heap PR-2: mark the closure block (region_var); env marked via trace.
            Value::Closure(c) => c.mark(),
            // unify-gc-heap PR-4: mark the string block (leaf). `FuncRef` carries a `Str`.
            Value::Str(s) => s.mark(),
            Value::FuncRef(s) => s.mark(),
            // add-boxed-struct-identity (P4b, 路 B2): mark the boxed struct's shared ScriptObject.
            Value::BoxedStruct(gc) => GcRef::mark(gc),
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
                v.trace_children(&mut |child| {
                    if Self::mark_if_unmarked(child) {
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

        // Object region.
        let mut tombstones_object: Vec<(crate::gc::region::RegionHandle, Option<FinalizerFn>, u64)> =
            Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_alive(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                } else {
                    // Estimate size before tombstoning (entry still readable).
                    let size = {
                        let obj = entry.value.lock();
                        Self::script_object_size_estimate(&obj)
                    };
                    let fin = entry.take_finalizer();
                    tombstones_object.push((h, fin, size));
                }
            });
        }
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: mark_queue non-empty after object region scan ({q} items)");
        }
        // Fire finalizers + clear inner refs + tombstone.
        for (h, fin, size) in tombstones_object {
            if let Some(f) = fin { f(); }
            #[cfg(debug_assertions)]
            {
                let q = self.mark_queue.lock().len();
                assert_eq!(q, 0, "BUG: mark_queue non-empty after finalizer (h={:?}, {q} items)", h);
            }
            freed_bytes += size;
            // Break inner refs to release any cycles for the region's
            // bookkeeping (iterate_live_objects, future child traversal
            // won't see refs into already-tombstoned entries).
            //
            // SAFETY: handle came from iterate_alive; entry is still
            // accessible (alive=true at this point — we haven't
            // tombstoned yet).
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
            #[cfg(debug_assertions)]
            {
                let q = self.mark_queue.lock().len();
                assert_eq!(q, 0, "BUG: mark_queue non-empty after slot clearing (h={:?}, {q} items)", h);
            }
            self.region_object.lock().tombstone(h);
            #[cfg(debug_assertions)]
            {
                let q = self.mark_queue.lock().len();
                assert_eq!(q, 0, "BUG: mark_queue non-empty after tombstone (h={:?}, {q} items)", h);
            }
        }
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: mark_queue non-empty after object tombstone loop ({q} items)");
        }

        // Array region.
        let mut tombstones_array: Vec<(crate::gc::region::RegionHandle, Option<FinalizerFn>, u64)> =
            Vec::new();
        {
            let region = self.region_array.lock();
            region.iterate_alive(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
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
        for (h, fin, size) in tombstones_array {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            // unify-gc-heap PR-3: no eager element drop here — the array's element
            // storage lives in a `region_var` block (uniquely owned by this header),
            // reclaimed by `region_var.sweep()` (drop-glue drops the boxed Values) in
            // the same cycle. Tombstoning the header just releases the region_array slot.
            self.region_array.lock().tombstone(h);
        }

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
            let (_reclaimed, credited) = self.region_var.lock().sweep();
            freed_bytes += credited;
        }

        // **add-gc-tlab (stage 2, D7)**: chunk-level reclaim — move every
        // fully-dead chunk into its region's `free_chunk_pool` so `borrow_chunk`
        // recycles it (short-lived-object workloads like the compiler otherwise
        // grow chunks unboundedly, since the TLAB path bypasses slot-level
        // free_list reuse). Runs under STW at the sweep tail, after tombstoning.
        self.region_object.lock().reclaim_dead_chunks();
        self.region_array.lock().reclaim_dead_chunks();
        // stage 3: variable-length region chunk reclaim (fully-dead bump chunks → pool).
        self.region_var.lock().reclaim_dead_var_chunks();

        #[cfg(debug_assertions)]
        self.debug_stw_no_push.store(false, std::sync::atomic::Ordering::SeqCst);
        freed_bytes
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
        let _ = crate::gc::soft_registry::SoftRegistry::revive_snapshot(&entries, used_bytes, max_bytes);
    }

    /// **add-gc-stress-test (2026-05-22)**: clear `marked` on every
    /// alive entry across both regions. Used by
    /// `run_cycle_collection_stw` to guarantee mark-bit clean slate
    /// when starting a STW cycle. Idempotent.
    pub(super) fn reset_all_marks_in_regions(&self) {
        self.region_object.lock().iterate_alive(|_h, e| e.clear_mark());
        self.region_array.lock().iterate_alive(|_h, e| e.clear_mark());
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
        alive
    }
}
