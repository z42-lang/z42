//! Heap allocation, tracing, barriers, references and collection policy.

pub mod config;
pub use config::GcConfig;
pub mod to_vm;
pub mod heap;
pub mod walk;
pub mod mode;
pub mod footprint;
pub mod phase_timer;
pub mod satb;
pub(crate) mod side_bits;
pub mod tlab;
pub mod var_region;
pub mod soft_registry;
pub mod trace;
pub mod types;

pub use heap::Heap;
pub use mode::GcMode;


pub use types::{
    AllocKind, AllocSample, AllocSamplerFn, CollectStats, FinalizerFn, FrameMark,
    GcEvent, GcHandleKind, GcKind, GcObserver, HeapStats, ObserverId, RootHandle, WeakRef,
};

/// Minor collections a block must survive before it is promoted (old). The age is packed into two
/// bits of the block header's `type_tag`, so this default sits at the top of what fits
/// (`crate::object::header::MAX_GEN_AGE`); `Z42_GC_PROMOTION_AGE` can lower it.
pub const PROMOTION_THRESHOLD: u8 = 3;
