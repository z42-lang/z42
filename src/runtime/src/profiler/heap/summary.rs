//! Type-count and byte summaries over the existing live-value view.
use super::types::{HeapSnapshot, SnapshotCoverage};
use crate::gc::Heap;
use crate::object::Value;

/// 取 Value 的"类型名"（用于 snapshot 聚合）。
fn type_name_of(value: &Value) -> Option<String> {
    match value {
        Value::Object(rc) => Some(rc.type_desc().name.clone()),
        Value::BoxedStruct(rc) => Some(rc.type_desc().name.clone()), // add-boxed-struct-identity (P4b)
        Value::Array(_) => Some("<Array>".to_string()),
        _ => None,
    }
}

pub fn take_snapshot(heap: &Heap) -> HeapSnapshot {
    // Include unpinned live objects through the heap's incremental liveness filter.
    let mut snapshot = HeapSnapshot {
        coverage: SnapshotCoverage::Full,
        timestamp_us: crate::pal::time::monotonic_us(),
        ..Default::default()
    };
    crate::gc::walk::live_values(heap, &mut |v| {
        let size = heap.object_size_bytes(v) as u64;
        let Some(type_name) = type_name_of(v) else {
            return;
        };
        let entry = snapshot.objects_by_type.entry(type_name).or_default();
        entry.count += 1;
        entry.bytes += size;
        snapshot.total_objects += 1;
        snapshot.total_bytes += size;
    });
    snapshot
}

#[cfg(test)]
#[path = "summary_tests.rs"]
mod summary_tests;
