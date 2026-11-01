//! **add-gc-softref **: soft-reference registry for `ArcMagrGC`.
//!
//! The registry tracks all live soft-reference targets (object and array blocks) in a
//! `Vec<ErasedSoftEntry>` the GC revive pass re-marks from.
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

/// Soft-ref GC-eligibility pressure ratio. Delegates to process-wide
/// [`crate::config::runtime_config()`] (runtime-config-phase2);
/// default 0.80, clamped `[0.0, 1.0]`, overridable via
/// `Z42_GC_SOFT_THRESHOLD`.
pub(crate) fn soft_threshold_from_env() -> f64 {
    crate::config::runtime_config().gc_soft_threshold
}

/// The target's kind — which `Value` to rebuild from the handle.
#[derive(Clone, Copy, Debug)]
pub(crate) enum ErasedKind {
    Object,
    Array,
}

/// A soft-ref registry entry: the target's block handle (object model R6 / R9 — every target is
/// a block of the variable-length region; the handle carries its own generation snapshot) and
/// its kind. Liveness is re-checked at revive time. The entry is dropped through the heap's
/// flagged-dead drain before the target's chunk can be reclaimed (the block carries `SIDE_BIT`),
/// so the handle always names a readable header.
#[derive(Clone)]
pub(crate) struct ErasedSoftEntry {
    block: crate::object::header::VarGcRef,
    pub(crate) kind: ErasedKind,
}

impl ErasedSoftEntry {
    /// An object target.
    pub(crate) fn object(obj: crate::object::ObjRef) -> Self {
        Self { block: obj.var_ref(), kind: ErasedKind::Object }
    }

    /// An array target.
    pub(crate) fn array(arr: crate::object::ArrRef) -> Self {
        Self { block: arr.var_ref(), kind: ErasedKind::Array }
    }

    /// The target's block.
    pub(crate) fn block(&self) -> crate::object::header::VarGcRef {
        self.block
    }

    /// The target as a `Value` (the recorded handle — safe against slot reuse).
    pub(crate) fn value(&self) -> crate::object::Value {
        match self.kind {
            ErasedKind::Object => crate::object::Value::Object(crate::object::ObjRef::from_var(self.block)),
            ErasedKind::Array => crate::object::Value::Array(crate::object::ArrRef::from_var(self.block)),
        }
    }

    /// The block-header address — the lookup / removal key.
    pub(crate) fn ptr_key(&self) -> usize {
        self.block.addr()
    }

    /// True iff the target is still alive (not tombstoned, same generation).
    pub(crate) fn is_alive(&self) -> bool {
        self.block.is_live()
    }

    /// Re-mark the target if it is alive and currently unmarked.
    /// Returns `true` if the entry was successfully revived.
    pub(crate) fn revive_if_unmarked(&self, major: crate::object::header::marks::MarkKind) -> bool {
        self.is_alive() && self.block.mark(major)
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

    /// Drop every entry whose target is the block at `key` (it died — see
    /// `ArcMagrGC::process_flagged_dead`).
    pub(crate) fn remove_all(&mut self, key: usize) {
        self.entries.retain(|e| e.ptr_key() != key);
    }

    /// Clone the current entry list so the caller can release the heap
    /// lock before doing revive work (which only needs block-header atomics).
    pub(crate) fn snapshot_entries(&self) -> Vec<ErasedSoftEntry> {
        self.entries.clone()
    }

    /// Revive pass over a pre-snapshotted entry list. Called outside
    /// the heap lock — `ErasedSoftEntry::revive_if_unmarked` only
    /// touches block-header atomics, no mutex needed.
    ///
    /// Returns the number of entries revived.
    /// `on_revived` sees each entry this pass marked — the incremental major queues it, so the
    /// target's children are traced too.
    pub(crate) fn revive_snapshot(
        entries: &[ErasedSoftEntry],
        used_bytes: u64,
        max_bytes: u64,
        major: crate::object::header::marks::MarkKind,
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
