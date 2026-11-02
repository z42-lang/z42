//! Heap allocation, tracing, barriers, references and collection policy.

pub mod config;
pub use config::GcConfig;
pub mod to_vm;
pub mod heap;
pub mod mode;
pub mod footprint;
pub mod phase_timer;
pub mod satb;
pub(crate) mod side_bits;
pub mod tlab;
pub mod var_region;
pub mod retention;
pub mod sampler;
pub mod snapshot;
pub mod soft_registry;
pub mod trace;
pub mod types;

pub use heap::Heap;
pub use sampler::Sampler;
pub use mode::GcMode;

pub use snapshot::{
    build_graph_snapshot, serialize_v8_heapsnapshot, EdgeType, GraphSnapshot, NodeType,
};
pub use types::{
    AllocKind, AllocSample, AllocSamplerFn, CollectStats, FinalizerFn, FrameMark,
    GcEvent, GcHandleKind, GcKind, GcObserver, HeapSnapshot, HeapStats, ObjectStats,
    ObserverId, RootHandle, SnapshotCoverage, WeakRef,
};

/// Minor collections a block must survive before it is promoted (old). The age is packed into two
/// bits of the block header's `type_tag`, so this default sits at the top of what fits
/// (`crate::object::header::MAX_GEN_AGE`); `Z42_GC_PROMOTION_AGE` can lower it.
pub const PROMOTION_THRESHOLD: u8 = 3;
