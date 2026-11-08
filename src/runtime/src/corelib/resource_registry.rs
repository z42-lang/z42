use parking_lot::Mutex;
use std::collections::HashMap;

/// Per-VM resource slots: IDs start at 1, are monotonic, and are never reused.
/// Blocking I/O clones Arc handles out under the table lock, then releases the lock.
pub(crate) struct ResourceRegistry<T> {
    table:   Mutex<HashMap<u64, T>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl<T> ResourceRegistry<T> {
    /// Empty registry; the id counter starts at 1 (slot 0 is never handed out,
    /// reserved for invalid handles).
    pub(crate) fn new() -> Self {
        Self {
            table:   Mutex::new(HashMap::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Bump the monotonic counter and return a fresh id **without** inserting.
    /// For call sites that compute the id first, then insert under their own
    /// lock (e.g. the network accept/read paths).
    pub(crate) fn alloc_id(&self) -> u64 {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Allocate a fresh id and insert `v` under it in one step. Returns the id.
    pub(crate) fn insert_new(&self, v: T) -> u64 {
        let id = self.alloc_id();
        self.table.lock().insert(id, v);
        id
    }

    /// Remove and return the slot's value (`None` if the id is unknown).
    pub(crate) fn take(&self, id: u64) -> Option<T> {
        self.table.lock().remove(&id)
    }

    /// Run `f` against a mutable reference to the slot in place; `None` if the
    /// id is unknown. The table lock is held for the duration of `f`.
    pub(crate) fn with_mut<R>(&self, id: u64, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.table.lock().get_mut(&id).map(f)
    }

    /// Number of live slots.
    pub(crate) fn count(&self) -> usize {
        self.table.lock().len()
    }

    /// Raw guard escape hatch for multi-step map operations that must run under
    /// a single lock acquisition (iterate / take-out-then-put-back / explicit-id
    /// insert). Prefer the semantic methods above where they fit.
    pub(crate) fn lock(&self) -> parking_lot::MutexGuard<'_, HashMap<u64, T>> {
        self.table.lock()
    }
}

impl<T: Clone> ResourceRegistry<T> {
    /// Clone the slot's value out (`None` if unknown). Used by the Arc-wrapped
    /// sync primitives (`Mutex` / `RwLock`) whose handlers clone the `Arc` out
    /// and operate on the inner lock without holding the registry lock.
    pub(crate) fn get_cloned(&self, id: u64) -> Option<T> {
        self.table.lock().get(&id).cloned()
    }
}

impl<T> Default for ResourceRegistry<T> {
    fn default() -> Self { Self::new() }
}

/// Mandatory state is installed by the facade before a VM can execute builtins.
pub(super) fn state<T: std::any::Any + Send + Sync>(
    ctx: &crate::vm::context::VmContext,
) -> Result<&T, crate::vm::exec::call::builtin_abi::BuiltinError> {
    ctx.ext::<T>().ok_or_else(|| crate::vm::exec::call::builtin_abi::BuiltinError::Internal(
        format!("missing core-library VM state `{}`", std::any::type_name::<T>())))
}

#[cfg(test)]
#[path = "resource_registry_tests.rs"]
mod resource_registry_tests;
