//! `ArcMagrGC`'s side of true-footprint accounting (`gc::footprint`): the payload measures its
//! regions charge with, the readings, and the soft cap restated for the allowance math.

use std::sync::atomic::Ordering;

use crate::metadata::types::ArrayObj;
use crate::metadata::ScriptObject;

/// `region_object`'s payload measure: the field block and extras outside the slot.
pub(super) fn object_payload(o: &ScriptObject) -> u64 {
    o.heap_payload_bytes()
}

/// `region_array`'s payload measure: the element-type name, an `Arc<str>` of its own per
/// array (two counters + the bytes). The element storage is a `region_var` block, charged
/// there as chunk memory.
pub(super) fn array_payload(a: &ArrayObj) -> u64 {
    crate::gc::footprint::malloc_size(2 * std::mem::size_of::<usize>() + a.element_type.len())
}

/// A soft cap in both units the policy needs: `footprint` is the cap as configured — judged
/// against [`ArcMagrGC::occupied_bytes`]; `used` is the same cap restated in `used_bytes` units
/// (scaled by the heap's current footprint-per-used-byte), which is what the allowance math is
/// written in.
#[derive(Clone, Copy, Debug)]
pub(super) struct SoftCap {
    pub(super) footprint: u64,
    pub(super) used: u64,
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// Everything the heap holds right now — `HeapStats::committed_bytes`. One atomic load.
    #[inline]
    pub(super) fn committed_bytes(&self) -> u64 {
        self.footprint.committed()
    }

    /// What the soft cap is judged against: committed memory minus the empty chunks waiting in
    /// the pools (those refill without growing the footprint, and collecting cannot shrink them).
    #[inline]
    pub(super) fn occupied_bytes(&self) -> u64 {
        self.footprint.occupied()
    }

    /// The configured soft cap (`Z42_GC_MAX_BYTES` / `set_max_heap_bytes`), lock-free.
    #[inline]
    pub(super) fn soft_cap_bytes(&self) -> Option<u64> {
        match self.max_bytes_atomic.load(Ordering::Relaxed) {
            u64::MAX => None,
            n => Some(n),
        }
    }

    /// The soft cap in both units (see [`SoftCap`]). The `used` view scales the cap by
    /// `used / occupied`: if every live byte currently costs `r` bytes of footprint, a cap of
    /// `C` footprint bytes fits `C / r` used bytes. Never scaled *up* — a heap reading less
    /// footprint than `used` (possible right after a sweep credits estimates the slots have not
    /// given back yet) keeps the cap as is.
    pub(super) fn soft_cap(&self) -> Option<SoftCap> {
        let cap = self.soft_cap_bytes()?;
        let used = self.used_bytes_atomic();
        let occupied = self.occupied_bytes();
        let in_used = if used == 0 || occupied <= used {
            cap
        } else {
            ((cap as u128 * used as u128) / occupied as u128) as u64
        };
        Some(SoftCap { footprint: cap, used: in_used })
    }

    /// Whether the heap is at its soft cap's near-limit ratio (`Z42_GC_NEAR_LIMIT_RATIO`).
    pub(super) fn near_soft_cap(&self, cap: Option<SoftCap>, near_ratio: f64) -> bool {
        cap.is_some_and(|c| self.occupied_bytes() >= (c.footprint as f64 * near_ratio) as u64)
    }

    /// The sweep tail every collection path shares: pool the fully-dead chunks of all three
    /// regions, then re-measure their side tables (both `O(chunks)`).
    pub(super) fn reclaim_dead_chunks_and_measure(&self) {
        {
            let mut r = self.region_object.lock();
            r.reclaim_dead_chunks();
            r.refresh_side_tables();
        }
        {
            let mut r = self.region_array.lock();
            r.reclaim_dead_chunks();
            r.refresh_side_tables();
        }
        let mut r = self.region_var.lock();
        r.reclaim_dead_var_chunks();
        r.refresh_side_tables();
    }
}
