//! Heap retention diagnostics — reverse reference graph + `whyRetained` queries
//! (add-heap-retention-diagnostics).
//!
//! Answers "what keeps this object alive": **L1** direct referrers (objects/roots
//! pointing straight at it) and **L2** retaining roots (which GC roots reach it,
//! category-level). z42's precise GC can do this where .NET's cannot.
//!
//! This module holds the **pure graph logic**: `RetentionGraph` records reverse
//! edges (`child ← parent`) + root edges, and answers L1/L2. The heap walk that
//! feeds it (iterate live regions + categorized roots) lives in `arc_heap.rs`.
//! Full retaining *paths* (L3) are deferred.

use std::collections::{HashMap, HashSet, VecDeque};

/// GC root category (category-level; specific field/local names are deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RootKind {
    StaticField = 0,
    StackFrame = 1,
    FuncRefSlot = 2,
    Pinned = 3,
}

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
