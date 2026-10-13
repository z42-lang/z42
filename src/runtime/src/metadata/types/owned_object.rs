//! `OwnedObject` — a managed object that lives **outside** the GC heap (object model R6).
//!
//! A heap object is one `BlockType::Object` block of the variable-length region: the 16 B block
//! header (GC word + type word), the field cells, maybe a meta word. An `OwnedObject` is the same
//! bytes in a detached block
//! (`gc::var_region::DetachedBlock`) the value owns, so `&ScriptObject` reads identically
//! wherever the object lives. Two kinds of objects need that:
//!
//! - **stack-allocated objects** (`interp::stack_alloc`): escape analysis proved them
//!   frame-local, so they never enter the heap;
//! - **objects under construction** that are built before the heap sees them
//!   (`MagrGC::alloc_script_object`: reflection's `Activator.CreateInstance`, tests). The heap
//!   copies the header and fields into a fresh block and frees this one. The hot path
//!   (`ObjNew`) does not go through here — it builds the object directly in its heap block.
//!
//! A fresh `OwnedObject` has zeroed fields (zero = every field's default, `null` included).
//! Dropping it drops the type word and the meta word's extras box and frees the block. Giving it
//! type-arguments or a native backing after construction grows the block by the meta word
//! ([`OwnedObject::set_type_args`] / [`OwnedObject::set_native`]).
//!
//! # Safety
//!
//! The type word is written once at construction and dropped once (in `Drop`, or handed over by
//! [`OwnedObject::into_detached`]); nothing else touches the block's memory except through
//! `&ScriptObject` / `&mut ScriptObject` (and the block move in `ensure_meta`).

use std::sync::Arc;

use super::{NativeData, ObjHeader, ScriptObject, TypeArgs, TypeDesc};
use crate::gc::var_region::{BlockType, DetachedBlock, KIND_WORD_OFFSET};

/// A managed object owned outside the GC heap — see the module docs.
pub struct OwnedObject {
    block: std::mem::ManuallyDrop<DetachedBlock>,
}

/// Bytes of the meta word a generic / native-backed object carries at the end of its payload.
const META_BYTES: usize = std::mem::size_of::<*const ()>();

impl OwnedObject {
    /// A fresh object of `type_desc` with `field_bytes` zeroed bytes of field cells, plus a
    /// (null) meta word when `meta`.
    fn alloc(type_desc: Arc<TypeDesc>, field_bytes: usize, meta: bool) -> Self {
        let payload = field_bytes + if meta { META_BYTES } else { 0 };
        assert!(payload <= u32::MAX as usize, "object payload too large: {field_bytes} bytes");
        let block = DetachedBlock::new(payload, BlockType::Object);
        // SAFETY: a fresh detached block nothing else can see; its kind word is null. The type
        // word is released exactly once (see `Drop`).
        unsafe {
            if meta {
                block.header_ptr().as_ref().set_obj_meta();
            }
            ObjHeader::install(block.header_ptr().cast(), type_desc);
        }
        Self { block: std::mem::ManuallyDrop::new(block) }
    }

    /// A fresh object of `type_desc` with `field_bytes` zeroed bytes of field cells.
    pub fn new(type_desc: Arc<TypeDesc>, field_bytes: usize) -> Self {
        Self::alloc(type_desc, field_bytes, false)
    }

    /// A fresh instance of `type_desc` sized by its layout (`TypeDesc::object_bytes_len`).
    pub fn of_type(type_desc: Arc<TypeDesc>) -> Self {
        let n = type_desc.object_bytes_len();
        Self::new(type_desc, n)
    }

    /// A fresh instance for `ObjNew`'s stack branch: zeroed fields plus the instance's generic
    /// step ([`ScriptObject::install_type_args`]). The heap branch builds the same object in place
    /// (`MagrGC::alloc_instance`); both must stay in lockstep, or flipping `Z42_STACKALLOC`
    /// changes observable values.
    pub fn new_instance(type_desc: Arc<TypeDesc>, type_args: TypeArgs) -> Self {
        let n = type_desc.object_bytes_len();
        let mut obj = Self::alloc(type_desc, n, !type_args.is_empty());
        obj.install_type_args(type_args);
        obj
    }

    /// An object whose field area is exactly `bytes` — the boxed-primitive / boxed-struct shape.
    pub fn from_bytes(type_desc: Arc<TypeDesc>, bytes: &[u8]) -> Self {
        let obj = Self::new(type_desc, bytes.len());
        // A fresh block nothing else can see yet; `copy_in` is the cell view's bulk store.
        obj.cells().copy_in(0, bytes);
        obj
    }

    /// A layout-sized instance with a native backing (built-in boxes).
    pub fn with_native(type_desc: Arc<TypeDesc>, native: NativeData) -> Self {
        let n = type_desc.object_bytes_len();
        let mut obj = Self::alloc(type_desc, n, !matches!(native, NativeData::None));
        obj.set_native(native);
        obj
    }

    /// [`ScriptObject::set_native`], growing the meta word first if the object has none.
    pub fn set_native(&mut self, native: NativeData) {
        if !matches!(native, NativeData::None) {
            self.ensure_meta();
        }
        (**self).set_native(native);
    }

    /// [`ScriptObject::set_type_args`], growing the meta word first if the object has none.
    pub fn set_type_args(&mut self, type_args: TypeArgs) {
        if !type_args.is_empty() {
            self.ensure_meta();
        }
        (**self).set_type_args(type_args);
    }

    /// [`ScriptObject::install_type_args`], growing the meta word first if the object has none.
    pub fn install_type_args(&mut self, type_args: TypeArgs) {
        if !type_args.is_empty() {
            self.ensure_meta();
        }
        (**self).install_type_args(type_args);
    }

    /// Give the object a (null) meta word if it has none: move the type word and the fields into
    /// a block one word longer. Only an object built without one and then given type-arguments or
    /// a native backing pays this (reflection's `Activator.CreateInstance`, tests).
    fn ensure_meta(&mut self) {
        if self.has_meta() {
            return;
        }
        let fields = self.payload_len();
        let fresh = DetachedBlock::new(fields + META_BYTES, BlockType::Object);
        // SAFETY: `fresh` is exclusively owned and zeroed; the old block holds a constructed type
        // word and `fields` initialized bytes. Copying the type word moves the `Arc`'s ownership,
        // and the old block is then released as plain memory (its header is not dropped).
        unsafe {
            fresh.header_ptr().as_ref().set_obj_meta();
            let (src, dst) = (self.block.header_ptr().cast::<u8>().as_ptr(), fresh.header_ptr().cast::<u8>().as_ptr());
            std::ptr::copy_nonoverlapping(src.add(KIND_WORD_OFFSET), dst.add(KIND_WORD_OFFSET), std::mem::size_of::<usize>());
            std::ptr::copy_nonoverlapping(self.block.payload_ptr(), fresh.payload_ptr(), fields);
        }
        drop(std::mem::replace(&mut *self.block, fresh));
    }

    /// Hand the block over **with** its header still constructed (the caller now owns the
    /// type word and meta word): the heap moves them and the fields into a GC block and then
    /// drops this block's memory only.
    pub(crate) fn into_detached(self) -> DetachedBlock {
        let mut this = std::mem::ManuallyDrop::new(self);
        // SAFETY: `this` is never used or dropped again; the block moves out exactly once.
        unsafe { std::mem::ManuallyDrop::take(&mut this.block) }
    }
}

impl OwnedObject {
    /// The object as a fat pointer over the detached block (header + payload), derived from the
    /// block's own allocation.
    #[inline]
    fn object_ptr(&self) -> *mut ScriptObject {
        // SAFETY: the block was carved by `class_for` for its payload (capacity covers the
        // rounded-up `size_of_val`); its header heads the allocation.
        unsafe { ScriptObject::from_block(self.block.header_ptr().cast(), self.block.payload_size()) }
    }
}

impl std::ops::Deref for OwnedObject {
    type Target = ScriptObject;
    #[inline]
    fn deref(&self) -> &ScriptObject {
        // SAFETY: the type word was written in `alloc` and lives until `Drop`.
        unsafe { &*self.object_ptr() }
    }
}

impl std::ops::DerefMut for OwnedObject {
    #[inline]
    fn deref_mut(&mut self) -> &mut ScriptObject {
        // SAFETY: as `deref`; `&mut self` makes the access exclusive.
        unsafe { &mut *self.object_ptr() }
    }
}

impl Drop for OwnedObject {
    fn drop(&mut self) {
        // SAFETY: the type word was written in `alloc` and is dropped exactly once, here (with the
        // meta word's extras box); the block's memory is released right after.
        unsafe {
            ScriptObject::destroy(self.object_ptr());
            std::mem::ManuallyDrop::drop(&mut self.block);
        }
    }
}

impl std::fmt::Debug for OwnedObject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

#[cfg(test)]
#[path = "owned_object_tests.rs"]
mod owned_object_tests;
