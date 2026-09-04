//! 内联缓存（PIC）—— `VCallIC` / `FieldIC` 的槽位布局与查找/安装协议。
//!
//! review.md C4 P2 + C5 P2（jit-polymorphic-ic，2026-05-28）：4 槽多态内联缓存。
//! 与「token 解析」是两件事：token 解析是**每函数一次**的冷路径，PIC 是**每次派发**
//! 的热路径。父模块 `resolver` 只负责前者。

use std::sync::atomic::AtomicU32;
use crate::metadata::tokens::UNRESOLVED;

/// review.md C4 P2 + C5 P2 (jit-polymorphic-ic, 2026-05-28): 4-slot
/// polymorphic inline cache. `IC_SLOTS` is the lookup window per site.
/// Sites that observe ≤4 receiver types fast-path through linear scan
/// with early exit on `UNRESOLVED`. Sites with > 4 types victimize
/// via round-robin counter — the per-IC `round_robin` AtomicU32 ticks
/// on each install, modulo 4 picks the slot to overwrite.
pub const IC_SLOTS: usize = 4;

/// Single VCall PIC entry — (TypeId, vtable slot, target MethodId).
#[derive(Debug)]
pub struct VCallICEntry {
    pub type_id: AtomicU32,
    pub slot:    AtomicU32,
    pub fn_idx:  AtomicU32,
}

impl Default for VCallICEntry {
    fn default() -> Self {
        Self {
            type_id: AtomicU32::new(UNRESOLVED),
            slot:    AtomicU32::new(UNRESOLVED),
            fn_idx:  AtomicU32::new(UNRESOLVED),
        }
    }
}

/// Polymorphic inline cache for `VCall` sites. Linear scan through up to
/// `IC_SLOTS` (TypeId, slot, fn_idx) entries; first matching `type_id`
/// returns the cached dispatch. Sites that see < `IC_SLOTS` types skip
/// remaining slots via the `UNRESOLVED` early-exit sentinel.
///
/// Pre-2026-05-28 this was a single (type_id, slot, fn_idx) tuple —
/// mono only; polymorphic sites bounced between the cache and
/// `vtable_index` HashMap. PIC closes that gap for sites with ≤4
/// observed receiver types. Mono fast-path is preserved at zero cost
/// (the scan hits slot 0 in one compare).
///
/// Eviction: on miss past all `IC_SLOTS` filled entries, overwrite the
/// slot indexed by `round_robin.fetch_add(1, Relaxed) % IC_SLOTS`.
/// Round-robin sacrifices recency for the simplicity of a single atomic
/// increment per miss + zero overhead per hit.
#[derive(Debug)]
pub struct VCallIC {
    pub entries:     [VCallICEntry; IC_SLOTS],
    pub round_robin: AtomicU32,
}

impl Default for VCallIC {
    fn default() -> Self {
        Self {
            entries:     std::array::from_fn(|_| VCallICEntry::default()),
            round_robin: AtomicU32::new(0),
        }
    }
}

/// Single Field PIC entry — (TypeId, field slot).
#[derive(Debug)]
pub struct FieldICEntry {
    pub type_id: AtomicU32,
    pub slot:    AtomicU32,
}

impl Default for FieldICEntry {
    fn default() -> Self {
        Self {
            type_id: AtomicU32::new(UNRESOLVED),
            slot:    AtomicU32::new(UNRESOLVED),
        }
    }
}

/// Polymorphic inline cache for `FieldGet` / `FieldSet` sites. Mirrors
/// `VCallIC` PIC layout but caches just (TypeId, field slot). See
/// [`VCallIC`] for the linear-scan + round-robin-eviction protocol.
#[derive(Debug)]
pub struct FieldIC {
    pub entries:     [FieldICEntry; IC_SLOTS],
    pub round_robin: AtomicU32,
}

impl Default for FieldIC {
    fn default() -> Self {
        Self {
            entries:     std::array::from_fn(|_| FieldICEntry::default()),
            round_robin: AtomicU32::new(0),
        }
    }
}

// ── PIC lookup + install helpers (shared interp + JIT) ──────────────────────
//
// Inline lookup helpers used by both the interp dispatch (exec_object.rs +
// exec_vcall.rs) and the JIT helper bodies (helpers/object.rs, vcall.rs).
//
// SAFETY / atomic ordering: all loads / stores use `Ordering::Relaxed` —
// the type_id check gates payload use, so a torn read (type_id of slot A,
// payload of slot B) is bounded to "got wrong cached entry for a type that
// IS currently transitioning"; subsequent reads converge to a valid state.
// Same hazard the pre-PIC mono IC accepted.

/// PIC lookup for `FieldIC`. Returns `Some(slot)` on hit; `None` on miss
/// (caller must do `field_index.get(name)` fallback + `field_ic_install`).
#[inline]
pub fn field_ic_lookup(ic: &FieldIC, recv_type: u32) -> Option<u32> {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return None; }
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == recv_type { return Some(entry.slot.load(Relaxed)); }
        if tid == UNRESOLVED { return None; }  // early exit: rest are empty
    }
    None
}

/// PIC install for `FieldIC`. Finds the first `UNRESOLVED` slot; if all
/// filled, picks the round-robin victim. Idempotent for the same
/// `(recv_type, slot)` pair (just re-writes the same data).
#[inline]
pub fn field_ic_install(ic: &FieldIC, recv_type: u32, slot: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return; }
    // First-empty-slot install.
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == UNRESOLVED || tid == recv_type {
            entry.slot.store(slot, Relaxed);
            entry.type_id.store(recv_type, Relaxed);  // write type_id LAST
            return;
        }
    }
    // All filled — round-robin victim.
    let victim = (ic.round_robin.fetch_add(1, Relaxed) as usize) % IC_SLOTS;
    let entry = &ic.entries[victim];
    entry.slot.store(slot, Relaxed);
    entry.type_id.store(recv_type, Relaxed);
}

/// PIC lookup for `VCallIC`. Returns `Some((slot, fn_idx))` on hit.
#[inline]
pub fn vcall_ic_lookup(ic: &VCallIC, recv_type: u32) -> Option<(u32, u32)> {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return None; }
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == recv_type {
            return Some((entry.slot.load(Relaxed), entry.fn_idx.load(Relaxed)));
        }
        if tid == UNRESOLVED { return None; }
    }
    None
}

/// PIC install for `VCallIC`. Same protocol as `field_ic_install` but
/// writes a (slot, fn_idx) pair before publishing type_id.
#[inline]
pub fn vcall_ic_install(ic: &VCallIC, recv_type: u32, slot: u32, fn_idx: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return; }
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == UNRESOLVED || tid == recv_type {
            entry.slot.store(slot, Relaxed);
            entry.fn_idx.store(fn_idx, Relaxed);
            entry.type_id.store(recv_type, Relaxed);
            return;
        }
    }
    let victim = (ic.round_robin.fetch_add(1, Relaxed) as usize) % IC_SLOTS;
    let entry = &ic.entries[victim];
    entry.slot.store(slot, Relaxed);
    entry.fn_idx.store(fn_idx, Relaxed);
    entry.type_id.store(recv_type, Relaxed);
}
