//! **add-gc-softref (2026-05-26)**: soft-reference registry for `ArcMagrGC`.
//!
//! The registry tracks all live `SoftGcRef` targets in a type-erased
//! `Vec<ErasedSoftEntry>` so the GC revive pass can re-mark entries
//! without holding a typed `GcRef<T>`.
//!
//! # Revive-pass algorithm
//!
//! Called between `mark_phase` and `sweep_phase`:
//!
//! 1. Compute `used_ratio = used_bytes / max_bytes` (0.0 if unlimited).
//! 2. Read `Z42_GC_SOFT_THRESHOLD` (default 0.80).
//! 3. If `used_ratio < threshold`: for each alive entry in the registry
//!    that is currently unmarked, re-mark it so sweep keeps it.
//! 4. If `used_ratio >= threshold`: do nothing — entries that are only
//!    soft-reachable stay unmarked and are swept normally.
//! 5. After sweep, `prune_dead()` removes entries whose target was tombstoned.
//!
//! # Thread safety
//!
//! `SoftRegistry` is `!Send + !Sync` — it lives inside
//! `ArcMagrGC.inner: Mutex<RcHeapInner>` and is only accessed while
//! that lock is held (STW collect / alloc / drop). All operations are
//! thus single-threaded at call time.

use std::ptr::NonNull;
use std::sync::atomic::Ordering;

use super::region::RegionEntry;

/// Soft-ref GC-eligibility pressure ratio. Delegates to process-wide
/// [`crate::config::runtime_config()`] (runtime-config-phase2 2026-06-03);
/// default 0.80, clamped `[0.0, 1.0]`, overridable via
/// `Z42_GC_SOFT_THRESHOLD`.
pub(crate) fn soft_threshold_from_env() -> f64 {
    crate::config::runtime_config().gc_soft_threshold
}

/// Which region owns the target — needed to re-construct the right
/// `Value` variant if the revive pass needed to produce a `Value`
/// (not needed currently; kept for future extension).
#[derive(Clone, Copy, Debug)]
pub(crate) enum ErasedKind {
    /// An object block (object model R6) — its handle (a pointer, so provenance is kept).
    Object { obj: crate::gc::ObjRef },
    Array,
}

/// Type-erased soft-ref entry. Stores a raw pointer to the target — a `RegionEntry<ArrayObj>`
/// for an array, the block header for an object (object model R6) — plus the generation
/// snapshot taken at construction time.
/// Does NOT increment soft_ref_count — that is the responsibility of
/// `SoftGcRef<T>`. The registry is purely a set of "entries to
/// consider for revive"; liveness is re-checked at revive time.
#[derive(Clone)]
pub(crate) struct ErasedSoftEntry {
    /// Raw pointer to the `RegionEntry<T>` (type-erased as `*mut u8`).
    /// Stable for region lifetime (chunks are Box-owned).
    ptr: NonNull<u8>,
    generation: u32,
    pub(crate) kind: ErasedKind,
}

// SAFETY: ErasedSoftEntry is accessed only while ArcMagrGC.inner Mutex
// is held. The raw pointer is stable (Region chunks never relocate).
unsafe impl Send for ErasedSoftEntry {}
unsafe impl Sync for ErasedSoftEntry {}

impl ErasedSoftEntry {
    /// An object target (object model R6): the handle itself (it carries its own generation
    /// snapshot); `ptr` is its block header, the key the flagged-dead drain removes it by.
    pub(crate) fn from_object(obj: crate::gc::ObjRef) -> Self {
        let ptr = obj.var_ref().header_raw();
        Self { ptr, generation: 0, kind: ErasedKind::Object { obj } }
    }

    /// The object handle of an [`ErasedKind::Object`] entry.
    pub(crate) fn object(&self) -> Option<crate::gc::ObjRef> {
        match self.kind {
            // The entry is dropped (via the heap's flagged-dead drain) before the object's chunk
            // can be reclaimed, so the handle always names a readable header.
            ErasedKind::Object { obj } => Some(obj),
            ErasedKind::Array => None,
        }
    }

    pub(crate) fn from_array(ptr: NonNull<RegionEntry<crate::metadata::types::ArrayObj>>, generation: u32) -> Self {
        Self { ptr: ptr.cast(), generation, kind: ErasedKind::Array }
    }

    /// Pointer as usize — used as lookup / removal key.
    pub(crate) fn ptr_key(&self) -> usize {
        self.ptr.as_ptr() as usize
    }

    /// Generation recorded at registration time. Used by `soft_ref_get`
    /// to construct a `GcRef` without re-reading the (potentially-reused)
    /// current generation.
    pub(crate) fn generation_snapshot(&self) -> u32 {
        self.generation
    }

    /// True iff the target entry is still alive (not tombstoned + same
    /// generation). Checked before revive and during `prune_dead`.
    pub(crate) fn is_alive(&self) -> bool {
        // SAFETY: ptr was derived from a live RegionEntry whose region
        // outlives the registry. We only touch atomic fields.
        match self.kind {
            // An object's entry is dropped when its block is tombstoned (`process_flagged_dead`),
            // before the chunk can be reclaimed, so its header is always readable here.
            ErasedKind::Object { .. } => self.object().is_some_and(|o| o.var_ref().is_live()),
            ErasedKind::Array => {
                let e = unsafe { &*(self.ptr.as_ptr() as *const RegionEntry<crate::metadata::types::ArrayObj>) };
                e.alive.load(Ordering::Acquire) && e.generation.load(Ordering::Acquire) == self.generation
            }
        }
    }

    /// Re-mark the target if it is alive and currently unmarked.
    /// Returns `true` if the entry was successfully revived.
    pub(crate) fn revive_if_unmarked(&self, major: crate::gc::refs::MarkKind) -> bool {
        if !self.is_alive() { return false; }
        match self.kind {
            ErasedKind::Object { .. } => self.object().is_some_and(|o| crate::gc::ObjRef::mark(&o, major)),
            ErasedKind::Array => {
                let e = unsafe { &*(self.ptr.as_ptr() as *const RegionEntry<crate::metadata::types::ArrayObj>) };
                e.mark(major)
            }
        }
    }
}

/// Registry of all current `SoftGcRef` targets.
#[derive(Default)]
pub(crate) struct SoftRegistry {
    entries: Vec<ErasedSoftEntry>,
}

impl SoftRegistry {
    /// Insert a new entry. The key is the raw pointer (ptr_key).
    pub(crate) fn insert(&mut self, entry: ErasedSoftEntry) {
        self.entries.push(entry);
    }

    /// Remove all entries whose ptr_key matches `key`. There may be
    /// multiple if the same slot was soft-referenced by several handles
    /// (each creates its own `ErasedSoftEntry`); remove only one to
    /// keep the count correct (mirrors one `unregister_soft_ref` call
    /// per `register_soft_ref` call).
    pub(crate) fn remove_one(&mut self, key: usize) {
        if let Some(pos) = self.entries.iter().position(|e| e.ptr_key() == key) {
            self.entries.swap_remove(pos);
        }
    }

    /// Drop every entry whose target is the block at `key` (object model R6: the object died —
    /// see `ArcMagrGC::process_flagged_dead`).
    pub(crate) fn remove_all(&mut self, key: usize) {
        self.entries.retain(|e| e.ptr_key() != key);
    }

    /// Clone the current entry list so the caller can release the heap
    /// lock before doing revive work (which only needs RegionEntry atomics).
    pub(crate) fn snapshot_entries(&self) -> Vec<ErasedSoftEntry> {
        self.entries.clone()
    }

    /// Revive pass over a pre-snapshotted entry list. Called outside
    /// the heap lock — `ErasedSoftEntry::revive_if_unmarked` only
    /// touches atomic fields on `RegionEntry`, no mutex needed.
    ///
    /// Returns the number of entries revived.
    /// `on_revived` sees each entry this pass marked — the incremental major queues it, so the
    /// target's children are traced too.
    pub(crate) fn revive_snapshot(
        entries: &[ErasedSoftEntry],
        used_bytes: u64,
        max_bytes: u64,
        major: crate::gc::refs::MarkKind,
        mut on_revived: impl FnMut(&ErasedSoftEntry),
    ) -> usize {
        let threshold = soft_threshold_from_env();
        let ratio = if max_bytes == 0 {
            0.0_f64
        } else {
            used_bytes as f64 / max_bytes as f64
        };
        if ratio >= threshold {
            return 0;
        }
        let mut revived = 0usize;
        for entry in entries {
            if entry.revive_if_unmarked(major) {
                on_revived(entry);
                revived += 1;
            }
        }
        revived
    }

    /// Remove entries whose target was tombstoned by sweep. Called
    /// after `sweep_phase` to keep the registry compact.
    pub(crate) fn prune_dead(&mut self) {
        self.entries.retain(|e| e.is_alive());
    }

}
