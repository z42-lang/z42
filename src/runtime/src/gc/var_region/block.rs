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
    /// Reconstruct from the tag bits of a header's `type_tag` byte. Returns `None` on an
    /// unknown tag (corruption guard — a valid block always carries one of the variants
    /// above). Callers pass the already-masked low [`TAG_BITS`]; values 5..=7 are unused
    /// and reaching one means the byte is corrupt.
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

/// Width of the [`BlockType`] field inside a header's `type_tag` byte. 5 variants → 3 bits.
const TAG_BITS: u32 = 3;
/// Mask selecting the [`BlockType`] bits of a `type_tag` byte.
const TAG_MASK: u8 = (1 << TAG_BITS) - 1;
/// Bit offset of the packed `gen_age` inside a `type_tag` byte.
const AGE_SHIFT: u32 = TAG_BITS;
/// Mask (post-shift) selecting the `gen_age` bits. **Two bits — ages above 3 do not fit.**
const AGE_MASK: u8 = 0b11;

/// Bit marking "this block is currently listed in `VarRegion::young_list`".
///
/// The young list uses **lazy deletion** — a tombstoned block stays in it until the next
/// minor sweep filters it out. Without this flag, a slot that dies and is then handed back
/// out by the free list before that sweep would be pushed a second time, so one block would
/// sit in the list twice: aged twice per minor, and the list would grow without bound.
/// `alloc` consults the flag and only pushes when it is clear.
const IN_YOUNG_BIT: u8 = 1 << 5;

/// Largest `gen_age` a block header can represent (see [`AGE_MASK`]).
///
/// `region::PROMOTION_THRESHOLD` is 2, so ages only ever reach 2 today. If a future
/// `PROMOTION_AGE` knob wants to exceed this, the age needs more room than `type_tag`'s
/// spare bits provide — `size_class` has one spare bit (its largest index is 64), or the
/// header has to grow, which costs far more than it sounds (see the note on `DATA_OFFSET`
/// in `chunk::class_for`).
pub const MAX_GEN_AGE: u8 = AGE_MASK;

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
    /// size; the slot's physical capacity is `size_class`'s footprint, which may be larger.
    pub(super) size: u32,
    /// Mark bit (0 = unmarked). CAS 0→1 by the mark phase; reset by sweep on survivors.
    pub(super) marked: AtomicU8,
    /// Tombstone flag: `true` while live, `false` after sweep reclaims the slot.
    pub(super) alive: AtomicBool,
    /// Packed payload kind + generation age + young-list membership. Low [`TAG_BITS`] hold
    /// the [`BlockType`] (tells the tracer how to scan the payload); the next two bits hold
    /// `gen_age`; bit 5 is [`IN_YOUNG_BIT`]. Bits 6-7 are unused (always 0).
    ///
    /// **fix-minor-gc-skips-var-region (2026-09-08)**: the age had to live *inside* an
    /// existing byte — the header is pinned at 16 by the assert below, and growing it to 24
    /// would push the 1.8 M blocks whose total is exactly `MIN_BLOCK` into the next size
    /// class, giving back 15 MB+ of what PR #526 saved.
    ///
    /// `AtomicU8` (same size and align as `u8`, so the layout is unchanged) because the
    /// write barrier reads `gen_age` lock-free on the mutator hot path while promotion
    /// writes it during STW sweep — a plain `u8` there is a data race. Mirrors
    /// `RegionEntry::gen_age`, which is atomic for the same reason.
    pub(super) type_tag: AtomicU8,
    /// Size-class index (`octave << SUB_LOG2 | sub` of the total footprint — see
    /// `chunk::class_for`), or [`OVERSIZED_CLASS`] for a dedicated chunk. Lets tombstone
    /// return the slot to the right free list and lets iteration know the slot's footprint.
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

    /// Pack a [`BlockType`] and a `gen_age` into the `type_tag` byte. The single place the
    /// layout is encoded — every header construction site goes through it.
    #[inline]
    pub(super) fn pack_tag(block_type: BlockType, gen_age: u8, in_young: bool) -> u8 {
        debug_assert!(gen_age <= MAX_GEN_AGE, "gen_age {gen_age} exceeds the 2 bits available");
        (block_type as u8)
            | ((gen_age & AGE_MASK) << AGE_SHIFT)
            | if in_young { IN_YOUNG_BIT } else { 0 }
    }

    /// Payload kind.
    #[inline]
    pub fn block_type(&self) -> BlockType {
        let tag = self.type_tag.load(Ordering::Relaxed) & TAG_MASK;
        // A live block always carries a valid tag (set at alloc); fall back to `Str` only to
        // avoid a panic on a corrupted read (debug builds assert instead).
        debug_assert!(BlockType::from_u8(tag).is_some(), "corrupt block type_tag");
        BlockType::from_u8(tag).unwrap_or(BlockType::Str)
    }

    /// Generation age: `< PROMOTION_THRESHOLD` means young, `>=` means promoted to old.
    /// Read `Relaxed` from the write barrier's cross-generation check — promotion writes
    /// happen under STW, so there is nothing to synchronize with.
    #[inline]
    pub fn gen_age(&self) -> u8 {
        (self.type_tag.load(Ordering::Relaxed) >> AGE_SHIFT) & AGE_MASK
    }

    /// Increment `gen_age`, saturating at [`MAX_GEN_AGE`]. Returns the new age. Called by
    /// the minor sweep on survivors — **STW only**, so the read-modify-write needs no CAS
    /// (nothing else writes this byte after alloc).
    #[inline]
    pub(super) fn bump_gen_age(&self) -> u8 {
        let cur = self.type_tag.load(Ordering::Relaxed);
        let age = ((cur >> AGE_SHIFT) & AGE_MASK).saturating_add(1).min(MAX_GEN_AGE);
        let keep = cur & (TAG_MASK | IN_YOUNG_BIT);
        self.type_tag.store(keep | (age << AGE_SHIFT), Ordering::Relaxed);
        age
    }

    /// Whether this block is currently listed in `VarRegion::young_list`. See
    /// [`IN_YOUNG_BIT`] for why membership is tracked on the header rather than by
    /// searching the list.
    #[inline]
    pub(super) fn is_in_young(&self) -> bool {
        self.type_tag.load(Ordering::Relaxed) & IN_YOUNG_BIT != 0
    }

    /// Set / clear the young-list membership bit. **STW only** (alloc holds the region lock;
    /// the minor sweep runs stopped-the-world), so no CAS is needed.
    #[inline]
    pub(super) fn set_in_young(&self, yes: bool) {
        let cur = self.type_tag.load(Ordering::Relaxed);
        let next = if yes { cur | IN_YOUNG_BIT } else { cur & !IN_YOUNG_BIT };
        self.type_tag.store(next, Ordering::Relaxed);
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
