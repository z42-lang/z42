//! `MagrGC` trait 默认方法契约测试。
//!
//! 验证 trait 的默认实现（Phase 1 全部 no-op）行为符合预期 ——
//! 不 panic、不改变可观察状态。具体后端的行为测试在 `arc_heap_tests.rs`。

use crate::gc::{HeapStats, MagrGC, ArcMagrGC};
use crate::metadata::Value;

#[test]
fn default_write_barrier_field_is_noop() {
    let heap = ArcMagrGC::new();
    let owner = heap.alloc_array(vec![Value::I64(1)]);
    let new   = Value::I64(42);
    let stats_before = heap.stats();
    heap.write_barrier_field(&owner, 0, &new);
    assert_eq!(heap.stats(), stats_before);
}

#[test]
fn default_write_barrier_array_elem_is_noop() {
    let heap = ArcMagrGC::new();
    let arr  = heap.alloc_array(vec![Value::I64(0); 3]);
    let new  = Value::I64(99);
    let stats_before = heap.stats();
    heap.write_barrier_array_elem(&arr, 1, &new);
    assert_eq!(heap.stats(), stats_before);
}

#[test]
fn default_collect_is_noop() {
    let heap = ArcMagrGC::new();
    let _    = heap.alloc_array(vec![]);
    let stats_before = heap.stats();
    heap.collect();
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
