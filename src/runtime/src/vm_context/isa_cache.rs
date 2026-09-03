//! `IsaCache` — per-`VmContext` direct-mapped cache for type tests (`is` / `as` / typed
//! `catch`). perf-vm-isa-cache (2026-09-03, 三面评审 V-6).
//!
//! `dispatch::is_subclass_or_eq_td` already memoises the (derived, target) verdict in a
//! `Mutex<FxHashMap<String, FxHashMap<String, bool>>>` — correct, but every hit still costs a
//! lock + two string hashes over FQ class names (~60 ns). z42c's serializer dispatches each
//! IR instruction through a ~60-way `is`-chain, so that is the dominant per-instruction cost.
//!
//! This cache sits in front of the memo and keys on **identity**, not content:
//!   * `td`     — the receiver's `*const TypeDesc`. Registry / lazy-loader descriptors are
//!                immortal for the VM's lifetime, so the address is a stable identity.
//!                Transient fallback descriptors (`make_fallback_type_desc`, `id == UNRESOLVED`)
//!                are allocated per object and may be freed → **never cached** (caller skips).
//!   * `target` — the address of the class-name string. Callers pass only names that live in
//!                immortal metadata (`IsInstanceInsn.class_name` / `AsCastInsn.class_name` /
//!                exception-table `catch_type` / the same strings baked into JIT code) — never
//!                a heap string built at runtime (reflection paths keep using the memo).
//!
//! Hit = two relaxed loads + two compares; no hashing, no lock. Direct-mapped with overwrite
//! on collision (no chaining): a miss just re-asks the memo and re-installs. Verdicts are
//! monotonic facts (a loaded type's chain never changes; lazy loading only adds types) — the
//! same invariant the memo relies on — and the cache is cleared with the memo on explicit
//! module (re)load (REPL redefinition, see `vm_context::lookup`).
//!
//! Single writer: a `VmContext` is owned by one mutator thread; atomics only make the type
//! `Sync` for the shared `&VmContext`, they are not a cross-thread protocol.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};

/// Slot count — power of two so the index is a mask. 1024 × 16 B = 16 KiB per context.
const SLOTS: usize = 1024;
/// `tgt` word layout: bit 63 = verdict, bits 0..63 = target-name address (never 0 when set).
const VERDICT_BIT: u64 = 1 << 63;

struct Slot {
    td:  AtomicUsize,
    tgt: AtomicU64,
}

pub(crate) struct IsaCache {
    slots: Box<[Slot]>,
}

impl std::fmt::Debug for IsaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IsaCache").field("slots", &self.slots.len()).finish()
    }
}

impl IsaCache {
    pub(crate) fn new() -> Self {
        let slots = (0..SLOTS)
            .map(|_| Slot { td: AtomicUsize::new(0), tgt: AtomicU64::new(0) })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { slots }
    }

    #[inline]
    pub(crate) fn index(td: usize, target: usize) -> usize {
        // Both keys are pointers (16-byte aligned heap allocations → low bits are zero);
        // fold the informative bits of each before masking. Mix in u64 so the constant is
        // valid on 32-bit targets too (wasm32: `usize` is 32 bits).
        let h = ((td as u64) >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((target as u64) >> 3);
        ((h ^ (h >> 17)) as usize) & (SLOTS - 1)
    }

    /// Cached verdict for (`td`, `target`), if this exact pair was installed.
    #[inline]
    pub(crate) fn get(&self, td: *const crate::metadata::TypeDesc, target: &str) -> Option<bool> {
        let td = td as usize;
        let tgt = target.as_ptr() as usize as u64;
        let slot = &self.slots[Self::index(td, tgt as usize)];
        if slot.td.load(Relaxed) != td { return None; }
        let w = slot.tgt.load(Relaxed);
        if (w & !VERDICT_BIT) != tgt { return None; }
        Some(w & VERDICT_BIT != 0)
    }

    /// Install (overwrite on collision).
    #[inline]
    pub(crate) fn put(&self, td: *const crate::metadata::TypeDesc, target: &str, verdict: bool) {
        let td = td as usize;
        let tgt = target.as_ptr() as usize as u64;
        let slot = &self.slots[Self::index(td, tgt as usize)];
        // Publish the target word first so a stale `td` never pairs with a fresh verdict.
        slot.tgt.store(tgt | if verdict { VERDICT_BIT } else { 0 }, Relaxed);
        slot.td.store(td, Relaxed);
    }

    /// Forget everything (explicit module reload may redefine a type).
    pub(crate) fn clear(&self) {
        for s in self.slots.iter() {
            s.td.store(0, Relaxed);
            s.tgt.store(0, Relaxed);
        }
    }
}

