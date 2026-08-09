//! add-struct-value-semantics: per-`VmContext` byte arena for blob value types
//! (multi-field structs stored inline, not as GC heap objects).
//!
//! # Model
//! A value-struct local/temp/param lives as a **byte blob** in this per-context
//! arena; registers hold a `Value::StructRef { idx, frame_id }` handle into it.
//! Field access reads/writes leaves at compile-time-baked byte offsets;
//! assignment/param/return copy the blob (value semantics). No GC heap object, no
//! per-struct allocation lock / mark / sweep.
//!
//! Mirrors [`super::stack_alloc::StackArena`] (per-context, LIFO-truncated at
//! `pop_frame`, `frame_id` staleness guard) — see that module for the rationale
//! on per-context (not per-frame) arenas and the LIFO lifetime contract.
//!
//! ## Primitives vs. reference leaves (A-use)
//! Primitive leaves (`int`/`bool`/`char`/`double`/…) are stored **byte-packed**
//! in `StructSlot::bytes` at their layout offset — this is the γ byte-density
//! goal. Reference leaves (`string` / object / array fields) are **not** stored
//! as raw handle bytes inside the `[u8]` blob (that would be memory-unsafe in
//! Rust: an `Arc<str>` / `GcRef` is a managed value whose refcount the byte Vec
//! could not clone/drop, and a moving GC could not rewrite). Instead they live as
//! real `Value`s in a parallel [`StructSlot::refs`] side-slice, ordered by the
//! type's reference bitmap ([`StructTypeLayout::ref_offsets`]). This gives:
//!   * correct per-kind clone/drop on copy (`Value::clone` = `Arc::clone` for
//!     strings, handle copy for objects/arrays);
//!   * trivial, safe GC root scanning (walk the `refs` slice — see `scan_roots`).
//!
//! ## GC: no write barrier needed (A-use P1 scope)
//! The struct arena is scanned as a **GC root** every collection (`scan_roots`
//! is wired into the mark + categorized root scanners in `vm_context`, exactly
//! like `stack_alloc`). References stored into an arena blob are therefore always
//! re-marked, so writing one needs no write barrier. Write barriers are only for
//! references stored into *heap* objects (which are not re-scanned as roots) —
//! that is P3 (struct inlined into a heap object / array), not P1 local structs.

use crate::metadata::types::{StructTypeLayout, Value};
use anyhow::Result;
use std::sync::Arc;

/// One value-struct blob. `frame_id` = creating frame's id (staleness guard);
/// `type_name` = FQ value-type name (used by boxing to recover the precise type);
/// `bytes` = byte-packed primitive leaves; `refs` = reference leaves as `Value`s
/// (see module docs); `layout` = shared type layout (offset→ref-index + copy).
pub(crate) struct StructSlot {
    pub frame_id: u32,
    #[allow(dead_code)] // consumed by boxing (A-use P4) to recover the precise type
    pub type_name: Arc<str>,
    pub bytes: Box<[u8]>,
    pub refs: Box<[Value]>,
    pub layout: Arc<StructTypeLayout>,
}

/// Per-`VmContext` value-struct byte arena. Guarded by a `Mutex` on the context
/// (owner-thread accesses uncontended; GC scanner reads it at a safepoint).
#[derive(Default)]
pub(crate) struct StructArena {
    slots: Vec<StructSlot>,
    /// Diagnostics: number of struct allocations this run.
    pub allocs: u64,
}

impl StructArena {
    /// Current length — captured by `push_frame` as a frame's truncation base.
    #[inline]
    pub fn base(&self) -> usize {
        self.slots.len()
    }

    /// LIFO free: drop every blob a frame allocated (called by `pop_frame`).
    #[inline]
    pub fn truncate(&mut self, base: usize) {
        if base < self.slots.len() {
            self.slots.truncate(base);
        }
    }

    /// Allocate a zero-initialized blob for `layout` (bytes zeroed, reference
    /// leaves defaulted to `Null`); returns its arena index (paired with
    /// `frame_id` into a `Value::StructRef { idx, frame_id }`).
    #[inline]
    pub fn alloc(&mut self, frame_id: u32, type_name: Arc<str>, layout: Arc<StructTypeLayout>) -> u32 {
        let idx = self.slots.len() as u32;
        let bytes = vec![0u8; layout.size].into_boxed_slice();
        let refs = vec![Value::Null; layout.ref_count()].into_boxed_slice();
        self.slots.push(StructSlot { frame_id, type_name, bytes, refs, layout });
        self.allocs += 1;
        idx
    }

    /// Validated shared access to a blob.
    pub fn with<R>(&self, idx: u32, frame_id: u32, f: impl FnOnce(&StructSlot) -> R) -> Result<R> {
        let slot = self.slots.get(idx as usize)
            .ok_or_else(|| stale_err(idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stale_err(idx, frame_id));
        }
        Ok(f(slot))
    }

    /// Validated mutable access to a blob.
    pub fn with_mut<R>(&mut self, idx: u32, frame_id: u32, f: impl FnOnce(&mut StructSlot) -> R) -> Result<R> {
        let slot = self.slots.get_mut(idx as usize)
            .ok_or_else(|| stale_err(idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stale_err(idx, frame_id));
        }
        Ok(f(slot))
    }

    /// Read a reference leaf (`string`/object/array field) at `byte_off` of blob
    /// `idx`, returning a clone of the held `Value` (`Arc::clone` / handle copy).
    pub fn get_ref(&self, idx: u32, frame_id: u32, byte_off: u32) -> Result<Value> {
        self.with(idx, frame_id, |s| {
            let ri = s.layout.ref_index(byte_off).ok_or_else(|| {
                anyhow::anyhow!("struct ref leaf at byte offset {byte_off} not in type layout")
            })?;
            Ok(s.refs[ri].clone())
        })?
    }

    /// Write a reference leaf at `byte_off` of blob `idx` (in place; the lvalue
    /// write). No write barrier: the arena is a GC root (see module docs).
    pub fn set_ref(&mut self, idx: u32, frame_id: u32, byte_off: u32, val: Value) -> Result<()> {
        self.with_mut(idx, frame_id, |s| {
            let ri = s.layout.ref_index(byte_off).ok_or_else(|| {
                anyhow::anyhow!("struct ref leaf at byte offset {byte_off} not in type layout")
            })?;
            s.refs[ri] = val;
            Ok(())
        })?
    }

    /// Copy blob `src` into blob `dst` (both pre-allocated, same type): byte-copy
    /// the primitive leaves + clone each reference leaf (`Value::clone` = per-kind
    /// `Arc::clone` / handle copy). This is the value-semantics copy point
    /// (assign/param/return). No write barrier (arena is a GC root).
    pub fn copy_into(&mut self, dst_idx: u32, dst_frame_id: u32,
                     src_idx: u32, src_frame_id: u32, _size: usize) -> Result<()> {
        // Two indices into the same Vec — snapshot src (bytes + cloned refs),
        // then write dst.
        let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = {
            let s = self.slots.get(src_idx as usize)
                .ok_or_else(|| stale_err(src_idx, src_frame_id))?;
            if s.frame_id != src_frame_id {
                return Err(stale_err(src_idx, src_frame_id));
            }
            (s.bytes.to_vec(), s.refs.to_vec())
        };
        let d = self.slots.get_mut(dst_idx as usize)
            .ok_or_else(|| stale_err(dst_idx, dst_frame_id))?;
        if d.frame_id != dst_frame_id {
            return Err(stale_err(dst_idx, dst_frame_id));
        }
        let n = src_bytes.len().min(d.bytes.len());
        d.bytes[..n].copy_from_slice(&src_bytes[..n]);
        let rn = src_refs.len().min(d.refs.len());
        d.refs[..rn].clone_from_slice(&src_refs[..rn]);
        Ok(())
    }

    /// GC root scan: visit every reference leaf of every live blob so heap refs
    /// held in value structs stay marked. Pure-primitive blobs have empty `refs`
    /// → visit nothing.
    pub fn scan_roots(&self, visit: &mut dyn FnMut(&Value)) {
        for slot in &self.slots {
            for v in slot.refs.iter() {
                visit(v);
            }
        }
    }
}

/// Clear diagnostic for a stale / out-of-range struct handle — surfaced instead
/// of silent UB (mirrors the stack-arena staleness guard).
fn stale_err(idx: u32, frame_id: u32) -> anyhow::Error {
    anyhow::anyhow!(
        "struct-value handle used after its creating frame exited \
         (idx={idx}, frame_id={frame_id}) — value-struct lifetime unsound"
    )
}

#[cfg(test)]
#[path = "struct_arena_tests.rs"]
mod struct_arena_tests;
