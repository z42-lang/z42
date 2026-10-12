//! `OwnedObject` — a managed object that lives **outside** the GC heap (object model R6).
//!
//! A heap object is one `BlockType::Object` block of the variable-length region: block header,
//! [`ScriptObject`] header, field cells. An `OwnedObject` is the same bytes in a detached block
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
//! Dropping it drops the header (`Arc<TypeDesc>`, extras box) and frees the block.
//!
//! # Safety
//!
//! The object header is written once at construction and dropped once (in `Drop`, or handed
//! over by [`OwnedObject::into_detached`]); nothing else touches the block's memory except
//! through `&ScriptObject` / `&mut ScriptObject`.

use std::sync::Arc;

use super::{NativeData, ObjHeader, ScriptObject, TypeArgs, TypeDesc, OBJ_HEADER_BYTES};
use crate::gc::var_region::{BlockType, DetachedBlock};

/// A managed object owned outside the GC heap — see the module docs.
pub struct OwnedObject {
    block: std::mem::ManuallyDrop<DetachedBlock>,
}

impl OwnedObject {
    /// A fresh object of `type_desc` with `field_bytes` zeroed bytes of field cells.
    pub fn new(type_desc: Arc<TypeDesc>, field_bytes: usize) -> Self {
        assert!(field_bytes <= u32::MAX as usize - OBJ_HEADER_BYTES, "object payload too large: {field_bytes} bytes");
        let block = DetachedBlock::new(OBJ_HEADER_BYTES + field_bytes, BlockType::Object);
        // SAFETY: the block's payload is `OBJ_HEADER_BYTES + field_bytes` zeroed bytes, 8-aligned;
        // the header goes at its start and is dropped exactly once (see `Drop`).
        unsafe { block.payload_ptr().cast::<ObjHeader>().write(ObjHeader::new(type_desc)) };
        Self { block: std::mem::ManuallyDrop::new(block) }
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
        let mut obj = Self::of_type(type_desc);
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
        let mut obj = Self::of_type(type_desc);
        obj.set_native(native);
        obj
    }

    /// Hand the block over **with** its header still constructed (the caller now owns the
    /// header's drop): the heap copies header + fields into a GC block and then drops this
    /// block's memory only.
    pub(crate) fn into_detached(self) -> DetachedBlock {
        let mut this = std::mem::ManuallyDrop::new(self);
        // SAFETY: `this` is never used or dropped again; the block moves out exactly once.
        unsafe { std::mem::ManuallyDrop::take(&mut this.block) }
    }
}

impl OwnedObject {
    /// The object as a fat pointer over the detached block's payload (header + fields), derived
    /// from the block's own allocation.
    #[inline]
    fn object_ptr(&self) -> *mut ScriptObject {
        // SAFETY: the payload is `payload_size` bytes of header + fields of this block, which was
        // carved by `class_for` (capacity covers the rounded-up `size_of_val`).
        unsafe { ScriptObject::from_payload(self.block.payload_ptr(), self.block.payload_size()) }
    }
}

impl std::ops::Deref for OwnedObject {
    type Target = ScriptObject;
    #[inline]
    fn deref(&self) -> &ScriptObject {
        // SAFETY: the header was written in `new` and lives until `Drop`.
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
        // SAFETY: the header was written in `new` and is dropped exactly once, here; the block's
        // memory is released right after.
        unsafe {
            std::ptr::drop_in_place(self.object_ptr());
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
