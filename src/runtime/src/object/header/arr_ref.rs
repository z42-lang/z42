//! [`ArrRef`] — the handle of an array (`Value::Array`), object model R9.
//!
//! An array is one block of the heap's variable-length region (one 16 B header — GC word +
//! descriptor word — then the elements, `object::ArrayObj`), so the handle is a
//! [`VarGcRef`] — the 8-byte tagged pointer (low 48 bits block-header address, high 16 bits the
//! generation snapshot) — typed as "this block is an array". Same bits for everything that stores
//! a handle: an array reference word (`ref_word` kinds 2 / 7) and the JIT's `Value` payload are
//! the handle's raw bits.
//!
//! The method set mirrors [`ObjRef`](crate::object::ObjRef). Weak references record the chunk's
//! reuse epoch and are upgraded by the heap ([`WeakArrRef`]), exactly as for objects.

use crate::object::header::marks::MarkKind;
use crate::object::header::VarGcRef;
use crate::object::ArrayObj;

/// Handle of an array — see the module docs. `Copy`, 8 bytes, `Option<ArrRef>` too.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ArrRef(VarGcRef);

const _: () = assert!(std::mem::size_of::<ArrRef>() == 8);
const _: () = assert!(std::mem::size_of::<Option<ArrRef>>() == 8);

impl ArrRef {
    /// Type a block handle as an array. The caller knows the block is an array block whose
    /// descriptor word has been written.
    #[inline(always)]
    pub(crate) fn from_var(v: VarGcRef) -> Self {
        Self(v)
    }

    /// The underlying block handle.
    #[inline(always)]
    pub fn var_ref(&self) -> VarGcRef {
        self.0
    }

    /// The array — header and elements, as one fat reference built from the block's own pointer
    /// (whole-block provenance) with the payload length read from the block header. **Panics**
    /// when the handle is stale (the block was tombstoned, maybe reused, since the handle was
    /// taken) — the backstop against reading another block's memory as this array. One relaxed
    /// load, like `ObjRef::borrow`.
    #[inline(always)]
    pub fn borrow(&self) -> &ArrayObj {
        if !self.0.generation_matches() {
            stale_borrow();
        }
        // SAFETY: a live array block holds a written descriptor and `payload_size` bytes of
        // payload; the pointer is the block header pointer (whole-block provenance); the block
        // stays mapped while the handle is reachable (the GC keeps it alive).
        unsafe { &*ArrayObj::from_block(self.0.header_raw(), self.0.payload_size()) }
    }

    /// Identity: same block **and** same generation.
    #[inline]
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.0.ptr_eq(&b.0)
    }

    /// The block header's address — the array's identity (hash keys, dedup, retention). Stable for
    /// the array's lifetime (blocks never move); thin, derived from the handle.
    #[inline]
    pub fn as_ptr(this: &Self) -> *const u8 {
        this.0.header_raw().as_ptr()
    }

    /// The handle's raw bits, for storing in a reference word (`ref_word`). Never zero, low 3
    /// bits clear.
    #[inline(always)]
    pub fn to_tagged_bits(&self) -> u64 {
        self.0.to_bits()
    }

    /// Rebuild a handle from [`Self::to_tagged_bits`]; `0` → `None`.
    ///
    /// # Safety
    /// `bits` is `0` or came from `to_tagged_bits` on an array handle whose block still outlives
    /// the result.
    #[inline(always)]
    pub unsafe fn from_tagged_bits(bits: u64) -> Option<Self> {
        // SAFETY: forwarded; the caller upholds the lifetime contract.
        unsafe { VarGcRef::from_bits(bits) }.map(Self)
    }

    /// Mark the array for `kind`. Returns `true` iff this call made the transition.
    #[inline]
    pub fn mark(this: &Self, kind: MarkKind) -> bool {
        this.0.mark(kind)
    }

    /// Whether the array carries `kind`'s mark.
    #[inline]
    pub fn is_marked(this: &Self, kind: MarkKind) -> bool {
        this.0.is_marked(kind)
    }

    /// Generation age (`< promotion age` = young). Lock-free; the write barrier reads it.
    #[inline]
    pub fn gen_age(this: &Self) -> u8 {
        this.0.gen_age()
    }
}

/// The failure arm of [`ArrRef::borrow`], kept out of line.
#[cold]
#[inline(never)]
fn stale_borrow() -> ! {
    panic!("ArrRef::borrow: generation mismatch — use-after-finalize")
}

impl std::fmt::Debug for ArrRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.0.is_live() {
            return f.debug_tuple("ArrRef").field(&"<tombstoned>").finish();
        }
        f.debug_tuple("ArrRef").field(&self.borrow()).finish()
    }
}

/// A weak array reference: the handle plus its chunk's reuse epoch (see `WeakObjRef` — same
/// rule, same upgrade through the heap).
#[derive(Clone, Copy)]
pub struct WeakArrRef {
    pub(crate) arr: ArrRef,
    /// Owning chunk index (`u32::MAX` for a block outside any region — never reclaimed).
    pub(crate) chunk: u32,
    pub(crate) epoch: u32,
}

impl std::fmt::Debug for WeakArrRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeakArrRef").field("chunk", &self.chunk).field("epoch", &self.epoch).finish()
    }
}
