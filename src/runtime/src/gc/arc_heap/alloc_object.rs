//! Object allocation (object model R6): an object is one `BlockType::Object` block of the
//! variable-length region — block header, [`ScriptObject`] header, field cells — so it rides the
//! same TLAB / locked paths as strings and closures (`acquire_var_block`).
//!
//! Construction happens **in the block, before the object is published**: the header is written
//! into the zeroed block, the instance's generic step runs on the unpublished object (`&mut`
//! is sound — the handle has not left this function), and only then is the allocation recorded
//! (which may run a collection) and the handle returned. A published object never changes its
//! header (object model R4). Nothing here allocates between taking the block and recording it,
//! so no collection can see a half-built object.

use std::sync::Arc;

use crate::gc::obj_ref::ObjRef;
use crate::gc::types::{AllocKind, GcEvent};
use crate::gc::var_region::{BlockType, GcBlockHeader};
use crate::metadata::types::{ObjHeader, OwnedObject, ScriptObject, TypeArgs, OBJ_HEADER_BYTES};
use crate::metadata::{TypeDesc, Value};

impl crate::gc::arc_heap::ArcMagrGC {
    /// The allocation funnel for objects. `payload` is header + fields; `write` must initialize
    /// the header at the (fat, whole-block-provenance) object pointer it is given — through
    /// `ObjHeader` writes, never by value — the fields are zeroed already. Returns
    /// `Value::Object`, or `Value::Null` when a strict-OOM heap refuses the allocation.
    pub(super) fn alloc_object_block(
        &self,
        payload: usize,
        write: impl FnOnce(*mut ScriptObject),
    ) -> Value {
        // TLAB fast path when armed and not strict-OOM (D6), else the locked region path; both
        // shade the block when an incremental cycle is marking (allocate-black).
        let (vref, fast) = self.acquire_var_block(payload, BlockType::Object);
        // SAFETY: a fresh, zeroed block of `payload >= OBJ_HEADER_BYTES` bytes that nothing else
        // can reach yet; `write` puts the header at its payload start.
        let obj: &ScriptObject = unsafe {
            let p = ScriptObject::from_payload(vref.payload_as_ptr::<u8>(), payload);
            write(p);
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
    /// step ([`ScriptObject::install_type_args`]), built in its block.
    pub(super) fn alloc_instance_in_region(&self, type_desc: Arc<TypeDesc>, type_args: TypeArgs) -> Value {
        let payload = OBJ_HEADER_BYTES + type_desc.object_bytes_len();
        self.alloc_object_block(payload, |p| {
            // SAFETY: `p` is the unpublished block's header slot (see `alloc_object_block`).
            unsafe {
                p.cast::<ObjHeader>().write(ObjHeader::new(type_desc));
                (*p).install_type_args(type_args);
            }
        })
    }

    /// A boxed primitive / boxed struct blob: the field area is exactly `bytes`.
    pub(super) fn alloc_boxed_bytes_in_region(&self, type_desc: Arc<TypeDesc>, bytes: &[u8]) -> Value {
        self.alloc_object_block(OBJ_HEADER_BYTES + bytes.len(), |p| {
            // SAFETY: as above; the fields are written before the object is published.
            unsafe {
                p.cast::<ObjHeader>().write(ObjHeader::new(type_desc));
                (*p).cells().copy_in(0, bytes);
            }
        })
    }

    /// Move an object built outside the heap into a fresh block: the header (with its
    /// `Arc<TypeDesc>` and extras) and the fields are copied bitwise, then the detached block's
    /// memory is freed without dropping the header it no longer owns.
    pub(super) fn alloc_owned_in_region(&self, obj: OwnedObject) -> Value {
        let payload = OBJ_HEADER_BYTES + obj.payload_len();
        let block = obj.into_detached();
        self.alloc_object_block(payload, |p| {
            // SAFETY: `block` is exclusively owned and holds a constructed header + `payload`
            // initialized bytes; the destination is a fresh block of the same payload size. The
            // copy moves the header's ownership; `block` is then released as plain memory.
            unsafe { std::ptr::copy_nonoverlapping(block.payload_ptr(), p.cast::<u8>(), payload) };
        })
        // `block` drops here: memory only.
    }
}
