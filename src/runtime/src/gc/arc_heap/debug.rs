//! `ArcMagrGC` 调试/测试辅助：`#[cfg(test)]` accessors + `debug_validate_invariants`。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use parking_lot::Mutex;
use crate::metadata::{ScriptObject, Value};
use crate::metadata::types::{ArrayObj};
use crate::gc::refs::{GcRef};

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-concurrent-gc P2 (2026-05-22)**: test-only entry point to
    /// `snapshot_roots_into_mark_queue`. The production caller will be
    /// `run_cycle_collection_concurrent` (P4) under STW; tests need to
    /// drive the snapshot directly without setting up a real collect.
    #[cfg(test)]
    pub(crate) fn snapshot_roots_into_mark_queue_for_test(&self) -> usize {
        self.snapshot_roots_into_mark_queue()
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: test-only entry to read
    /// the mark queue contents.
    #[cfg(test)]
    pub(crate) fn mark_queue_for_test(&self) -> Vec<Value> {
        self.mark_queue.lock().clone()
    }

    /// **add-gc-debug-invariants P1 (2026-05-22)**: test-only mutable
    /// access to the mark queue for injecting corruption (e.g. leaving
    /// stale entries to verify validate panics).
    #[cfg(test)]
    pub(crate) fn mark_queue_for_test_mut(&self) -> parking_lot::MutexGuard<'_, Vec<Value>> {
        self.mark_queue.lock()
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: test-only entry to the
    /// `mark_if_unmarked` static helper.
    #[cfg(test)]
    pub(crate) fn mark_if_unmarked_for_test(v: &Value) -> bool {
        Self::mark_if_unmarked(v)
    }

    /// **add-generational-gc P1 (2026-05-22)**: test-only accessors
    /// for the region locks (needed by `arc_heap_tests::generational`
    /// to peek at card_dirty state without going through MagrGC trait).
    #[cfg(test)]
    pub(crate) fn region_object_for_test(&self) -> &Mutex<crate::gc::region::Region<ScriptObject>> {
        &self.region_object
    }

    #[cfg(test)]
    pub(crate) fn region_array_for_test(&self) -> &Mutex<crate::gc::region::Region<ArrayObj>> {
        &self.region_array
    }

    /// **add-generational-gc P3 (2026-05-22)**: test-only entry to the
    /// minor escalation threshold (used by tests; production reads via
    /// minor_escalation_threshold() in the dispatch path).
    #[cfg(test)]
    pub(crate) fn minor_escalation_threshold_for_test() -> f32 {
        Self::minor_escalation_threshold()
    }

    /// **add-gc-debug-invariants P1 (2026-05-22)**: post-collect
    /// invariant check. Validates both regions + heap-wide invariants.
    /// Panics on first violation with a descriptive message. Release
    /// builds compile this method body out entirely via the cfg gate
    /// at the call site.
    ///
    /// Invariants checked:
    /// - `region_object` + `region_array`: see
    ///   [`crate::gc::region::Region::validate`]
    /// - `mark_queue` is empty post-collect (concurrent mark must
    ///   drain to empty before sweep; STW + generational never use
    ///   the queue)
    /// - No alive entry has `marked == 1` (sweep clears marks on
    ///   survivors; orphaned mark bit = bug)
    #[cfg(debug_assertions)]
    pub(crate) fn debug_validate_invariants(&self) {
        // 1. Per-region invariants.
        if let Err(v) = self.region_object.lock().validate() {
            panic!("region_object invariant violation: {}", v);
        }
        if let Err(v) = self.region_array.lock().validate() {
            panic!("region_array invariant violation: {}", v);
        }

        // 2. mark_queue must be empty post-collect.
        //
        // diag-mark-queue-stale (2026-05-30): on failure, dump each
        // stale entry's kind + (for heap refs) the GcRef pointer + the
        // marked bit. This is the only way to tell whether the entry
        // was pushed by a still-running mutator (write_barrier_field
        // pushes pre-marked refs) vs. a buggy collector-internal push
        // (would push unmarked, which would also indicate a different
        // class of bug). Without the dump the assertion is opaque —
        // we couldn't diagnose the windows-only flake (concurrent_gc_
        // mode_stress_no_race_no_leak) before this commit.
        let stale: Vec<Value> = self.mark_queue.lock().clone();
        if !stale.is_empty() {
            let summary: Vec<String> = stale.iter().take(8).map(|v| {
                let kind = match v {
                    Value::Object(_) => "Object",
                    Value::Array(_)  => "Array",
                    Value::Str(_)    => "Str",
                    Value::I64(_)    => "I64",
                    Value::F64(_)    => "F64",
                    Value::Bool(_)   => "Bool",
                    Value::Char(_)   => "Char",
                    Value::Null      => "Null",
                    _                => "Other",
                };
                let extra = match v {
                    Value::Object(gc) => format!(" obj_borrow_type={}", gc.type_desc().name),
                    Value::Array(gc)  => format!(" array_len={}", gc.borrow().len()),
                    _                  => String::new(),
                };
                format!("    [kind={kind}{extra}]")
            }).collect();
            let extra = if stale.len() > 8 {
                format!("\n    ... ({} more)", stale.len() - 8)
            } else {
                String::new()
            };
            panic!(
                "mark_queue stale post-collect: {} entries remaining\n{}{}",
                stale.len(), summary.join("\n"), extra
            );
        }

        // 3. No alive entry should carry marked=1 post-sweep.
        //    iterate_alive walks heap-registry-equivalent (regions).
        //
        // diag-stale-mark-bit (2026-05-30): on failure, dump the entry's
        // type name + slot summary so we can tell whether the marked
        // object is the freshly-allocated `Leaf` from the stress
        // worker loop (race: barrier ran post-sweep) vs an `Owner` /
        // older object (race: something re-marked it during STW).
        let region_object = self.region_object.lock();
        let mut stale_obj: Option<(u32, u32, String, usize)> = None;
        region_object.iterate_alive(|h, e| {
            if stale_obj.is_some() { return; }
            if e.is_marked() {
                let obj = e.value.lock();
                let ty = obj.type_desc.name.clone();
                let nslots = obj.refs.len(); // unify-object-byte-layout (PR-2): ref-slot count (diagnostic)
                stale_obj = Some((h.chunk_idx as u32, h.entry_idx as u32, ty, nslots));
            }
        });
        drop(region_object);
        if let Some((c, i, ty, n)) = stale_obj {
            panic!(
                "stale mark bit in region_object after sweep: chunk={c}, entry={i}, type={ty}, slots={n}"
            );
        }
        let region_array = self.region_array.lock();
        let mut stale_arr: Option<(u32, u32, usize)> = None;
        region_array.iterate_alive(|h, e| {
            if stale_arr.is_some() { return; }
            if e.is_marked() {
                let arr = e.value.lock();
                stale_arr = Some((h.chunk_idx as u32, h.entry_idx as u32, arr.len()));
            }
        });
        drop(region_array);
        if let Some((c, i, len)) = stale_arr {
            panic!(
                "stale mark bit in region_array after sweep: chunk={c}, entry={i}, array_len={len}"
            );
        }
    }

    /// **add-concurrent-gc P4a (2026-05-22)**: end-to-end concurrent
    /// collect minus the safepoint phase transitions (P4b wires those
    /// in). Runs the steps that DON'T require a real VmContext: root
    /// snapshot → drain → sweep. Test-callable on a standalone
    /// `ArcMagrGC::new()` so we can verify algorithmic correctness
    /// (reachable chains preserved, unreachable cycles freed, barrier
    /// integration) before integrating with safepoint protocol.
    ///
    /// **NOT a production path**: production goes through P4b which
    /// adds STW pause coordination + handshake. Calling this without
    /// the surrounding pause is racy under real concurrent mutators —
    /// safe only for single-threaded test contexts that simulate
    /// mutator writes inline.
    #[cfg(test)]
    pub(crate) fn run_cycle_collection_concurrent_inline_for_test(&self) -> u64 {
        // Step 1: STW-equivalent root snapshot (no mutators in test).
        self.snapshot_roots_into_mark_queue();

        // Step 2: Drain queue (simulates "ConcurrentMarking" but
        // single-threaded — no real concurrency).
        let _traced = self.drain_mark_queue();

        // Step 3: Final residual drain (post-handshake equivalent —
        // catches anything pushed by barrier between roots snapshot
        // and now; in single-thread test no concurrent writes happen,
        // so this should be a no-op, but the loop is still here for
        // structural parity with P4b production flow).
        let _residual = self.drain_mark_queue();

        // Step 4: Sweep (STW; identical to STW path's sweep).
        self.sweep_phase()
    }

    /// **add-mark-sweep-collector P3 (2026-05-21)**: test-only entry
    /// point that exposes the full mark+sweep cycle for unit tests in
    /// `arc_heap_tests::mark_phase`. Production code goes through
    /// `collect_cycles` → `run_cycle_collection`, which calls the same
    /// two phases.
    #[cfg(test)]
    pub(super) fn collect_cycles_mark_sweep_for_test(&self) -> u64 {
        let _newly_marked = self.mark_phase();
        self.sweep_phase()
    }

    /// **add-mark-sweep-collector P3 (2026-05-21)**: clear all mark bits.
    /// Test-only — production sweep resets marks on survivors inline.
    /// Walks the registry via the existing snapshot upgrade path (so dead
    /// WeakRefs are skipped naturally). Makes mark_phase tests idempotent
    /// across runs.
    #[cfg(test)]
    pub(super) fn reset_marks_for_test(&self) {
        for v in self.snapshot_live_from_registry() {
            match &v {
                Value::Object(gc) => GcRef::clear_mark(gc),
                Value::Array(gc)  => GcRef::clear_mark(gc),
                _ => {}
            }
        }
    }
}
