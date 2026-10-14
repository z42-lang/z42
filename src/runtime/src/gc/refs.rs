//! Mark bits shared by every GC block: [`MarkKind`] and the operations on a block's `marked`
//! byte. The handles themselves are [`ObjRef`](crate::gc::ObjRef) / [`ArrRef`](crate::gc::ArrRef)
//! / `VarGcRef` (object model R6 / R9 — every managed value is a variable-length block).

use std::sync::atomic::Ordering;

/// **add-incremental-major-gc M1 (2026-09-15)**: which mark a mark-phase operation reads or
/// writes. Both kinds share one `marked` byte per block (`GcBlockHeader`):
///
/// ```text
///   bit 0      minor mark        — set by the minor BFS, cleared by the minor sweep
///   bits 1..=7 major epoch (1..=127, 0 = never major-marked)
/// ```
///
/// "Major-marked" means **the stored epoch equals the current cycle's epoch**, so starting a
/// cycle (advancing the heap's epoch) makes every slot white at once — there is no reset pass,
/// and a stale major mark cannot exist. That rests on one invariant: every major sweep visits
/// every alive slot, tombstoning the ones whose epoch is not current, so after cycle `E` every
/// alive slot holds `E` or `0`; the next epoch is neither. See `next_epoch`.
///
/// The two kinds never touch each other's bits, which is what lets a minor run while a major
/// cycle's marks are outstanding (add-incremental-major-gc M2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Minor,
    Major(u8),
}

/// Largest major epoch; `next_epoch` wraps from here back to 1.
pub const MAX_MARK_EPOCH: u8 = 127;
const MINOR_BIT: u8 = 1;

/// The epoch after `cur`, skipping 0 (reserved for "never major-marked").
#[inline]
pub fn next_epoch(cur: u8) -> u8 {
    if cur >= MAX_MARK_EPOCH { 1 } else { cur + 1 }
}

/// Mark `cell` for `kind`, leaving the other kind's bits alone. Returns `true` iff this call
/// made the transition (the caller is the one that should trace the slot's children).
#[inline]
pub(crate) fn mark_cell(cell: &std::sync::atomic::AtomicU8, kind: MarkKind) -> bool {
    debug_assert!(!matches!(kind, MarkKind::Major(0)), "major epoch 0 is reserved");
    let mut cur = cell.load(Ordering::Relaxed);
    loop {
        let next = match kind {
            MarkKind::Minor => {
                if cur & MINOR_BIT != 0 { return false; }
                cur | MINOR_BIT
            }
            MarkKind::Major(epoch) => {
                if cur >> 1 == epoch { return false; }
                (epoch << 1) | (cur & MINOR_BIT)
            }
        };
        match cell.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(actual) => cur = actual,
        }
    }
}

/// Whether `cell` carries `kind`'s mark.
#[inline]
pub(crate) fn is_marked_cell(cell: &std::sync::atomic::AtomicU8, kind: MarkKind) -> bool {
    let cur = cell.load(Ordering::Relaxed);
    match kind {
        MarkKind::Minor => cur & MINOR_BIT != 0,
        MarkKind::Major(epoch) => cur >> 1 == epoch,
    }
}

/// Clear the minor bit only (the minor sweep on a survivor). The major epoch is left in place.
#[inline]
pub(crate) fn clear_minor_cell(cell: &std::sync::atomic::AtomicU8) {
    cell.fetch_and(!MINOR_BIT, Ordering::Relaxed);
}

/// The major epoch stored in `cell` (diagnostics / invariant checks).
#[inline]
pub(crate) fn major_epoch_of_cell(cell: &std::sync::atomic::AtomicU8) -> u8 {
    cell.load(Ordering::Relaxed) >> 1
}
