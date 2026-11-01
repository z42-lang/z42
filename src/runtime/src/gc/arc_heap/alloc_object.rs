//! Object allocation (object model R6 / R8): an object is one `BlockType::Object` block of the
//! variable-length region — one 16 B header (GC word + type word), field cells, maybe a meta word
//! — so it rides the same TLAB / locked paths as strings and closures (`acquire_var_block`).
//!
//! Construction happens **in the block, before the object is published**: the type word is
//! written into the zeroed block, the instance's generic step runs on the unpublished object (`&mut`
//! is sound — the handle has not left this function), and only then is the allocation recorded
//! (which may run a collection) and the handle returned. A published object never changes its
//! header (object model R4). Nothing here allocates between taking the block and recording it,
//! so no collection can see a half-built object.

use std::ptr::NonNull;
use std::sync::Arc;

use crate::object::header::obj_ref::ObjRef;
use crate::gc::types::{AllocKind, GcEvent};
use crate::object::header::{BlockType, GcBlockHeader, KIND_WORD_OFFSET};
use crate::object::{ObjHeader, OwnedObject, ScriptObject, TypeArgs};
use crate::object::{TypeDesc, Value};

/// Bytes of the meta word a generic / native-backed object carries at the end of its payload.
const META_BYTES: usize = std::mem::size_of::<*const ()>();

impl crate::gc::arc_heap::ArcMagrGC {
    /// The allocation funnel for objects. `payload` is the field area plus, when `meta`, the meta
    /// word; `write` must install the type word (`ObjHeader::install`) into the fresh block at the
    /// header it is given and may then initialize the object through the (fat, whole-block
    /// provenance) object pointer — the payload is zeroed already. Returns `Value::Object`, or
    /// `Value::Null` when a strict-OOM heap refuses the allocation.
    pub(super) fn alloc_object_block(
        &self,
        payload: usize,
        meta: bool,
        write: impl FnOnce(NonNull<u8>, *mut ScriptObject),
    ) -> Value {
        // TLAB fast path when armed and not strict-OOM (D6), else the locked region path; both
        // shade the block when an incremental cycle is marking (allocate-black).
        let (vref, fast) = self.acquire_var_block(payload, BlockType::Object);
        if meta {
            vref.set_obj_meta();
        }
        // SAFETY: a fresh, zeroed block of `payload` bytes that nothing else can reach yet;
        // `write` installs the type word before the object is read.
        let obj: &ScriptObject = unsafe {
            let p = ScriptObject::from_block(vref.header_raw(), payload);
            write(vref.header_raw(), p);
            &*p
        };
        // Built: from here a heap walk may see it (M12 — a reused slot is born not alive).
        vref.publish();
        let extras = obj.extras_bytes();
        if extras != 0 {
            // The extras box lives outside the block; the drop glue credits it back.
            self.footprint.charge(extras);
        }
        let handle = ObjRef::from_var(vref);
        let value = Value::Object(handle);
        let size = GcBlockHeader::DATA_OFFSET + payload;
        let kind = || AllocKind::Object { class: handle.type_desc().name.clone() };
        if fast {
            self.record_alloc_fast(kind, size);
            return value;
        }
        // Phase 3-OOM: a strict heap refunds an allocation that would cross its limit.
        let (would_oom, limit) = self.would_oom_after_alloc(size as u64);
        if would_oom {
            // No finalizer / side entry can exist yet; tombstoning runs the drop glue (header).
            self.region_var.lock().tombstone(vref);
            self.fire_event(GcEvent::OutOfMemory { requested_bytes: size as u64, limit_bytes: limit });
            return Value::Null;
        }
        self.record_alloc(&value, kind, size);
        self.maybe_auto_collect();
        value
    }

    /// `ObjNew`'s heap branch: a fresh instance of `type_desc` (zeroed fields) plus the generic
    /// step ([`ScriptObject::install_type_args`]), built in its block. A generic instance gets the
    /// meta word its type-arguments live in.
    pub(super) fn alloc_instance_in_region(&self, type_desc: Arc<TypeDesc>, type_args: TypeArgs) -> Value {
        let meta = !type_args.is_empty();
        let payload = type_desc.object_bytes_len() + if meta { META_BYTES } else { 0 };
        self.alloc_object_block(payload, meta, |hdr, p| {
            // SAFETY: `hdr` / `p` are the unpublished block (see `alloc_object_block`).
            unsafe {
                ObjHeader::install(hdr, type_desc);
                (*p).install_type_args_unpublished(type_args);
            }
        })
    }

    /// A boxed primitive / boxed struct blob: the field area is exactly `bytes`.
    pub(super) fn alloc_boxed_bytes_in_region(&self, type_desc: Arc<TypeDesc>, bytes: &[u8]) -> Value {
        self.alloc_object_block(bytes.len(), false, |hdr, p| {
            // SAFETY: as above; the fields are written before the object is published.
            unsafe {
                ObjHeader::install(hdr, type_desc);
                (*p).cells().copy_in(0, bytes);
            }
        })
    }

    /// Move an object built outside the heap into a fresh block: the type word (with its
    /// `Arc<TypeDesc>`) and the payload (fields and meta word, with its extras) are copied
    /// bitwise, then the detached block's memory is freed without dropping what it no longer owns.
    pub(super) fn alloc_owned_in_region(&self, obj: OwnedObject) -> Value {
        let (payload, meta) = (obj.payload_len() + if obj.has_meta() { META_BYTES } else { 0 }, obj.has_meta());
        let block = obj.into_detached();
        debug_assert_eq!(block.payload_size(), payload);
        self.alloc_object_block(payload, meta, |hdr, _| {
            // SAFETY: `block` is exclusively owned and holds a constructed type word + `payload`
            // initialized bytes; the destination is a fresh block of the same payload size. The
            // copy moves their ownership; `block` is then released as plain memory.
            unsafe {
                let (src, dst) = (block.header_ptr().cast::<u8>().as_ptr(), hdr.as_ptr());
                std::ptr::copy_nonoverlapping(src.add(KIND_WORD_OFFSET), dst.add(KIND_WORD_OFFSET), std::mem::size_of::<usize>());
                std::ptr::copy_nonoverlapping(block.payload_ptr(), dst.add(GcBlockHeader::DATA_OFFSET), payload);
            }
        })
        // `block` drops here: memory only.
    }
}
