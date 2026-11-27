//! Heap traversal for same-build diagnostics.
use crate::gc::Heap;
use crate::object::Value;
use std::{marker::PhantomData, rc::Rc};

/// GC root category (category-level; specific field/local names are deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RootKind {
    StaticField = 0,
    StackFrame = 1,
    Pinned = 2,
}

/// Preserve the heap's incremental liveness filter and TLAB retirement.
pub fn live_values(heap: &Heap, visitor: &mut dyn FnMut(&Value)) {
    heap.iterate_live_objects(visitor);
}
/// Read-only heap inspection scoped to the collecting thread's VM pause.
///
/// Visitors run under heap or VM locks. Keep access to managed values within
/// the callback, and avoid re-entering the heap or running managed code.
pub struct PausedHeap<'pause> {
    heap: &'pause Heap,
    _collector_thread: PhantomData<Rc<()>>,
}

impl<'pause> PausedHeap<'pause> {
    /// # Safety
    /// Mutators must remain paused for this heap until the inspection ends.
    pub(crate) unsafe fn new(heap: &'pause Heap) -> Self {
        Self { heap, _collector_thread: PhantomData }
    }

    /// Visit the values accepted by the heap's incremental liveness filter.
    pub fn live_values(&self, visitor: &mut dyn FnMut(&Value)) {
        self.heap.iterate_live_objects(visitor);
    }

    /// Measure an object or array visited in this heap.
    pub fn object_size_bytes(&self, value: &Value) -> usize {
        self.heap.object_size_bytes(value)
    }

    /// Visit host-pinned roots for the V8 snapshot's synthetic root.
    pub fn pinned_roots(&self, visitor: &mut dyn FnMut(&Value)) {
        self.heap.for_each_root(visitor);
    }

    /// Visit raw objects in the paused heap.
    pub fn objects(&self, visitor: &mut dyn FnMut(&Value)) {
        // SAFETY: the VM pause covers the entire borrow of the inspection.
        unsafe { self.heap.walk_diagnostic_objects(visitor); }
    }

    /// Visit raw arrays in region order.
    pub fn arrays(&self, visitor: &mut dyn FnMut(&Value)) {
        // SAFETY: the VM pause covers the entire borrow of the inspection.
        unsafe { self.heap.walk_diagnostic_arrays(visitor); }
    }

    /// Visit pinned and strong-handle roots followed by categorized VM roots.
    pub fn roots(&self, visitor: &mut dyn FnMut(&Value, RootKind)) {
        // SAFETY: the VM pause covers the entire borrow of the inspection.
        unsafe { self.heap.walk_diagnostic_roots(visitor); }
    }
}
