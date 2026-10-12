//! [`ObjRef`] — the handle of a managed object (`Value::Object` / `Value::BoxedStruct`), object
//! model R6.
//!
//! An object is one `BlockType::Object` block of the heap's variable-length region (16 B block
//! header, 16 B [`ScriptObject`] header, field cells), so the handle is a [`VarGcRef`] — the
//! 8-byte path-A tagged pointer (low 48 bits block-header address, high 16 bits a narrow
//! generation snapshot) — typed as "this block is an object". Same bits as before for
//! everything that stores a handle: an object reference word (`ref_word` kinds 1 / 6) and the
//! JIT's `Value` payload are the handle's raw bits.
//!
//! The method set mirrors `GcRef<T>` (`borrow` / `ptr_eq` / `as_ptr` / tagged bits / marks /
//! age), so code that handles an object did not have to learn a new vocabulary. Weak references
//! are the exception: a var-region address can be re-carved (a pooled chunk is re-bumped from
//! offset 0, a dedicated chunk is freed), so a weak handle also records its chunk's reuse epoch
//! and is upgraded by the heap ([`WeakObjRef`], `ArcMagrGC::upgrade_weak_object`).

use crate::gc::refs::MarkKind;
use crate::gc::var_region::VarGcRef;
use crate::metadata::types::{ObjHeader, OwnedObject, ScriptObject};

/// Handle of a managed object — see the module docs. `Copy`, 8 bytes, `Option<ObjRef>` too.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ObjRef(VarGcRef);

const _: () = assert!(std::mem::size_of::<ObjRef>() == 8);
const _: () = assert!(std::mem::size_of::<Option<ObjRef>>() == 8);

impl ObjRef {
    /// Type a block handle as an object. The caller knows the block is a `BlockType::Object`
    /// block whose header has been written.
    #[inline(always)]
    pub(crate) fn from_var(v: VarGcRef) -> Self {
        Self(v)
    }

    /// The underlying block handle.
    #[inline(always)]
    pub fn var_ref(&self) -> VarGcRef {
        self.0
    }

    /// The object — header and fields, as one fat reference built from the block's own pointer
    /// (whole-block provenance) with the field-area length read from the block header (see
    /// [`ScriptObject`]). **Panics** when the handle is stale —
    /// the block was tombstoned (and maybe reused) since the handle was taken: the generation
    /// guard is the backstop against reading another object's memory as this one
    /// (use-after-finalize via `GC.Finalize`, or a VM bug). Unconditional (release too), and one
    /// relaxed load from the block header — every tombstone bumps the generation, so the alive
    /// flag would add a second load and nothing else (`VarGcRef::generation_matches`). This sits
    /// on every field access and virtual call of the interpreter.
    #[inline(always)]
    pub fn borrow(&self) -> &ScriptObject {
        if !self.0.generation_matches() {
            stale_borrow();
        }
        // SAFETY: a live object block holds a written object header at its payload start and
        // `payload_size` bytes of header + fields; the pointer derives from the block header
        // pointer (whole-block provenance); the block stays mapped while the handle is reachable
        // (the GC keeps it alive).
        unsafe { &*ScriptObject::from_payload(self.0.payload_as_ptr::<u8>(), self.0.payload_size()) }
    }

    /// Just the object header (type descriptor, meta word) as a thin reference over its 16 bytes
    /// — what the type checks and virtual-call caches need, without reading the field-area
    /// length or building the fat `&ScriptObject`. Same stale-handle guard as [`Self::borrow`].
    #[inline(always)]
    pub fn header(&self) -> &ObjHeader {
        if !self.0.generation_matches() {
            stale_borrow();
        }
        // SAFETY: as `borrow`; the header is the payload's sized prefix.
        unsafe { &*self.0.payload_as_ptr::<ObjHeader>() }
    }

    /// Identity: same block **and** same generation.
    #[inline]
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        a.0.ptr_eq(&b.0)
    }

    /// The object header's address — the object's identity (hash keys, dedup, retention). Stable
    /// for the object's lifetime: blocks never move. A thin pointer derived from the block (not
    /// from a `&ScriptObject`), so it is safe to keep and compare without touching the object.
    #[inline]
    pub fn as_ptr(this: &Self) -> *const ObjHeader {
        // SAFETY: pointer arithmetic only (no dereference).
        unsafe { this.0.payload_as_ptr::<ObjHeader>() }
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
    /// `bits` is `0` or came from `to_tagged_bits` on an object handle whose block still
    /// outlives the result.
    #[inline(always)]
    pub unsafe fn from_tagged_bits(bits: u64) -> Option<Self> {
        // SAFETY: forwarded; the caller upholds the lifetime contract.
        unsafe { VarGcRef::from_bits(bits) }.map(Self)
    }

    /// Mark the object for `kind`. Returns `true` iff this call made the transition.
    #[inline]
    pub fn mark(this: &Self, kind: MarkKind) -> bool {
        this.0.mark(kind)
    }

    /// Whether the object carries `kind`'s mark.
    #[inline]
    pub fn is_marked(this: &Self, kind: MarkKind) -> bool {
        this.0.is_marked(kind)
    }

    /// Generation age (`< promotion age` = young). Lock-free; the write barrier reads it.
    #[inline]
    pub fn gen_age(this: &Self) -> u8 {
        this.0.gen_age()
    }

    /// Give `obj` a home outside any heap, forever — a block no region knows about, never
    /// marked or swept. **Tests and heap-less fallbacks only** (a leak by construction): every
    /// object a running VM creates goes through `ctx.heap()`.
    pub fn leak(obj: OwnedObject) -> Self {
        Self(obj.into_detached().leak())
    }
}

/// The failure arm of [`ObjRef::borrow`], kept out of line so the hot path is a load, a compare
/// and a never-taken branch.
#[cold]
#[inline(never)]
fn stale_borrow() -> ! {
    panic!("ObjRef::borrow: generation mismatch — use-after-finalize")
}

impl std::fmt::Debug for ObjRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.0.is_live() {
            return f.debug_tuple("ObjRef").field(&"<tombstoned>").finish();
        }
        f.debug_tuple("ObjRef").field(&self.borrow()).finish()
    }
}

/// A weak object reference: the handle plus the reuse epoch of the chunk the block was carved
/// from (`VarRegion::chunk_epoch`). While the epoch is unchanged, the address is still a block
/// header of that chunk, so the generation / alive guard decides; once the chunk has been pooled
/// (re-bumped from offset 0) or freed (a dedicated large-object chunk), the reference is dead
/// without the header being read at all. Upgraded by the heap (`MagrGC::upgrade_weak`), which
/// holds the region.
#[derive(Clone, Copy)]
pub struct WeakObjRef {
    pub(crate) obj: ObjRef,
    /// Owning chunk index (`u32::MAX` for a block outside any region — never reclaimed).
    pub(crate) chunk: u32,
    pub(crate) epoch: u32,
}

impl std::fmt::Debug for WeakObjRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeakObjRef").field("chunk", &self.chunk).field("epoch", &self.epoch).finish()
    }
}
