//! Heap allocation, tracing, barriers, references and collection policy.
//!
//! Implementation submodules (heap internals, epochs, SATB/TLS state, region
//! layout, tracing) stay crate-internal; cross-engine vocabulary is the root
//! re-exports plus `walk` / `satb`.
//! ```compile_fail
//! use z42::gc::footprint;
//! ```

pub(crate) mod config;
pub use config::GcConfig;
pub(crate) mod to_vm;
pub mod ambient;
pub(crate) mod heap;
pub mod walk;
pub(crate) mod mode;
pub(crate) mod footprint;
pub(crate) mod phase_timer;
pub mod satb;
pub(crate) mod side_bits;
pub(crate) mod tlab;
pub(crate) mod var_region;
pub(crate) mod soft_registry;
pub(crate) mod trace;
pub(crate) mod types;

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
