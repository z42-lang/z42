//! Heap retention diagnostics — reverse reference graph + `whyRetained` queries
//! (add-heap-retention-diagnostics).
//!
//! Answers "what keeps this object alive": **L1** direct referrers (objects/roots
//! pointing straight at it) and **L2** retaining roots (which GC roots reach it,
//! category-level). z42's precise GC can do this where .NET's cannot.
//!
//! This module holds graph construction and the **pure graph logic**: `RetentionGraph` records reverse
//! edges (`child ← parent`) + root edges, and answers L1/L2. The heap walk that
//! feeds it uses `gc::walk` for live regions and categorized roots.
//! Full retaining *paths* (L3) are deferred.

use crate::object::{ArrRef, ObjRef, Value};
use std::collections::{HashMap, HashSet, VecDeque};

use crate::gc::walk::RootKind;

/// Whether a heap referrer is a script object or an array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RetainerKind {
    Object = 0,
    Array = 1,
}

/// A direct heap referrer of the target (an object/array that holds a reference).
#[derive(Debug, Clone)]
pub struct RetainerInfo {
    pub kind: RetainerKind,
    /// FQ type name of the referrer (e.g. `"Demo.Holder"`, `"int[]"`).
    pub type_name: String,
    /// The referrer's heap identity (data ptr as usize) — dedup + stable id.
    pub id: usize,
}

/// A GC root that retains the target (category-level).
#[derive(Debug, Clone)]
pub struct RootInfo {
    pub kind: RootKind,
}

/// Reverse reference graph over the live heap, built once per diagnostic query
/// (after a full GC → only reachable objects, no floating garbage).
#[derive(Debug, Default)]
pub struct RetentionGraph {
    /// child ptr → its direct referrer objects.
    rev: HashMap<usize, Vec<RetainerInfo>>,
    /// object ptr directly pointed at by a root → the root kinds.
    root_ptrs: HashMap<usize, Vec<RootKind>>,
}

impl RetentionGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a heap edge `parent → child` (called once per live object's
    /// heap-ref child during the walk).
    pub fn add_edge(&mut self, child: usize, parent: RetainerInfo) {
        self.rev.entry(child).or_default().push(parent);
    }

    /// Record a `root → obj` edge (obj is directly held by a root of `kind`).
    pub fn add_root_edge(&mut self, obj: usize, kind: RootKind) {
        self.root_ptrs.entry(obj).or_default().push(kind);
    }

    /// **L1**: the direct referrers of `target` (deduped by identity).
    pub fn direct_referrers(&self, target: usize) -> Vec<RetainerInfo> {
        let mut seen = HashSet::new();
        self.rev
            .get(&target)
            .into_iter()
            .flatten()
            .filter(|r| seen.insert(r.id))
            .cloned()
            .collect()
    }

    /// **L2**: the GC roots that retain `target` — reverse BFS from `target`
    /// along `rev`, collecting the kinds of any root that directly holds any
    /// reached ancestor (deduped by kind, deterministic order).
    pub fn retaining_roots(&self, target: usize) -> Vec<RootInfo> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut kinds: HashSet<RootKind> = HashSet::new();

        visited.insert(target);
        queue.push_back(target);
        while let Some(p) = queue.pop_front() {
            if let Some(rks) = self.root_ptrs.get(&p) {
                kinds.extend(rks.iter().copied());
            }
            if let Some(parents) = self.rev.get(&p) {
                for parent in parents {
                    if visited.insert(parent.id) {
                        queue.push_back(parent.id);
                    }
                }
            }
        }

        let mut out: Vec<RootInfo> = kinds.into_iter().map(|kind| RootInfo { kind }).collect();
        out.sort_by_key(|r| r.kind as u8);
        out
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;

/// **add-heap-retention-diagnostics **: the heap identity (data ptr
/// as usize) of a heap-ref `Value`, or `None` for primitives / stack refs. Used
/// to key the reverse reference graph. Mirrors `mark_if_unmarked`'s variant set.
fn value_heap_ptr(v: &Value) -> Option<usize> {
    match v {
        // Object model R6: the header address, taken from the handle (thin, whole-block provenance).
        Value::Object(gc) => Some(ObjRef::as_ptr(gc) as usize),
        // add-boxed-struct-identity (P4b): boxed struct is a heap object (an object block).
        Value::BoxedStruct(gc) => Some(ObjRef::as_ptr(gc) as usize),
        // Object model R9: an array is a block too — the header address.
        Value::Array(gc) => Some(ArrRef::as_ptr(gc) as usize),
        // unify-gc-heap PR-2: the closure is itself a heap object (region_var block) — its own
        // block address is its identity.
        Value::Closure(c) => Some(c.addr()),
        // unify-gc-heap PR-4: strings are region_var blocks now — the block header address is
        // their heap identity. (`FuncRef` is a `FnId` since P1-2 PR 8 — no heap identity.)
        Value::Str(s) => Some(s.as_ptr() as usize),
        // make-value-copy: a `Ref` handle has no stable heap identity of its own (its
        // payload lives in the transient arena); like `StructRef`/`StackObject` → None.
        _ => None,
    }
}

/// **add-heap-retention-diagnostics **: build the reverse
/// reference graph over the live heap (object + array regions) + categorized
/// roots. Callers `force_collect()` first so only reachable objects remain.
fn build_graph(heap: &crate::gc::Heap) -> RetentionGraph {
    // add-gc-tlab (stage 2): merge the caller's TLAB so retention analysis
    // sees its recent allocations (callers already force_collect first, but
    // this keeps the region view consistent regardless of entry).
    heap.retire_thread_tlab();
    let mut g = RetentionGraph::new();

    crate::gc::walk::objects(heap, &mut |value| {
        let Value::Object(handle) = value else { return };
        let self_ptr = crate::object::ObjRef::as_ptr(handle) as usize;
        let obj = handle.borrow();
        let type_name = obj.type_desc.name.clone();
        obj.visit_refs(&mut |slot| {
            if let Some(child) = value_heap_ptr(slot) {
                g.add_edge(
                    child,
                    RetainerInfo {
                        kind: RetainerKind::Object,
                        type_name: type_name.clone(),
                        id: self_ptr,
                    },
                );
            }
        });
    });
    crate::gc::walk::arrays(heap, &mut |value| {
        let Value::Array(handle) = value else { return };
        let self_ptr = crate::object::ArrRef::as_ptr(handle) as usize;
        let arr = handle.borrow();
        let type_name = format!("{}[]", &*arr.element_type());
        arr.visit_refs(&mut |elem| {
            if let Some(child) = value_heap_ptr(elem) {
                g.add_edge(
                    child,
                    RetainerInfo {
                        kind: RetainerKind::Array,
                        type_name: type_name.clone(),
                        id: self_ptr,
                    },
                );
            }
        });
    });
    crate::gc::walk::roots(heap, &mut |value, kind| {
        if let Some(object) = value_heap_ptr(value) {
            g.add_root_edge(object, kind);
        }
    });
    g
}
/// Caller holds the VM pause (or uses a standalone heap), as for a full heap walk.
pub fn direct_referrers(heap: &crate::gc::Heap, target: usize) -> Vec<RetainerInfo> {
    heap.collect_full();
    build_graph(heap).direct_referrers(target)
}
/// Caller holds the VM pause (or uses a standalone heap).
pub fn retaining_roots(heap: &crate::gc::Heap, target: usize) -> Vec<RootInfo> {
    heap.collect_full();
    build_graph(heap).retaining_roots(target)
}
