//! add-struct-value-semantics Phase A: per-`VmContext` byte arena for blob value
//! types (multi-field structs stored inline as byte blobs, not GC heap objects).
//!
//! # Model
//! A value-struct local/temp/param lives as a **byte blob** in this per-context
//! arena; registers hold a `Value::StructRef { idx, frame_id }` handle into it.
//! Field access reads/writes primitive leaves at compile-time-baked byte offsets;
//! assignment/param/return copy the blob (value semantics). No GC heap object, no
//! per-struct allocation lock / mark / sweep.
//!
//! Mirrors [`super::stack_alloc::StackArena`] (per-context, LIFO-truncated at
//! `pop_frame`, `frame_id` staleness guard) — see that module for the rationale
//! on per-context (not per-frame) arenas and the LIFO lifetime contract.
//!
//! ## Reference leaves (Phase A scope note)
//! A-support handles **pure-primitive** structs (empty reference bitmap): copy is
//! a raw byte memcpy and GC has nothing to scan. Structs with reference leaves
//! (`string` / object / array fields) need the type's reference bitmap, which
//! reaches the runtime via the zbc TYPE section in A-use — `scan_roots` and the
//! per-kind copy (Arc::clone / GC write barrier) land then.

use crate::metadata::types::Value;
use anyhow::Result;
use std::sync::Arc;

/// One value-struct blob. `frame_id` = creating frame's id (staleness guard);
/// `type_name` = FQ value-type name (used by the GC reference-bitmap scan and by
/// boxing to recover the precise type); `bytes` = the byte-precise blob.
pub(crate) struct StructSlot {
    pub frame_id: u32,
    #[allow(dead_code)] // consumed by GC ref-bitmap scan + boxing in A-use
    pub type_name: Arc<str>,
    pub bytes: Box<[u8]>,
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

    /// Allocate a zero-initialized `size`-byte blob; returns its arena index
    /// (paired with `frame_id` into a `Value::StructRef { idx, frame_id }`).
    #[inline]
    pub fn alloc(&mut self, frame_id: u32, type_name: Arc<str>, size: usize) -> u32 {
        let idx = self.slots.len() as u32;
        self.slots.push(StructSlot {
            frame_id,
            type_name,
            bytes: vec![0u8; size].into_boxed_slice(),
        });
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

    /// Copy `size` bytes from blob `src` into blob `dst` (both pre-allocated).
    /// A-support scope = pure-primitive blobs → raw byte copy. (Reference-leaf
    /// blobs need per-kind Arc::clone / GC barrier by the type's ref-bitmap; that
    /// lands in A-use when the bitmap reaches the runtime.)
    pub fn copy_into(&mut self, dst_idx: u32, dst_frame_id: u32,
                     src_idx: u32, src_frame_id: u32, size: usize) -> Result<()> {
        // Two indices into the same Vec — read src into a temp, then write dst.
        let src_bytes: Vec<u8> = {
            let s = self.slots.get(src_idx as usize)
                .ok_or_else(|| stale_err(src_idx, src_frame_id))?;
            if s.frame_id != src_frame_id {
                return Err(stale_err(src_idx, src_frame_id));
            }
            let n = size.min(s.bytes.len());
            s.bytes[..n].to_vec()
        };
        let d = self.slots.get_mut(dst_idx as usize)
            .ok_or_else(|| stale_err(dst_idx, dst_frame_id))?;
        if d.frame_id != dst_frame_id {
            return Err(stale_err(dst_idx, dst_frame_id));
        }
        let n = src_bytes.len().min(d.bytes.len());
        d.bytes[..n].copy_from_slice(&src_bytes[..n]);
        Ok(())
    }

    /// GC root scan. Pure-primitive blobs have no heap references → nothing to
    /// visit. Reference-leaf scanning (by the type's ref-bitmap) lands in A-use.
    pub fn scan_roots(&self, _visit: &mut dyn FnMut(&Value)) {
        // A-support: pure-primitive structs only — no reference leaves to scan.
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
