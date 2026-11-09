//! 根快照、marked-context 扫描与锁内基础遍历。

use super::*;
use crate::object::Value;

impl crate::gc::Heap {
    /// Root snapshot for an incremental major. Walks pinned roots +
    /// external root scanner output, marks each as gray (via
    /// `mark_if_unmarked`), and pushes newly-marked roots into
    /// `mark_queue`; the cycle's mark slices then drain the queue.
    ///
    /// Must be called inside a pause (`GcPhase::Marking`) so the snapshot is
    /// consistent — no mutator can add/remove roots between `pinned_roots`
    /// traversal and external scanner traversal.
    ///
    /// Returns the count of newly-marked root objects (for tests +
    /// diagnostics).
    pub(super) fn snapshot_roots_into_mark_queue(&self) -> usize {
        let mut queue = self.mark_queue.lock();
        queue.clear();
        // add-incremental-major-gc M1: the cycle's epoch, opened by the caller.
        let kind = self.major_mark();
        let mut count = 0usize;
        // Pinned roots — cloned under inner.lock() to release the lock
        // before any potential observer callbacks.
        let roots: Vec<Value> = {
            let i = self.inner.lock();
            // fix-strong-handles-are-not-roots: strong handle targets are roots too.
            i.roots.values().cloned().chain(i.handle_slab.strong_targets()).collect()
        };
        for v in roots {
            if Self::mark_if_unmarked(&v, kind) {
                queue.push(v);
                count += 1;
            }
        }
        // External root scanner (e.g. VmContext static_fields).
        let scanner_borrow = self.external_root_scanner.lock();
        if let Some(scan) = scanner_borrow.as_ref() {
            // SAFETY: mutators are paused, or this is a standalone heap.
                unsafe { scan.visit_roots(&mut |v| {
                if Self::mark_if_unmarked(v, kind) {
                    queue.push(v.clone());
                    count += 1;
                }
            }) };
        }
        count
    }

    /// Observe the same marked objects before sweep; association and identities belong to VM.
    pub(super) fn observe_marked_objects(&self, snapshot: &mut dyn crate::gc::to_vm::ContextReclaimSnapshot) {
        if snapshot.is_empty() { return; }
        let major = self.major_mark();
        let region = self.region_var.lock();
        region.iterate_alive_of(crate::object::header::BlockType::Object, |h, header| {
            if header.is_marked(major) {
                let obj = crate::object::ObjRef::from_var(h);
                snapshot.observe_live_object(&obj.borrow());
            }
        });
    }

}

impl crate::gc::Heap {
    pub(crate) fn walk_diagnostic_objects(&self, visit: &mut dyn FnMut(&Value)) {
        self.region_var.lock().iterate_alive_of(crate::object::header::BlockType::Object, |h, _| {
            let value = Value::Object(crate::object::ObjRef::from_var(h));
            visit(&value);
        });
    }
    pub(crate) fn walk_diagnostic_arrays(&self, visit: &mut dyn FnMut(&Value)) {
        self.region_var.lock().iterate_alive(|h, header| {
            let bt = header.block_type();
            if bt != crate::object::header::BlockType::Object && bt.is_object_or_array() {
                let value = Value::Array(crate::object::ArrRef::from_var(h));
                visit(&value);
            }
        });
    }
    pub(crate) fn walk_diagnostic_roots(&self, visit: &mut dyn FnMut(&Value, crate::gc::walk::RootKind)) {
        {
            let inner = self.inner.lock();
            for value in inner.roots.values().chain(&inner.handle_slab.strong_targets().collect::<Vec<_>>()) {
                visit(value, crate::gc::walk::RootKind::Pinned);
            }
        }
        if let Some(scan) = self.categorized_root_scanner.lock().as_ref() { scan(visit); }
    }
}
