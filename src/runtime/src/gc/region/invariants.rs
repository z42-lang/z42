//! Region debug invariants (`add-gc-debug-invariants` P0) — the `Violation`
//! taxonomy and `Region::check_invariants`.
//!
//! shrink-object-footprint: split out of `region.rs` (see `entry.rs`).

use std::sync::atomic::Ordering;

use super::{side_bits, Region, CHUNK_SIZE};

// ── add-gc-debug-invariants P0 (2026-05-22) ─────────────────────────────────

/// Per-region invariant violation. Returned by [`Region::validate`].
/// Variants 来自 add-write-barriers / add-custom-allocator /
/// add-generational-gc design 段的 invariants。
#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// 年轻集合里有 gen_age >= 晋升线的 entry（generational invariant）。
    OldEntryInYoungList { chunk_idx: u32, entry_idx: u16, gen_age: u8 },
    /// alive 的年轻 entry（gen_age < 晋升线）不在年轻集合里（generational invariant）。
    YoungEntryNotInList { chunk_idx: u32, entry_idx: u16 },
    /// 年轻集合里有未构造或已死的槽（tombstone 必须同步摘掉年轻位）。
    DeadSlotInYoungSet { chunk_idx: u32, entry_idx: u16 },
    /// chunk 摘要位与该 chunk 的年轻位图不一致（摘要漏置会让 minor 跳过年轻对象）。
    YoungSummaryDrift { chunk_idx: u32 },
    /// `young_len` 与年轻位图的置位总数不一致。
    YoungCountDrift { counted: usize, tracked: usize },
    /// 空闲集合里有 alive 或未构造的槽（违反 tombstone 契约 — custom-allocator invariant）。
    AliveSlotInFreeList { chunk_idx: u32, entry_idx: u16 },
    /// `free_chunks` 与 `free_bits` 不一致 — 一个 chunk 被列了两次、列着却没有空闲槽、
    /// 或有空闲槽却没列。
    FreeChunkIndexDrift { chunk_idx: u32 },
    /// `free_len` 与空闲位图的置位总数不一致。
    FreeSlotCountDrift { counted: usize, tracked: usize },
    /// `entry.location()` 不等于实际 (chunk_idx, entry_idx)（自定位错乱 —
    /// custom-allocator invariant）.
    LocationMismatch { chunk_idx: u32, entry_idx: u16, recorded: (u32, u16) },
    /// `card_dirty.len()` 与 `chunks.len()` 不一致（generational invariant；
    /// alloc-time grow 应保持一一对应）.
    CardDirtyLengthMismatch { expected: usize, actual: usize },
}

#[cfg(debug_assertions)]
impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OldEntryInYoungList { chunk_idx, entry_idx, gen_age } =>
                write!(f, "young set contains old entry (chunk={}, entry={}, gen_age={})",
                    chunk_idx, entry_idx, gen_age),
            Self::YoungEntryNotInList { chunk_idx, entry_idx } =>
                write!(f, "alive young entry not in the young set (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::DeadSlotInYoungSet { chunk_idx, entry_idx } =>
                write!(f, "young set contains a dead or unconstructed slot (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::YoungSummaryDrift { chunk_idx } =>
                write!(f, "young chunk summary disagrees with young_bits[{chunk_idx}]"),
            Self::YoungCountDrift { counted, tracked } =>
                write!(f, "young_len {tracked} but the young bits hold {counted}"),
            Self::FreeChunkIndexDrift { chunk_idx } =>
                write!(f, "free_chunks disagrees with free_bits[{chunk_idx}]"),
            Self::FreeSlotCountDrift { counted, tracked } =>
                write!(f, "free_len {tracked} but the free bits hold {counted}"),
            Self::AliveSlotInFreeList { chunk_idx, entry_idx } =>
                write!(f, "free set contains alive slot (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::LocationMismatch { chunk_idx, entry_idx, recorded } =>
                write!(f, "location mismatch at ({}, {}): entry.location = ({}, {})",
                    chunk_idx, entry_idx, recorded.0, recorded.1),
            Self::CardDirtyLengthMismatch { expected, actual } =>
                write!(f, "card_dirty length mismatch: expected {}, actual {}",
                    expected, actual),
        }
    }
}

impl<T> Region<T> {
    /// **add-gc-debug-invariants P0 (2026-05-22)**: validate region
    /// internal invariants. Returns `Ok(())` on a healthy region; the
    /// first violation found is returned as `Err(Violation)` so test
    /// fixtures can pattern-match a specific variant.
    ///
    /// Cost: O(chunks * CHUNK_SIZE) = O(total slots). Acceptable on collect timescale (µs-ms);
    /// would be too slow per-alloc.
    #[cfg(debug_assertions)]
    pub fn validate(&self) -> Result<(), Violation> {
        // 1. card_dirty length matches chunks count.
        if self.card_dirty.len() != self.chunks.len() {
            return Err(Violation::CardDirtyLengthMismatch {
                expected: self.chunks.len(),
                actual: self.card_dirty.len(),
            });
        }

        // 2. The young set and the free set, slot by slot, against the entries.
        let (mut young_counted, mut free_counted) = (0usize, 0usize);
        for ci in 0..self.chunks.len() {
            let young = &self.young_bits[ci];
            let free = &self.free_bits[ci];
            young_counted += side_bits::count(young);
            free_counted += side_bits::count(free);
            if side_bits::test(&self.young_chunks, ci) == side_bits::is_empty(young) {
                return Err(Violation::YoungSummaryDrift { chunk_idx: ci as u32 });
            }
            // add-gc-tlab: borrowed chunks are mid-fill; skip (STW validate never runs with a
            // chunk borrowed, but stay defensive).
            if self.borrowed[ci] {
                continue;
            }
            for ei in 0..CHUNK_SIZE {
                let (c, e) = (ci as u32, ei as u16);
                let init = side_bits::test(&self.init_bits[ci], ei);
                let in_young = side_bits::test(young, ei);
                if !init {
                    if in_young {
                        return Err(Violation::DeadSlotInYoungSet { chunk_idx: c, entry_idx: e });
                    }
                    if side_bits::test(free, ei) {
                        return Err(Violation::AliveSlotInFreeList { chunk_idx: c, entry_idx: e });
                    }
                    continue;
                }
                // SAFETY: the bit says the slot holds a constructed entry.
                let entry = unsafe { self.chunks[ci][ei].assume_init_ref() };
                if entry.location() != (c, e) {
                    return Err(Violation::LocationMismatch {
                        chunk_idx: c, entry_idx: e, recorded: entry.location(),
                    });
                }
                let alive = entry.alive.load(Ordering::Acquire);
                if alive && side_bits::test(free, ei) {
                    return Err(Violation::AliveSlotInFreeList { chunk_idx: c, entry_idx: e });
                }
                if in_young && !alive {
                    return Err(Violation::DeadSlotInYoungSet { chunk_idx: c, entry_idx: e });
                }
                // The line in force, not the configured constant: adaptive promotion lowers it
                // on top of a major, after which an entry at the old line's last tier is old.
                if in_young && entry.gen_age() >= self.promotion_age {
                    return Err(Violation::OldEntryInYoungList {
                        chunk_idx: c, entry_idx: e, gen_age: entry.gen_age(),
                    });
                }
                // Alive young entries must be in the set — but only when the region keeps one.
                if self.generational && alive && entry.gen_age() < self.promotion_age && !in_young {
                    return Err(Violation::YoungEntryNotInList { chunk_idx: c, entry_idx: e });
                }
            }
        }
        if young_counted != self.young_len {
            return Err(Violation::YoungCountDrift { counted: young_counted, tracked: self.young_len });
        }

        // 3. The index that makes the free-slot pop `O(1)` — `ci ∈ free_chunks ⟺
        // free_bits[ci] ≠ 0`, with no duplicates. A drifted index is silent otherwise: a
        // missing entry strands reusable slots (the region grows chunks it does not need), a
        // stale one makes `pop_free_slot` panic on an empty chunk.
        let mut listed = vec![false; self.free_bits.len()];
        for &ci in &self.free_chunks {
            if ci as usize >= listed.len() || listed[ci as usize] {
                return Err(Violation::FreeChunkIndexDrift { chunk_idx: ci });
            }
            listed[ci as usize] = true;
        }
        for (ci, bits) in self.free_bits.iter().enumerate() {
            if side_bits::is_empty(bits) == listed[ci] {
                return Err(Violation::FreeChunkIndexDrift { chunk_idx: ci as u32 });
            }
        }
        if free_counted != self.free_len {
            return Err(Violation::FreeSlotCountDrift {
                counted: free_counted, tracked: self.free_len,
            });
        }

        Ok(())
    }
}
