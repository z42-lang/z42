//! Crate-internal heap walks. Callers stop mutators (or use a standalone heap).
use crate::gc::Heap;
use crate::object::Value;

/// GC root category (category-level; specific field/local names are deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RootKind {
    StaticField = 0,
    StackFrame = 1,
    Pinned = 2,
}

pub type CategorizedRootScanner = Box<dyn Fn(&mut dyn FnMut(&Value, RootKind)) + Send + Sync>;

/// Preserve the heap's incremental liveness filter and TLAB retirement.
pub fn live_values(heap: &Heap, visitor: &mut dyn FnMut(&Value)) {
    heap.iterate_live_objects(visitor);
}
/// Lock-scoped raw objects, after collection; callbacks must not re-enter the heap.
pub fn objects(heap: &Heap, visitor: &mut dyn FnMut(&Value)) {
    heap.walk_diagnostic_objects(visitor);
}
/// Lock-scoped raw arrays, preserving region order.
pub fn arrays(heap: &Heap, visitor: &mut dyn FnMut(&Value)) {
    heap.walk_diagnostic_arrays(visitor);
}
/// Pinned and strong-handle roots, followed by categorized VM roots.
pub fn roots(heap: &Heap, visitor: &mut dyn FnMut(&Value, RootKind)) {
    heap.walk_diagnostic_roots(visitor);
}
