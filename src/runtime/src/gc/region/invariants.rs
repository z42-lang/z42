//! Region debug invariants (`add-gc-debug-invariants` P0) — the `Violation`
//! taxonomy and `Region::check_invariants`.
//!
//! shrink-object-footprint: split out of `region.rs` (see `entry.rs`).

use std::sync::atomic::Ordering;

use super::{Region, CHUNK_SIZE, PROMOTION_THRESHOLD};

// ── add-gc-debug-invariants P0 (2026-05-22) ─────────────────────────────────

/// Per-region invariant violation. Returned by [`Region::validate`].
/// Variants 来自 add-write-barriers / add-custom-allocator /
/// add-concurrent-gc / add-generational-gc design 段的 invariants。
#[cfg(debug_assertions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// young_list 中找到 gen_age >= PROMOTION_THRESHOLD 的 entry
    /// (generational invariant).
    OldEntryInYoungList { chunk_idx: u32, entry_idx: u16, gen_age: u8 },
    /// alive young entry (gen_age < threshold) 不在 young_list 中
    /// (generational invariant).
    YoungEntryNotInList { chunk_idx: u32, entry_idx: u16 },
    /// young_list 中同一 (ci, ei) 出现多次（违反 swap_remove 契约）.
    DuplicateInYoungList { chunk_idx: u32, entry_idx: u16 },
    /// free_list 中找到 alive=true 的 slot（违反 tombstone 契约 —
    /// custom-allocator invariant）.
    AliveSlotInFreeList { chunk_idx: u32, entry_idx: u16 },
    /// `entry.location` 不等于实际 (chunk_idx, entry_idx)（自定位错乱 —
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
                write!(f, "young_list contains old entry (chunk={}, entry={}, gen_age={})",
                    chunk_idx, entry_idx, gen_age),
            Self::YoungEntryNotInList { chunk_idx, entry_idx } =>
                write!(f, "alive young entry not in young_list (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::DuplicateInYoungList { chunk_idx, entry_idx } =>
                write!(f, "duplicate in young_list (chunk={}, entry={})",
                    chunk_idx, entry_idx),
            Self::AliveSlotInFreeList { chunk_idx, entry_idx } =>
                write!(f, "free_list contains alive slot (chunk={}, entry={})",
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
    /// Cost: O(chunks * CHUNK_SIZE + young_list + free_list) =
    /// O(total slots). Acceptable on collect timescale (µs-ms);
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

        // 2. young_list: no duplicates, all gen_age < threshold,
        //    location matches.
        let mut in_young: std::collections::HashSet<(u32, u16)> =
            std::collections::HashSet::with_capacity(self.young_list.len());
        for &(ci, ei) in &self.young_list {
            if !in_young.insert((ci, ei)) {
                return Err(Violation::DuplicateInYoungList { chunk_idx: ci, entry_idx: ei });
            }
            // SAFETY: presence in young_list implies the slot was
            // initialized at alloc; we just read metadata.
            let entry = unsafe {
                self.chunks[ci as usize][ei as usize].assume_init_ref()
            };
            if entry.gen_age() >= PROMOTION_THRESHOLD {
                return Err(Violation::OldEntryInYoungList {
                    chunk_idx: ci, entry_idx: ei, gen_age: entry.gen_age(),
                });
            }
        }

        // 3. Walk every initialized entry: alive young must be in
        //    young_list; location must match.
        for (ci, chunk) in self.chunks.iter().enumerate() {
            // add-gc-tlab: borrowed chunks are mid-fill; skip (STW validate
            // never runs with a chunk borrowed, but stay defensive).
            if self.borrowed[ci] {
                continue;
            }
            for ei in 0..CHUNK_SIZE {
                if !self.initialized[ci][ei] {
                    continue;
                }
                let entry = unsafe { chunk[ei].assume_init_ref() };
                // Location self-consistency.
                if entry.location != (ci as u32, ei as u16) {
                    return Err(Violation::LocationMismatch {
                        chunk_idx: ci as u32, entry_idx: ei as u16,
                        recorded: entry.location,
                    });
                }
                // Alive young entries must be in young_list.
                if entry.alive.load(Ordering::Acquire)
                    && entry.gen_age() < PROMOTION_THRESHOLD
                    && !in_young.contains(&(ci as u32, ei as u16))
                {
                    return Err(Violation::YoungEntryNotInList {
                        chunk_idx: ci as u32, entry_idx: ei as u16,
                    });
                }
            }
        }

        // 4. free_list slots all alive=false.
        for &(ci, ei) in &self.free_list {
            let entry = unsafe {
                self.chunks[ci as usize][ei as usize].assume_init_ref()
            };
            if entry.alive.load(Ordering::Acquire) {
                return Err(Violation::AliveSlotInFreeList {
                    chunk_idx: ci, entry_idx: ei,
                });
            }
        }

        Ok(())
    }
}
