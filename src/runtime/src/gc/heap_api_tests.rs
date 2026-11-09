//! Public heap API contracts: barriers preserve state and stats start empty.

use crate::gc::{HeapStats, Heap};
use crate::object::Value;

// flip-gc-default-to-generational : these two used to pass a **primitive** as the
// written value. That is a contract violation — `write_barrier_*` documents "caller must filter
// primitives via `Value::is_heap_ref`" and debug-asserts it — which went unnoticed only because
// the then-default STW arm ignores its arguments entirely. Under the generational default the
// assert fires. The subject here is "a barrier does not perturb heap stats", and the primitive
// was incidental, so these now pass a heap reference and hold in *every* mode.

#[test]
fn default_write_barrier_field_is_noop() {
    let heap = Heap::default();
    let owner = heap.alloc_array(vec![Value::I64(1)]);
    let new   = heap.alloc_array(vec![]);
    let stats_before = heap.stats();
    heap.write_barrier_field(&owner, 0, &new);
    assert_eq!(heap.stats(), stats_before);
}

#[test]
fn default_write_barrier_array_elem_is_noop() {
    let heap = Heap::default();
    let arr  = heap.alloc_array(vec![Value::I64(0); 3]);
    let new  = heap.alloc_array(vec![]);
    let stats_before = heap.stats();
    heap.write_barrier_array_elem(&arr, 1, &new);
    assert_eq!(heap.stats(), stats_before);
}

#[test]
fn paused_collect_cycles_preserves_allocation_and_counters() {
    let heap = Heap::default();
    let _    = heap.alloc_array(vec![]);
    let stats_before = heap.stats();
    heap.pause();
    heap.collect_cycles();
    heap.resume();
    assert_eq!(heap.stats(), stats_before);
}

#[test]
fn heap_stats_default_is_zero() {
    let s = HeapStats::default();
    assert_eq!(s.allocations,        0);
    assert_eq!(s.gc_cycles,          0);
    assert_eq!(s.minor_collections,  0);
    assert_eq!(s.major_collections,  0);
    assert_eq!(s.reclaimed_bytes,    0);
    assert_eq!(s.used_bytes,         0);
    assert_eq!(s.max_bytes,          None);
    assert_eq!(s.roots_pinned,       0);
    assert_eq!(s.finalizers_pending, 0);
    assert_eq!(s.observers,          0);
}
