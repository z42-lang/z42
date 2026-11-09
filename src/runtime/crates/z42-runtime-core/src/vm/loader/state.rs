//! Per-VM loading and initialization-queue state.
use parking_lot::{Mutex, RwLock};
use std::sync::{Arc, atomic::AtomicUsize};
use crate::pal::fs::Vfs;
use super::{LazyLoader, contexts::ContextRegistry, fusion_inputs::FusionInputSource};

pub struct LoaderState {
    pub fusion_inputs: FusionInputSource,
    /// Shared reads are sufficient after symbol registration; loading takes the write lock.
    pub lazy_loader: RwLock<Option<LazyLoader>>,
    /// Owner type names queued by resolution, drained before use.
    pub pending_type_inits: Mutex<Vec<String>>,
    /// Mirror of the queue length, maintained under its lock. Only zero proves no work.
    pub pending_type_init_count: AtomicUsize,
    /// Covers the interval from before taking a batch until all its newly queued work is done.
    /// Without it, empty queues can falsely imply initialization has quiesced.
    pub init_batch_inflight: AtomicUsize,
    pub context_registry: Mutex<ContextRegistry>,
    /// Drop after the loader and contexts that use this injected file-system provider.
    pub vfs: Arc<dyn Vfs>,
}

impl LoaderState {
    pub fn new(vfs: Arc<dyn Vfs>, fusion_inputs: FusionInputSource) -> Self {
        Self {
            fusion_inputs,
            lazy_loader: RwLock::new(None),
            pending_type_inits: Mutex::new(Vec::new()),
            pending_type_init_count: AtomicUsize::new(0),
            init_batch_inflight: AtomicUsize::new(0),
            context_registry: Mutex::new(ContextRegistry::new()),
            vfs,
        }
    }
}
