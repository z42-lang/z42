//! `IsaCache` — per-`VmContext` direct-mapped cache for type tests (`is` / `as` / typed
//! `catch`), in front of the `subclass_memo` map and the chain walk (`interp::dispatch`).
//!
//! Keyed by **ids**, never by addresses:
//!   * `recv`   — the receiver descriptor's `TypeId`. Process-global and never reused, so a
//!                key cannot alias another type after a descriptor is freed (a collectible
//!                load context, a REPL round). All versions of one type (the fixup's merged
//!                copy, the eager own-only copy) share the id and the name chain the verdict
//!                is computed from. Descriptors without an id (`UNRESOLVED`: transient
//!                fallbacks, native-handle singletons) are never cached — the caller skips.
//!   * `target` — the target's key (`tokens::TypeKeyCell`): its `TypeId`, or an id reserved
//!                for the target *name* (`TypeTable::name_key`). Same allocator, so it is
//!                unique process-wide and denotes exactly one name.
//!
//! Both ids are below `IMPORT_BASE` (`alloc_type_id_block` asserts it), so a slot is **one**
//! `AtomicU64`: `recv << 32 | verdict << 31 | target`. One store installs, one load reads —
//! the triple can never tear, and an empty slot (`u64::MAX`, receiver `UNRESOLVED`) never
//! matches. Hit = one relaxed load + one compare; no hashing, no lock. Direct-mapped with
//! overwrite on collision: a miss re-asks the memo and re-installs. Verdicts are monotonic
//! facts (a loaded type's chain never changes; lazy loading only adds types) — the same
//! invariant the memo relies on — and the cache is cleared with the memo on explicit module
//! (re)load (REPL redefinition, see `vm_context::lookup`).

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Slot count — power of two. 1024 × 8 B = 8 KiB per context.
const SLOTS: usize = 1024;
const SLOT_BITS: u32 = SLOTS.trailing_zeros();
/// Bit 31 of the slot word: the verdict (ids stay below `IMPORT_BASE` = `1 << 31`).
const VERDICT_BIT: u64 = 1 << 31;
const EMPTY: u64 = u64::MAX;

pub(crate) struct IsaCache {
    /// Allocated on the first `put` (8 KiB): a `VmContext` that never runs a type test —
    /// worker / embedding / test contexts — pays nothing and keeps `VmContext::new()`'s
    /// allocation profile unchanged.
    slots: std::sync::OnceLock<Box<[AtomicU64]>>,
}

impl std::fmt::Debug for IsaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsaCache").field("slots", &self.slots.get().map_or(0, |s| s.len())).finish()
    }
}

/// The `(recv, target)` pair as one word (verdict bit clear). Also the memo key.
#[inline]
pub(crate) fn pair_key(recv: u32, target: u32) -> u64 {
    debug_assert!(target < VERDICT_BIT as u32 && recv < VERDICT_BIT as u32, "isa key out of the TypeId band");
    (u64::from(recv) << 32) | u64::from(target)
}

impl IsaCache {
    pub(crate) fn new() -> Self {
        Self { slots: std::sync::OnceLock::new() }
    }

    fn alloc_slots() -> Box<[AtomicU64]> {
        (0..SLOTS).map(|_| AtomicU64::new(EMPTY)).collect::<Vec<_>>().into_boxed_slice()
    }

    /// Multiplicative hash of the packed pair; the top bits pick the slot.
    #[inline]
    pub(crate) fn index(key: u64) -> usize {
        (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - SLOT_BITS)) as usize
    }

    /// Cached verdict for (`recv`, `target`), if this exact pair was installed.
    #[inline]
    pub(crate) fn get(&self, recv: u32, target: u32) -> Option<bool> {
        let slots = self.slots.get()?;
        let key = pair_key(recv, target);
        let w = slots[Self::index(key)].load(Relaxed);
        if w & !VERDICT_BIT != key { return None; }
        Some(w & VERDICT_BIT != 0)
    }

    /// Install (overwrite on collision).
    #[inline]
    pub(crate) fn put(&self, recv: u32, target: u32, verdict: bool) {
        let slots = self.slots.get_or_init(Self::alloc_slots);
        let key = pair_key(recv, target);
        slots[Self::index(key)].store(key | if verdict { VERDICT_BIT } else { 0 }, Relaxed);
    }

    /// Forget everything (explicit module reload may redefine a type).
    pub(crate) fn clear(&self) {
        if let Some(slots) = self.slots.get() {
            for s in slots.iter() {
                s.store(EMPTY, Relaxed);
            }
        }
    }
}
