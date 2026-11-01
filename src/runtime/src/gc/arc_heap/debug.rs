//! `ArcMagrGC` 调试/测试辅助：`#[cfg(test)]` accessors + `debug_validate_invariants`。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use parking_lot::Mutex;
use crate::object::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// Test-only entry point to `snapshot_roots_into_mark_queue` (whose
    /// production caller is the incremental cycle's opening slice); tests
    /// drive the snapshot directly without setting up a real collect.
    #[cfg(test)]
    pub(crate) fn snapshot_roots_into_mark_queue_for_test(&self) -> usize {
        self.snapshot_roots_into_mark_queue()
    }

    /// Test-only entry to read the mark queue contents.
    #[cfg(test)]
    pub(crate) fn mark_queue_for_test(&self) -> Vec<Value> {
        self.mark_queue.lock().clone()
    }

    /// **add-gc-debug-invariants P1 **: test-only mutable
    /// access to the mark queue for injecting corruption (e.g. leaving
    /// stale entries to verify validate panics).
    #[cfg(test)]
    pub(crate) fn mark_queue_for_test_mut(&self) -> parking_lot::MutexGuard<'_, Vec<Value>> {
        self.mark_queue.lock()
    }

    /// Test-only entry to the `mark_if_unmarked` static helper.
    #[cfg(test)]
    pub(crate) fn mark_if_unmarked_for_test(&self, v: &Value) -> bool {
        Self::mark_if_unmarked(v, self.major_mark())
    }

    /// add-incremental-major-gc M1: the major kind in force, for tests that inspect marks.
    #[cfg(test)]
    pub(crate) fn major_mark_for_test(&self) -> crate::object::header::marks::MarkKind {
        self.major_mark()
    }

    /// **add-gc-tlab stage 3**: test-only handle to the variable-length region
    /// (var TLAB chunk-count / pool assertions).
    #[cfg(test)]
    pub(crate) fn region_var_for_test(&self) -> &Mutex<crate::gc::var_region::VarRegion> {
        &self.region_var
    }

    /// **add-generational-gc P3 **: test-only entry to the
    /// minor escalation threshold (used by tests; production reads via
    /// minor_escalation_threshold() in the dispatch path).
    #[cfg(test)]
    pub(crate) fn minor_escalation_threshold_for_test() -> f32 {
        Self::minor_escalation_threshold()
    }

    /// **add-gc-debug-invariants P1 **: post-collect invariant check. Panics on the
    /// first violation with a descriptive message. Release builds compile the call out (cfg gate
    /// at the call site).
    ///
    /// Invariants checked:
    /// - `mark_queue` is empty post-collect (a major's mark drains it to empty before sweep)
    /// - no alive object / array carries a stale mark (sweep clears minor marks on survivors; an
    ///   orphaned mark = bug)
    #[cfg(debug_assertions)]
    pub(crate) fn debug_validate_invariants(&self) {
        // add-incremental-major-gc M2b: between the slices of an open cycle the grey queue holds
        // work in progress, and alive blocks legitimately carry the previous epoch (not yet
        // re-marked) or are doomed (not yet swept).
        if self.major_cycle_active() {
            return;
        }

        // 2. mark_queue must be empty post-collect.
        //
        // diag-mark-queue-stale : on failure, dump each
        // stale entry's kind (+ a detail for heap refs) — without the dump the assertion is
        // opaque.
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

        // 3. No alive entry should carry a mark post-sweep. add-incremental-major-gc M1: that now
        //    reads "minor bit clear, and the major epoch is either the current one or 0" — a
        //    survivor of cycle E keeps E (that is what whitens it for E+1); anything else is a
        //    stale epoch, which would make the next major skip tracing its children.
        let epoch = match self.major_mark() { crate::object::header::marks::MarkKind::Major(e) => e, _ => unreachable!() };
        let stale = |minor: bool, e: u8| minor || (e != 0 && e != epoch);
        //    iterate_alive walks heap-registry-equivalent (regions).
        //
        // diag-stale-mark-bit : on failure, dump the entry's
        // type name + slot summary so we can tell whether the marked
        // object is the freshly-allocated `Leaf` from the stress
        // worker loop (race: barrier ran post-sweep) vs an `Owner` /
        // older object (race: something re-marked it during STW).
        // Object model R6 / R9: objects and arrays are var blocks.
        let mut stale_blk: Option<(usize, String, usize)> = None;
        self.region_var.lock().iterate_alive(|h, header| {
            if stale_blk.is_some() || !header.block_type().is_object_or_array() { return; }
            if stale(header.is_marked(crate::object::header::marks::MarkKind::Minor), header.major_epoch()) {
                let what = if header.block_type() == crate::object::header::BlockType::Object {
                    crate::object::ObjRef::from_var(h).borrow().type_desc.name.clone()
                } else {
                    format!("{}[]", crate::object::ArrRef::from_var(h).borrow().element_type())
                };
                stale_blk = Some((h.addr(), what, header.size()));
            }
        });
        if let Some((addr, ty, n)) = stale_blk {
            panic!("stale mark bit after sweep: block={addr:#x}, type={ty}, payload={n} B");
        }
    }

    /// **add-mark-sweep-collector P3 **: test-only entry
    /// point that exposes the full mark+sweep cycle for unit tests in
    /// `arc_heap_tests::mark_phase`. Production code goes through
    /// `collect_cycles` → `run_cycle_collection`, which calls the same
    /// two phases.
    #[cfg(test)]
    pub(super) fn collect_cycles_mark_sweep_for_test(&self) -> u64 {
        // add-gc-tlab (stage 2): merge this thread's borrowed chunk before mark.
        self.retire_thread_tlab();
        self.open_major_cycle();
        let _newly_marked = self.mark_phase();
        self.close_major_marking();
        self.sweep_phase()
    }

    /// **add-mark-sweep-collector P3 **: clear all mark bits.
    /// Test-only — production sweep resets marks on survivors inline.
    /// Walks the registry via the existing snapshot upgrade path (so dead
    /// WeakRefs are skipped naturally). Makes mark_phase tests idempotent
    /// across runs.
    #[cfg(test)]
    pub(super) fn reset_marks_for_test(&self) {
        // add-incremental-major-gc M1: whitening the heap is opening a new epoch.
        self.begin_major_mark();
    }
}
