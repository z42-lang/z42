//! Block layout: the payload-kind tag, the 16-byte block header, and the payload-pointer /
//! finalizer plumbing shared by the allocator (`super`), the chunk layer (`super::chunk`) and
//! the handle (`super::var_ref`). See the module docs of `gc::var_region` for the block model.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Payload kind of a variable-length GC block. Since a `VarRegion` mixes payload types in
/// one allocator, the block header records which kind it holds so the GC tracer knows how to
/// scan the payload (leaf bytes vs. inline `Value`s vs. closure fields). PR-1 only tags them;
/// the actual per-kind tracing lands with each payload migration (PR-2…PR-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockType {
    /// UTF-8 string bytes (immutable leaf — no outgoing references). PR-2.
    Str = 0,
    /// `[Value; n]` reference-array elements (each element is a traced edge). PR-4.
    ArrayValue = 1,
    /// Packed primitive array bytes (leaf — no references). PR-4.
    ArrayPrim = 2,
    /// `struct[]` inline bytes + reference-leaf bitmap (mixed). PR-4.
    ArrayStruct = 3,
    /// `ClosureData` fields (env edge + fn_name string edge). PR-3.
    Closure = 4,
}

impl BlockType {
    /// Reconstruct from the raw `u8` stored in a header. Returns `None` on an unknown tag
    /// (corruption guard — a valid block always carries one of the variants above).
    #[inline]
    pub(super) fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Str),
            1 => Some(Self::ArrayValue),
            2 => Some(Self::ArrayPrim),
            3 => Some(Self::ArrayStruct),
            4 => Some(Self::Closure),
            _ => None,
        }
    }
}

/// Fixed header preceding a variable-length block's inline payload. `#[repr(C, align(8))]`
/// pins the field order and pads to 16 bytes so the payload always starts 8-aligned at
/// `DATA_OFFSET` (a `Value` element needs 8-alignment).
#[repr(C, align(8))]
pub struct GcBlockHeader {
    /// Generation counter (ABA guard). Bumped on every tombstone. A [`VarGcRef`] snapshots
    /// the low 16 bits at construction; a mismatch on resolve means the slot was reclaimed +
    /// reused → stale handle. Mirrors `RegionEntry::generation`.
    pub(super) generation: AtomicU32,
    /// Payload byte length (immutable after alloc). Note this is the *requested* payload
    /// size; the slot's physical capacity is `size_class`'s power-of-two footprint, which
    /// may be larger.
    pub(super) size: u32,
    /// Mark bit (0 = unmarked). CAS 0→1 by the mark phase; reset by sweep on survivors.
    pub(super) marked: AtomicU8,
    /// Tombstone flag: `true` while live, `false` after sweep reclaims the slot.
    pub(super) alive: AtomicBool,
    /// Payload kind ([`BlockType`] as `u8`) — tells the tracer how to scan the payload.
    pub(super) type_tag: u8,
    /// Size-class index (`log2(total_footprint)`), or [`OVERSIZED_CLASS`] for a dedicated
    /// chunk. Lets tombstone return the slot to the right free list and lets iteration know
    /// the slot's footprint.
    pub(super) size_class: u8,
}

// The header is exactly 16 bytes so the inline payload begins 8-aligned. This mirrors
// `vstr::StrHeader` (also 16 B) — deliberately, so a GC string block and the current
// thin-Arc string have identical payload offsets, easing the PR-2 migration.
const _: () = assert!(std::mem::size_of::<GcBlockHeader>() == 16);
const _: () = assert!(std::mem::align_of::<GcBlockHeader>() == 8);

impl GcBlockHeader {
    /// Byte offset of the inline payload within the allocation = the (padded) header size.
    pub const DATA_OFFSET: usize = std::mem::size_of::<GcBlockHeader>();

    /// Payload byte length (as requested at alloc).
    #[inline]
    pub fn size(&self) -> usize {
        self.size as usize
    }

    /// Payload kind.
    #[inline]
    pub fn block_type(&self) -> BlockType {
        // A live block always carries a valid tag (set at alloc); fall back to `Str` only to
        // avoid a panic on a corrupted read (debug builds assert instead).
        debug_assert!(BlockType::from_u8(self.type_tag).is_some(), "corrupt block type_tag");
        BlockType::from_u8(self.type_tag).unwrap_or(BlockType::Str)
    }

    /// True while the block is live (not yet swept).
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Attempt to mark this block (0 → 1). Returns `true` if this call won the CAS.
    #[inline]
    pub fn mark(&self) -> bool {
        self.marked
            .compare_exchange(0, 1, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    }

    /// Read the mark bit.
    #[inline]
    pub fn is_marked(&self) -> bool {
        self.marked.load(Ordering::Relaxed) != 0
    }

    /// Reset the mark bit (sweep on survivors).
    #[inline]
    pub fn clear_mark(&self) {
        self.marked.store(0, Ordering::Relaxed);
    }

    /// Current generation (full 32 bits — for tests / the handle guard).
    #[inline]
    pub(super) fn generation(&self) -> u32 {
        self.generation.load(Ordering::Acquire)
    }
}

/// Raw pointer to a block's inline payload bytes, derived from the **header pointer**
/// (whole-allocation provenance) — deliberately NOT from a `&GcBlockHeader` reference.
///
/// A `&GcBlockHeader` reborrow narrows provenance to the 16-byte header, so deriving the
/// payload pointer (at offset 16) through it and then accessing the payload is out-of-bounds
/// under Stacked Borrows (Miri UB — caught during PR-1). The header + payload are one
/// allocation, and `header` still carries the chunk-allocation provenance, so `.add(16)`
/// stays in-bounds of that allocation.
///
/// # Safety
/// `header` must point at a live block whose backing chunk outlives the access; the payload
/// is valid for the block's `size` bytes.
#[inline]
pub(crate) unsafe fn payload_ptr_of(header: NonNull<GcBlockHeader>) -> *mut u8 {
    // SAFETY: the payload occupies `[DATA_OFFSET, DATA_OFFSET + size)` in the same allocation
    // as the header; deriving from the raw header pointer keeps whole-allocation provenance.
    unsafe { header.as_ptr().cast::<u8>().add(GcBlockHeader::DATA_OFFSET) }
}

/// Injected payload finalizer: given a block's [`BlockType`], a pointer to its inline payload,
/// and the payload byte length, run the payload's destructor (e.g. `drop_in_place` a
/// `ClosureData`'s `String`, or each `Value` in an array-of-values block).
///
/// Injected (rather than matched inside `VarRegion`) so the allocator stays a **pure byte
/// allocator** with no dependency on the payload types (`metadata::types`). The heap supplies
/// one glue fn that dispatches by `BlockType`. `None` = every payload is POD (PR-1 default).
/// The `size` lets element-array glue compute the element count (`size / size_of::<Value>()`).
///
/// # Safety
/// The glue is called exactly once per block reclaim, with a valid pointer to that block's
/// initialized `size`-byte payload; it must not touch the `VarRegion` (called while borrowed).
pub type PayloadDropGlue = unsafe fn(BlockType, *mut u8, usize);
