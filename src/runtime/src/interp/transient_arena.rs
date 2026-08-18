//! make-value-copy: per-`VmContext` arena for the four **transient, frame-scoped,
//! immutable-after-creation** `Value` payloads that used to be boxed —
//! `Ref` / `PinnedView` / `StackClosure` / `StructRefHeap`.
//!
//! # Why
//! Those four were the only `Box` variants left in `Value`; together with `GcRef`'s
//! explicit no-op `Drop` they forced `Value: !Copy`, which made `Value::clone` a
//! discriminant-dispatched deep clone (profile #1 leaf, 11.4%) and `Vec<Value>`
//! drop a per-element drop-glue loop (`drop_in_place<Frame>`, 6.0%). Moving each
//! payload into this arena and leaving only an 8-byte `{ idx, frame_id }` handle in
//! the register file lets `Value` be `Copy` → clone is a plain 16B memcpy and the
//! frame register file drops in O(1).
//!
//! # Model
//! Mirrors [`super::struct_arena::StructArena`] / [`super::stack_alloc::StackArena`]:
//! a per-`VmContext` `Vec` guarded by a `Mutex`, `frame_id` staleness guard,
//! LIFO-truncated at `pop_frame` (base stamped by `push_frame`), scanned as a GC
//! root every collection. All four payloads share **one** arena (one context field,
//! one frame base, one `scan_roots`) since they share the same LIFO lifetime; the
//! payload enum is only touched on the cold construct/consume paths, never on the
//! hot register-copy path (which just moves the 8B handle).
//!
//! # GC
//! The arena is a GC **root** (`scan_roots` wired into the context root scanners),
//! so every `GcRef` a live payload holds (a `Ref`'s `RefKind::Array/Field` target,
//! a `StructRefHeap`'s backing array) stays marked without tracing *through* the
//! handle — `Value::visit_gc_children` / `mark_if_unmarked` are therefore no-ops for
//! these variants (exactly like `StructRef` / `StackObject`). No write barrier: the
//! arena is re-scanned as a root each collection.

use crate::metadata::types::{
    PinnedViewData, RefKind, StackClosureData, StructArrayElem, Value,
};
use anyhow::Result;

/// One transient payload. All four are immutable after creation, so a `Value`
/// handle sharing an entry (a cheap 8B copy) never observes a mutation.
pub(crate) enum TransientPayload {
    /// `Value::Ref` target descriptor (spec impl-ref-out-in-runtime).
    Ref(RefKind),
    /// `Value::PinnedView` FFI view (raw ptr + len; no GC leaf).
    PinView(PinnedViewData),
    /// `Value::StackClosure` (env_idx into the frame's `env_arena` + fn name; no GC leaf).
    StackClos(StackClosureData),
    /// `Value::StructRefHeap` — a `struct[]` element identity (holds the backing array `GcRef`).
    StructElem(StructArrayElem),
}

/// One arena entry; `frame_id` = creating frame's id (staleness guard).
pub(crate) struct TransientSlot {
    pub frame_id: u32,
    pub payload: TransientPayload,
}

/// Per-`VmContext` transient-value arena. Guarded by a `Mutex` on the context
/// (owner-thread accesses uncontended; the GC scanner reads it at a safepoint).
#[derive(Default)]
pub(crate) struct TransientArena {
    slots: Vec<TransientSlot>,
    /// Diagnostics: number of transient allocations this run.
    pub allocs: u64,
}

impl TransientArena {
    /// Current length — captured by `push_frame` as a frame's truncation base.
    #[inline]
    pub fn base(&self) -> usize {
        self.slots.len()
    }

    /// LIFO free: drop every payload a frame allocated (called by `pop_frame`).
    #[inline]
    pub fn truncate(&mut self, base: usize) {
        if base < self.slots.len() {
            self.slots.truncate(base);
        }
    }

    /// Allocate a payload for `frame_id`; returns its arena index (paired with
    /// `frame_id` into a `Value::Variant { idx, frame_id }` handle by the caller).
    #[inline]
    pub fn alloc(&mut self, frame_id: u32, payload: TransientPayload) -> u32 {
        let idx = self.slots.len() as u32;
        self.slots.push(TransientSlot { frame_id, payload });
        self.allocs += 1;
        idx
    }

    /// Validated shared access to a payload.
    pub fn with<R>(&self, idx: u32, frame_id: u32, f: impl FnOnce(&TransientPayload) -> R) -> Result<R> {
        let slot = self.slots.get(idx as usize)
            .ok_or_else(|| stale_err(idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stale_err(idx, frame_id));
        }
        Ok(f(&slot.payload))
    }

    /// Clone the `RefKind` behind a `Value::Ref` handle (payload is `Clone`), so the
    /// caller can release the arena lock before touching heap / other locks.
    pub fn ref_kind(&self, idx: u32, frame_id: u32) -> Result<RefKind> {
        self.with(idx, frame_id, |p| match p {
            TransientPayload::Ref(k) => Ok(k.clone()),
            _ => Err(stale_err(idx, frame_id)),
        })?
    }

    /// Clone the `StructArrayElem` behind a `Value::StructRefHeap` handle (cheap:
    /// `{GcRef(Copy), u32}`), releasing the arena lock before touching the array.
    pub fn struct_elem(&self, idx: u32, frame_id: u32) -> Result<StructArrayElem> {
        self.with(idx, frame_id, |p| match p {
            TransientPayload::StructElem(e) => Ok(e.clone()),
            _ => Err(stale_err(idx, frame_id)),
        })?
    }

    /// Clone the `StackClosureData` behind a `Value::StackClosure` handle.
    pub fn stack_closure(&self, idx: u32, frame_id: u32) -> Result<StackClosureData> {
        self.with(idx, frame_id, |p| match p {
            TransientPayload::StackClos(sc) => Ok(sc.clone()),
            _ => Err(stale_err(idx, frame_id)),
        })?
    }

    /// GC root scan: visit every heap `Value` reachable from a live transient
    /// payload (a `Ref`'s Array/Field target, a `StructRefHeap`'s backing array).
    /// `Ref::Stack` / `PinView` / `StackClos` hold no GC leaves.
    pub fn scan_roots(&self, visit: &mut dyn FnMut(&Value)) {
        for slot in &self.slots {
            match &slot.payload {
                TransientPayload::Ref(RefKind::Array { gc_ref, .. }) => {
                    visit(&Value::Array(*gc_ref));
                }
                TransientPayload::Ref(RefKind::Field { gc_ref, .. }) => {
                    visit(&Value::Object(*gc_ref));
                }
                TransientPayload::StructElem(e) => {
                    visit(&Value::Array(e.arr));
                }
                TransientPayload::Ref(RefKind::Stack { .. })
                | TransientPayload::PinView(_)
                | TransientPayload::StackClos(_) => {}
            }
        }
    }
}

/// Clear diagnostic for a stale / out-of-range transient handle — surfaced instead
/// of silent UB (mirrors the stack / struct arena staleness guards).
fn stale_err(idx: u32, frame_id: u32) -> anyhow::Error {
    anyhow::anyhow!(
        "transient-value handle used after its creating frame exited \
         (idx={idx}, frame_id={frame_id}) — Ref/PinnedView/StackClosure/StructRefHeap \
         lifetime unsound"
    )
}

#[cfg(test)]
#[path = "transient_arena_tests.rs"]
mod transient_arena_tests;
