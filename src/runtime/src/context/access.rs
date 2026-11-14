//! Borrowed access to the shared VM state and thread-local controls.
use super::*;

impl VmContext {
    /// Public accessor for the shared compiled Module installed by
    /// [`with_module`](Self::with_module). Returns `None` if VmContext was
    /// built via [`new`](Self::new) (test path).
    pub fn module(&self) -> Option<&Arc<crate::metadata::Module>> {
        self.core.runtime.module.as_ref()
    }

    /// The VM's function table (per-VM `FnId`s; see `metadata::ids::func_table`).
    #[inline]
    pub fn funcs(&self) -> &crate::metadata::ids::func_table::FuncTable {
        &self.core.runtime.funcs
    }

    /// The VM's type table (process-global `TypeId`s; see `metadata::ids::type_table`).
    #[inline]
    pub fn types(&self) -> &crate::metadata::ids::type_table::TypeTable {
        &self.core.runtime.types
    }

    /// Clone the shared `Arc<VmCore>` — needed by external integration
    /// tests / embedders that spawn raw OS threads and want to construct
    /// a child VmContext via [`new_with_core`](Self::new_with_core). The
    /// `core` field itself is `pub`; this is the public escape hatch.
    pub fn core_arc(&self) -> Arc<VmCore> {
        Arc::clone(&self.core)
    }

    /// add-gc-safepoint-counter-throttling : force the next
    /// `check_safepoint` call into the slow path immediately, bypassing
    /// the throttle counter. For tests and embedders that need
    /// deterministic safepoint timing — production code should not need
    /// this (the throttle counter caps GC pause latency at N iterations
    /// which is small enough in practice).
    pub fn force_safepoint(&self) {
        self.threads.safepoint_skip.store(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Borrow this VM's extension state by its exact Rust type, without a new lock.
    /// Returns `None` when the type was not registered before VM creation.
    pub fn ext<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.core.ext_state.get::<T>()
    }

    /// The file system this VM reads and writes through.
    pub fn vfs(&self) -> &Arc<dyn crate::pal::fs::Vfs> {
        &self.core.loader.vfs
    }

}
