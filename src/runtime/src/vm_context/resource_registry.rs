use super::*;

/// **M2 (refactor-vm-context-resource-registry, 2026-08-24)** — the
/// `Mutex<HashMap<u64, T>>` + monotonic `AtomicU64` id-counter pattern shared
/// by every per-VM resource table (processes / threads / sync primitives /
/// file handles / TCP·TLS·UDP sockets). Bundling the lock, the table, and the
/// id counter into one type turns "add a new resource kind" from editing
/// [`VmCore`] in three places (table field + counter field + two `new()` init
/// lines) into adding a single field.
///
/// Slot ids are monotonic, **start at 1**, and are never reused (u64 overflow
/// is not a practical concern at ~10^19 allocations). [`lock`](Self::lock)
/// exposes the raw guard for the multi-step take-out / put-back sequences in
/// `corelib::network` / `corelib::fs` that must hold one lock across several
/// map operations (e.g. remove a socket, do blocking I/O without the table
/// lock, reinsert it).
pub(crate) struct ResourceRegistry<T> {
    table:   Mutex<HashMap<u64, T>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl<T> ResourceRegistry<T> {
    /// Empty registry; the id counter starts at 1 (slot 0 is never handed out,
    /// matching the historical per-table `AtomicU64::new(1)` init).
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
