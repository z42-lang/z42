//! `ArcMagrGC` roots/retention 扫描：marked-context 扫描 + 反向引用图构建。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use super::*;
use crate::metadata::Value;

impl crate::gc::arc_heap::ArcMagrGC {
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
            scan(&mut |v| {
                if Self::mark_if_unmarked(v, kind) {
                    queue.push(v.clone());
                    count += 1;
                }
            });
        }
        count
    }

    /// **add-lazy-context-unload (2026-08-05)**: after mark, walk marked
    /// `ScriptObject`s and resolve which collectible contexts they retain
    /// (instance `type_desc` ptr + reflection native handles). Only called when
    /// an unload is in flight. Marks are still set (sweep clears them next).
    pub(super) fn scan_marked_contexts(
        &self,
        snap: &crate::metadata::context::ContextLiveness,
    ) -> std::collections::HashSet<crate::metadata::context::ContextId> {
        let mut live = std::collections::HashSet::new();
        if snap.is_empty() {
            return live;
        }
        let major = self.major_mark();
        let region = self.region_object.lock();
        region.iterate_alive(|_h, entry| {
            if entry.is_marked(major) {
                let obj = entry.value.lock();
                let td_ptr = std::sync::Arc::as_ptr(&obj.type_desc) as usize;
                if let Some(cid) = snap.retained_context(td_ptr, &obj.native()) {
                    live.insert(cid);
                }
            }
        });
        live
    }

    /// **add-heap-retention-diagnostics (2026-08-06)**: build the reverse
    /// reference graph over the live heap (object + array regions) + categorized
    /// roots. Callers `force_collect()` first so only reachable objects remain.
    pub(super) fn build_retention_graph(&self) -> crate::gc::retention::RetentionGraph {
        use crate::gc::retention::{RetainerInfo, RetainerKind};
        // add-gc-tlab (stage 2): merge the caller's TLAB so retention analysis
        // sees its recent allocations (callers already force_collect first, but
        // this keeps the region view consistent regardless of entry).
        self.retire_thread_tlab();
        let mut g = crate::gc::retention::RetentionGraph::new();

        // Object region → each object's heap-ref slots become reverse edges.
        {
            let region = self.region_object.lock();
            region.iterate_alive(|_h, entry| {
                let self_ptr = entry.value.data_ptr() as usize;
                let obj = entry.value.lock();
                let type_name = obj.type_desc.name.clone();
                obj.visit_refs(&mut |slot: &Value| {
                    if let Some(child) = value_heap_ptr(slot) {
                        g.add_edge(
                            child,
                            RetainerInfo { kind: RetainerKind::Object, type_name: type_name.clone(), id: self_ptr },
                        );
                    }
                });
            });
        }
        // Array region → each array's heap-ref elements become reverse edges.
        {
            let region = self.region_array.lock();
            region.iterate_alive(|_h, entry| {
                let self_ptr = entry.value.data_ptr() as usize;
                let arr = entry.value.lock();
                let type_name = format!("{}[]", &*arr.element_type);
                // `gc_refs`: the array's reference slots (elements / struct[] reference leaves).
                for elem in arr.gc_refs() {
                    if let Some(child) = value_heap_ptr(elem) {
                        g.add_edge(
                            child,
                            RetainerInfo { kind: RetainerKind::Array, type_name: type_name.clone(), id: self_ptr },
                        );
                    }
                }
            });
        }
        // Pinned roots (GC-internal host pins / frame pins) + strong GC handles.
        {
            let inner = self.inner.lock();
            // fix-strong-handles-are-not-roots (2026-09-11): a strong handle anchors its target,
            // so retention analysis has to report it as a root — otherwise `Heap.WhyIsThisAlive`
            // shows an object that is alive with no retainer, which is exactly the question the
            // diagnostic exists to answer. Reported as `Pinned`: it is the same kind of thing —
            // an out-of-heap reference the host holds deliberately.
            for v in inner.roots.values().chain(&inner.handle_slab.strong_targets().collect::<Vec<_>>()) {
                if let Some(obj) = value_heap_ptr(v) {
                    g.add_root_edge(obj, crate::gc::retention::RootKind::Pinned);
                }
            }
        }
        // Categorized VmCore roots (static fields / stack frames / func-ref slots).
        if let Some(scan) = self.categorized_root_scanner.lock().as_ref() {
            scan(&mut |v, kind| {
                if let Some(obj) = value_heap_ptr(v) {
                    g.add_root_edge(obj, kind);
                }
            });
        }
        g
    }
}
