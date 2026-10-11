//! `Region<T>`'s share of the heap's true-footprint accounting (`gc::footprint`). What each
//! mutation charges lives next to the mutation in `region.rs` / `claim.rs`; this file holds the
//! constants, the hookup, and the side-table re-measure.

use std::sync::Arc;

use super::*;
use crate::gc::footprint::{Footprint, REGION_CHUNK_TABLES};

impl<T> Region<T> {
    /// What one chunk costs while it exists: its slot array and its share of the per-chunk
    /// tables (bitmaps included). Chunks are never freed, so this is charged once at grow.
    pub(crate) const CHUNK_FOOTPRINT: u64 =
        std::mem::size_of::<[MaybeUninit<RegionEntry<T>>; CHUNK_SIZE]>() as u64 + REGION_CHUNK_TABLES;

    /// Account into the heap's `footprint` from now on, measuring each value's out-of-slot
    /// payload with `payload_of`. Must run before the first allocation (the heap calls it at
    /// construction), so nothing charged so far is lost.
    pub fn attach_footprint(&mut self, footprint: Arc<Footprint>, payload_of: fn(&T) -> u64) {
        debug_assert!(self.chunks.is_empty(), "attach the footprint before allocating");
        self.footprint = footprint;
        self.payload_of = payload_of;
    }

    /// Re-measure the variable-size side tables — the chunk lists (`free_chunks`,
    /// `free_chunk_pool`) — and charge the difference since the last reading. Runs at the sweep
    /// tail, next to `reclaim_dead_chunks`. The per-slot tables (constructed / young / free
    /// bits) are fixed per chunk and charged with it in [`Self::CHUNK_FOOTPRINT`].
    pub fn refresh_side_tables(&mut self) {
        let now = ((self.free_chunks.capacity() + self.free_chunk_pool.capacity())
            * std::mem::size_of::<u32>()) as u64;
        self.footprint.apply(now as i64 - self.side_accounted as i64);
        self.side_accounted = now;
    }
}
