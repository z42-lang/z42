//! Heap inspection result data.
use std::collections::HashMap;

/// Coverage discriminator on [`HeapSnapshot`].
///
/// Phase 1 RC mode cannot enumerate all live objects (no global Rc registry);
/// the snapshot covers only objects reachable from currently-pinned roots.
/// Phase 3 tracing GC will produce `Full` snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotCoverage {
    Full,
    ReachableFromPinnedRoots,
}

impl Default for SnapshotCoverage {
    fn default() -> Self {
        Self::ReachableFromPinnedRoots
    }
}

/// Per-type aggregate in a [`HeapSnapshot`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectStats {
    pub count: u64,
    pub bytes: u64,
}

/// Heap composition snapshot.
#[derive(Debug, Clone, Default)]
pub struct HeapSnapshot {
    /// Per type-name aggregate.
    pub objects_by_type: HashMap<String, ObjectStats>,
    pub total_objects: u64,
    pub total_bytes: u64,
    /// Monotonic microseconds from the platform clock.
    pub timestamp_us: u64,
    pub coverage: SnapshotCoverage,
}
