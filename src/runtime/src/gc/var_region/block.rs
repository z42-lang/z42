//! Block layout: the payload-kind tag, the 16-byte block header, and the payload-pointer /
//! finalizer plumbing shared by the allocator (`super`), the chunk layer (`super::chunk`) and
//! the handle (`super::var_ref`). See the module docs of `gc::var_region` for the block model.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicPtr, AtomicU16, AtomicU32, AtomicU8, Ordering};

/// Payload kind of a variable-length GC block. Since a `VarRegion` mixes payload types in
/// one allocator, the block header records which kind it holds so the GC tracer knows how to
/// scan the payload. Every array is one block (object model R9: the header's kind word holds the
/// array's descriptor, the elements follow); its block type is its mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockType {
    /// UTF-8 string bytes (immutable leaf — no outgoing references).
    Str = 0,
    /// A FROZEN array: `[Value; n]`, written once before publication (closure environments, the
    /// reference-word kind-7 value box) — each element a traced edge.
    ArrayValue = 1,
    /// A PRIM array: packed primitive elements (leaf — no references).
    ArrayPrim = 2,
    /// A STRUCT array: the element layout word, then value-struct blobs whose reference leaves are
    /// reference words. Owns its `Arc<StructTypeLayout>`, so the heap's drop glue runs for it.
    ArrayStruct = 3,
    /// `ClosureData` fields (env edge + target `FnId`).
    Closure = 4,
    /// A REF array: 8 B self-describing reference words (`ref_word`). Object model R3.
    ArrayRef = 5,
    /// A MIXED array: 16 B type-parameter cells (`tparam_cell`). Object model R3.
    ArrayCell = 6,
    /// A managed object (`Value::Object` / `Value::BoxedStruct`, `metadata::types::ScriptObject`):
    /// the header's kind word holds the type descriptor, the payload is the field cells (plus a
    /// trailing meta word for generic / native-backed instances, [`OBJ_META_BIT`]). Object model
    /// R6 / R8. Owns its `Arc<TypeDesc>` and extras box, so the heap's drop glue runs for it.
    Object = 7,
}

impl BlockType {
    /// An object or an array — the blocks that count as allocation units of the generational
    /// policy (the promotion policy's survival sample, the promoted-byte budget).
    #[inline]
    pub fn is_object_or_array(self) -> bool {
        !matches!(self, Self::Str | Self::Closure)
    }

    /// An owner the remembered set can hold: an object or an array that can hold references
    /// (every array mode but PRIM). Strings are leaves, and a closure's only edge is its
    /// environment array, which is never younger than the closure.
    #[inline]
    pub fn may_be_remembered(self) -> bool {
        !matches!(self, Self::Str | Self::Closure | Self::ArrayPrim)
    }

    /// Reconstruct from the tag bits of a header's `type_tag` byte. Returns `None` on an
    /// unknown tag (corruption guard — a valid block always carries one of the variants
    /// above). Callers pass the already-masked low [`TAG_BITS`]; all eight values are in use.
    #[inline]
    pub(super) fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Str),
            1 => Some(Self::ArrayValue),
            2 => Some(Self::ArrayPrim),
            3 => Some(Self::ArrayStruct),
            4 => Some(Self::Closure),
            5 => Some(Self::ArrayRef),
            6 => Some(Self::ArrayCell),
            7 => Some(Self::Object),
            _ => None,
        }
    }
}

/// Width of the [`BlockType`] field inside a header's `type_tag` byte. 8 variants → 3 bits (full).
const TAG_BITS: u32 = 3;
/// Mask selecting the [`BlockType`] bits of a `type_tag` byte.
const TAG_MASK: u8 = (1 << TAG_BITS) - 1;
/// Bit offset of the packed `gen_age` inside a `type_tag` byte.
const AGE_SHIFT: u32 = TAG_BITS;
/// Mask (post-shift) selecting the `gen_age` bits. **Two bits — ages above 3 do not fit.**
const AGE_MASK: u8 = 0b11;

/// `BlockType::Str` only: the string's bytes are all ASCII (so char index == byte index
/// and the char count is the byte length). Set once on a freshly written string block
/// (`set_ascii_str`) before the handle escapes; clear means *unknown*, never "not ASCII",
/// so a path that does not set it only loses the fast path. The STW age updates below
/// preserve it.
pub(crate) const ASCII_STR_BIT: u8 = 1 << 6;

/// Objects and arrays ([`BlockType::may_be_remembered`]): the block is in the heap's
/// **remembered set** — it is old and a write barrier (or promotion) saw it receive a young
/// reference, so the next minor re-roots from it (object model R6 / R9; the region has no card
/// table). Set by an atomic or on the mutator path (`try_set_remembered`), cleared by the
/// collector when the owner no longer reaches anything young.
pub(crate) const REMEMBERED_BIT: u8 = 1 << 5;

/// The heap keeps side-table state keyed by this block's address (a finalizer, a soft
/// reference). Tombstoning a block with this bit — or [`REMEMBERED_BIT`] — records its address
/// in the region's flagged-dead list, so the heap can drop those entries before the chunk is
/// pooled or freed (`VarRegion::take_flagged_dead`). Sweep never has to look anything up for an
/// unflagged block.
pub(crate) const SIDE_BIT: u8 = 1 << 7;

/// The `type_tag` bits a GC age update must preserve (everything but the age).
const KEEP_MASK: u8 = TAG_MASK | ASCII_STR_BIT | REMEMBERED_BIT | SIDE_BIT;

/// Largest `gen_age` a block header can represent (see [`AGE_MASK`]).
///
/// **retune-gc-nursery-and-promotion-age (2026-09-11)**: `gc::PROMOTION_THRESHOLD` is
/// now **3**, i.e. exactly this ceiling — the spare bits are full. `Z42_GC_PROMOTION_AGE`
/// can still be lowered but no longer raised; a higher age needs a bit the 16-byte header no
/// longer has (object model R8 spent the last spare bytes on the object's type word).
pub const MAX_GEN_AGE: u8 = AGE_MASK;

/// `BlockType::Object` only: the payload ends in the object's 8-byte meta word (generic
/// type-arguments / native backing, `metadata::types::ScriptObject`). Same bit as
/// [`ASCII_STR_BIT`] — the two never meet on one block (arrays use neither). Set before the block is published,
/// never changed; age updates preserve it.
pub(crate) const OBJ_META_BIT: u8 = ASCII_STR_BIT;

/// The live occupant of a block always has an **odd** generation; tombstoning moves it to the
/// next (even) value and reusing the slot to the next odd one. So "alive" is one bit of the
/// generation, and the stale-handle guard (`generation == snapshot`) rejects dead blocks and
/// reused slots in one compare (object model R8 — the header has no separate alive byte).
pub(crate) const ALIVE_GEN_BIT: u16 = 1;

/// Generation of a block carved fresh (a new chunk, a detached block).
pub(crate) const FIRST_GEN: u16 = ALIVE_GEN_BIT;

/// The first 8 bytes of every block header — what the GC owns: generation (alive bit),
/// mark, packed type tag, payload length. `#[repr(C)]` and shared by name with the object
/// header (`metadata::types::ObjHeader` / `ScriptObject` embed it), so the two views of an
/// object block cannot drift apart.
///
/// All four fields are atomics: mark / tag bits are set by concurrent markers and barriers,
/// `generation` is the release/acquire publication point of a block filled into a reused slot
/// (M12), and `size` is rewritten by that fill while a heap walk on another thread may read it to
/// step over the (dead) slot — both values name the same size class, so the walk sees the same
/// footprint either way.
#[repr(C)]
pub struct GcWord {
    /// Generation counter (ABA guard) whose low bit is the alive flag ([`ALIVE_GEN_BIT`]):
    /// odd while a block lives, bumped to even by its tombstone, bumped to odd again when the
    /// slot is reused. A [`VarGcRef`] snapshots it (16 bits — the whole field) at construction; a
    /// mismatch on resolve means the block died or the slot was reused.
    pub(super) generation: AtomicU16,
    /// Mark bits (0 = unmarked): minor bit + major epoch (`gc::refs::MarkKind`).
    pub(super) marked: AtomicU8,
    /// Packed payload kind + generation age + flags. Low [`TAG_BITS`] hold the [`BlockType`]
    /// (tells the tracer how to scan the payload); the next two bits hold `gen_age`; bit 5 is
    /// [`REMEMBERED_BIT`] (objects), bit 6 [`ASCII_STR_BIT`] (strings) / [`OBJ_META_BIT`]
    /// (objects), bit 7 [`SIDE_BIT`]. Young-set membership lives in the region's bitmap.
    ///
    /// `AtomicU8` because the write barrier reads `gen_age` lock-free on the mutator hot path
    /// while promotion writes it during STW sweep.
    pub(super) type_tag: AtomicU8,
    /// Payload byte length — the bytes after the 16-byte header (an object's field cells plus
    /// its meta word, a string's UTF-8 bytes, …). The block's size class and footprint are a
    /// function of it (`chunk::payload_class`), so it is also what a chunk walk steps by.
    pub(super) size: AtomicU32,
}

/// Fixed header preceding a variable-length block's inline payload: the GC word and one
/// per-kind word. `#[repr(C, align(8))]` pins the field order and pads to 16 bytes so the
/// payload always starts 8-aligned at `DATA_OFFSET` (a `Value` element needs 8-alignment).
///
/// **Object model R8**: an object block's whole header is these 16 bytes — the kind word holds
/// the object's `Arc<TypeDesc>` (`metadata::types::ObjHeader` is the object view of the same
/// bytes) and the field cells start right after. The block's chunk is not recorded: a sweep
/// knows which chunk it walks, and the few callers that start from a handle ask the region
/// (`VarRegion::chunk_of`). The size class is derived from `size`.
#[repr(C, align(8))]
pub struct GcBlockHeader {
    pub(super) word: GcWord,
    /// Per-kind word, owned by the payload layer: an object's type descriptor (an
    /// `Arc<TypeDesc>`'s pointer, released by the drop glue, which leaves it null again), an
    /// array's interned descriptor (`metadata::types::ArrayDesc`, object model R9). Written before
    /// the block is published; null in strings and closures. The region writes it null when it
    /// carves a block and never reads it.
    pub(super) kind_word: AtomicPtr<()>,
}

impl std::ops::Deref for GcBlockHeader {
    type Target = GcWord;
    #[inline(always)]
    fn deref(&self) -> &GcWord {
        &self.word
    }
}

// The header is exactly 16 bytes on every target (on 32-bit ones the kind word is 4 bytes and
// `align(8)` pads the rest), so the inline payload begins 8-aligned.
const _: () = assert!(std::mem::size_of::<GcWord>() == 8);
const _: () = assert!(std::mem::size_of::<GcBlockHeader>() == 16);
const _: () = assert!(std::mem::align_of::<GcBlockHeader>() == 8);

/// Byte offset of [`GcWord::size`] (the payload byte length, `u32`) — read by JIT-emitted code
/// that inspects a string block without a helper call (`metadata::vstr::jit_layout`).
#[cfg(feature = "jit")]
pub(crate) const HEADER_SIZE_OFFSET: usize = std::mem::offset_of!(GcWord, size);
/// Byte offset of [`GcWord::type_tag`] (block type, age, remembered / ASCII bits) — see
/// [`HEADER_SIZE_OFFSET`].
#[cfg(feature = "jit")]
pub(crate) const HEADER_TYPE_TAG_OFFSET: usize = std::mem::offset_of!(GcWord, type_tag);
/// Byte offset of [`GcBlockHeader::kind_word`] (an object's type descriptor, an array's
/// descriptor).
pub(crate) const KIND_WORD_OFFSET: usize = std::mem::offset_of!(GcBlockHeader, kind_word);

impl GcBlockHeader {
    /// Byte offset of the inline payload within the allocation = the (padded) header size.
    pub const DATA_OFFSET: usize = std::mem::size_of::<GcBlockHeader>();

    /// A fresh header: generation `generation` (odd = alive, even = not yet published), no mark,
    /// age 0, no flags, null kind word. The single place a header is built.
    #[inline]
    pub(super) fn fresh(generation: u16, payload: usize, block_type: BlockType) -> Self {
        debug_assert!(payload <= u32::MAX as usize, "block payload {payload} exceeds u32");
        GcBlockHeader {
            word: GcWord {
                generation: AtomicU16::new(generation),
                marked: AtomicU8::new(0),
                type_tag: AtomicU8::new(GcWord::pack_tag(block_type, 0)),
                size: AtomicU32::new(payload as u32),
            },
            kind_word: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// The kind word (objects: the type descriptor pointer; null otherwise).
    #[inline]
    pub(crate) fn kind_word(&self) -> *mut () {
        self.kind_word.load(Ordering::Relaxed)
    }

    /// Null the kind word — an object's drop glue, after releasing the type descriptor it held.
    #[inline]
    pub(crate) fn clear_kind_word(&self) {
        self.kind_word.store(std::ptr::null_mut(), Ordering::Relaxed);
    }
}

impl GcWord {
    /// Payload byte length.
    #[inline]
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed) as usize
    }

    /// Pack a [`BlockType`] and a `gen_age` into the `type_tag` byte. The single place the
    /// layout is encoded — every header construction site goes through it.
    #[inline]
    pub(super) fn pack_tag(block_type: BlockType, gen_age: u8) -> u8 {
        debug_assert!(gen_age <= MAX_GEN_AGE, "gen_age {gen_age} exceeds the 2 bits available");
        (block_type as u8) | ((gen_age & AGE_MASK) << AGE_SHIFT)
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
        let keep = cur & KEEP_MASK;
        self.type_tag.store(keep | (age << AGE_SHIFT), Ordering::Relaxed);
        age
    }

    /// **fix-primitives-count-as-young (2026-09-11)**: raise `gen_age` to at least `age`
    /// (never lowers it). **STW only**, same as [`Self::bump_gen_age`].
    ///
    /// Used by tenure (every young block straight to the promotion age) and tests.
    #[inline]
    pub fn raise_gen_age_to(&self, age: u8) {
        let cur = self.type_tag.load(Ordering::Relaxed);
        let now = (cur >> AGE_SHIFT) & AGE_MASK;
        let want = age.min(MAX_GEN_AGE);
        if now >= want {
            return;
        }
        let keep = cur & KEEP_MASK;
        self.type_tag.store(keep | (want << AGE_SHIFT), Ordering::Relaxed);
    }

    /// Mark a freshly written string block as all-ASCII (see [`ASCII_STR_BIT`]). An atomic
    /// OR, so it cannot clobber a concurrent header update; losing the bit to one is
    /// harmless (the reader falls back to the slow path).
    #[inline]
    pub(crate) fn set_ascii_str(&self) {
        self.type_tag.fetch_or(ASCII_STR_BIT, Ordering::Relaxed);
    }

    /// Whether [`Self::set_ascii_str`] marked this block.
    #[inline]
    pub(crate) fn is_ascii_str(&self) -> bool {
        self.type_tag.load(Ordering::Relaxed) & ASCII_STR_BIT != 0
    }

    /// Record that this (unpublished) object block's payload ends in a meta word
    /// ([`OBJ_META_BIT`]).
    #[inline]
    pub(crate) fn set_obj_meta(&self) {
        self.type_tag.fetch_or(OBJ_META_BIT, Ordering::Relaxed);
    }

    /// Whether this object block's payload ends in a meta word.
    #[inline(always)]
    pub(crate) fn has_obj_meta(&self) -> bool {
        self.type_tag.load(Ordering::Relaxed) & OBJ_META_BIT != 0
    }

    /// Put the block into the remembered set's bit (see [`REMEMBERED_BIT`]). Returns `true` iff
    /// this call set it — the caller that wins pushes the object onto the heap's remembered
    /// list, so the list holds each object once. An atomic OR: the mutator path races only
    /// other mutators here (age updates happen under STW).
    #[inline]
    pub(crate) fn try_set_remembered(&self) -> bool {
        self.type_tag.fetch_or(REMEMBERED_BIT, Ordering::Relaxed) & REMEMBERED_BIT == 0
    }

    /// Take the block out of the remembered set's bit (collector, STW).
    #[inline]
    pub(crate) fn clear_remembered(&self) {
        self.type_tag.fetch_and(!REMEMBERED_BIT, Ordering::Relaxed);
    }

    /// Whether [`REMEMBERED_BIT`] is set.
    #[inline]
    pub(crate) fn is_remembered(&self) -> bool {
        self.type_tag.load(Ordering::Relaxed) & REMEMBERED_BIT != 0
    }

    /// Flag that the heap keeps side-table state for this block (see [`SIDE_BIT`]).
    #[inline]
    pub(crate) fn set_side(&self) {
        self.type_tag.fetch_or(SIDE_BIT, Ordering::Relaxed);
    }

    /// Whether tombstoning this block must be reported to the heap (side tables or the
    /// remembered set name it).
    #[inline]
    pub(super) fn needs_dead_notice(&self) -> bool {
        self.type_tag.load(Ordering::Relaxed) & (SIDE_BIT | REMEMBERED_BIT) != 0
    }

    /// True while the block is live (published and not yet swept): its generation is odd. The
    /// acquire load pairs with the release store that publishes a block filled into a reused
    /// slot (`VarGcRef::publish`).
    #[inline]
    pub fn is_alive(&self) -> bool {
        self.generation.load(Ordering::Acquire) & ALIVE_GEN_BIT != 0
    }

    /// Mark this block for `kind` (see [`crate::gc::refs::MarkKind`]). Returns `true` iff this
    /// call made the transition.
    #[inline]
    pub fn mark(&self, kind: crate::gc::refs::MarkKind) -> bool {
        crate::gc::refs::mark_cell(&self.marked, kind)
    }

    /// Whether this block carries `kind`'s mark.
    #[inline]
    pub fn is_marked(&self, kind: crate::gc::refs::MarkKind) -> bool {
        crate::gc::refs::is_marked_cell(&self.marked, kind)
    }

    /// Clear the minor mark only (the minor sweep on a survivor).
    #[inline]
    pub fn clear_minor_mark(&self) {
        crate::gc::refs::clear_minor_cell(&self.marked);
    }

    /// The stored major epoch (0 = never major-marked). Invariant checks only.
    #[inline]
    pub fn major_epoch(&self) -> u8 {
        crate::gc::refs::major_epoch_of_cell(&self.marked)
    }

    /// Current generation (odd = alive).
    #[inline]
    pub(super) fn generation(&self) -> u16 {
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

/// Injected payload finalizer: given a block's [`BlockType`], its **header** pointer
/// (whole-allocation provenance) and its payload byte length, run the payload's destructor (an
/// object's type descriptor and extras box, or each `Value` in an array-of-values block). Returns
/// the bytes the destructor gave back to the allocator **outside** the block (an object's extras
/// box), which the region credits to the heap footprint.
///
/// Injected (rather than matched inside `VarRegion`) so the allocator stays a **pure byte
/// allocator** with no dependency on the payload types (`metadata::types`). The heap supplies
/// one glue fn that dispatches by `BlockType`. `None` = every payload is POD (PR-1 default).
/// The `size` lets element-array glue compute the element count (`size / size_of::<Value>()`).
///
/// # Safety
/// The glue is called exactly once per block reclaim, with a valid pointer to that block's
/// header and its initialized `size`-byte payload; it must not touch the `VarRegion` (called
/// while borrowed). It may write the header's kind word (an object's glue nulls it).
pub type PayloadDropGlue = unsafe fn(BlockType, NonNull<GcBlockHeader>, usize) -> u64;

/// Bit of `bt` in a drop-type mask (`VarRegion::drop_types`): which block types the drop glue
/// runs for.
#[inline]
pub const fn drop_type_bit(bt: BlockType) -> u8 {
    1 << (bt as u8)
}
