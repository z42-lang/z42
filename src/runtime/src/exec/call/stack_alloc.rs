//! add-escape-analysis-stack-alloc: per-thread stack-allocation arena for
//! escape-analysis-proven non-escaping objects/arrays (interp only).
//!
//! # Model
//! The compiler's `IrEscapeAnalysis` pass marks `ObjNew`/`ArrayNew`/`ArrayNewLit`
//! whose result provably does not escape its creating frame with `stack_alloc`.
//! At runtime the interp allocates such objects/arrays here — in a **per-`VmContext`
//! (per-thread) arena** — instead of the GC heap. Wins: no region-alloc lock, no
//! GC tracking / mark / sweep for these objects.
//!
//! ## Why per-context (not per-frame)
//! `new Foo(a,b)` runs its ctor in a **child frame** with `this` passed as a Value.
//! A per-frame arena index would be meaningless in the ctor's frame. A per-context
//! arena is reachable from every frame via `ctx`, so the ctor resolves `this`
//! trivially — no cross-frame handle machinery.
//!
//! ## Lifetime — LIFO truncation
//! Frames nest (LIFO), so their stack allocations nest too. Each frame records the
//! arena lengths at entry (`VmFrame::stack_obj_base` / `stack_arr_base`, stamped by
//! `push_frame`); `pop_frame` truncates back to them, bulk-freeing that frame's
//! stack objects. Escape analysis guarantees no handle to them survives the frame.
//!
//! ## Diagnostics (the optimization's failure mode is a dangling stack ref)
//! Every access validates `idx < len` **and** `slot.frame_id == handle.frame_id`.
//! After a frame truncates, a reused slot carries a *different* `frame_id`, so a
//! stale handle (escape analysis miscompiled) surfaces as a **clear error**
//! (`stack_stale_err`) instead of silent use-after-free. `Z42_STACKALLOC=off`
//! bypasses stack allocation at runtime (heap everything) for triage without a
//! recompile; `=stats` prints hit counts.

use crate::object::{OwnedObject, ScriptObject, Value};
use anyhow::Result;

/// One stack-allocated object; `frame_id` = creating frame's id (staleness guard). The object
/// lives in a detached block shaped exactly like a heap object's (`OwnedObject`, object model
/// R6), so `&ScriptObject` works identically on both.
struct StackObjSlot {
    frame_id: u32,
    obj: OwnedObject,
}

/// A frame-owned array can hold plain values under the arena lock.
struct StackArrSlot {
    frame_id: u32,
    elems: Vec<Value>,
}

/// Per-`VmContext` stack-allocation arena. Guarded by a `Mutex` on the context
/// (owner-thread accesses are uncontended; the GC scanner reads it at a safepoint).
#[derive(Default)]
pub(in crate::exec) struct StackArena {
    objs: Vec<StackObjSlot>,
    arrs: Vec<StackArrSlot>,
    /// Diagnostics: number of stack allocations this run (`Z42_STACKALLOC=stats`).
    obj_allocs: u64,
    arr_allocs: u64,
}

impl StackArena {
    /// Current lengths — captured by `push_frame` as a frame's truncation base.
    #[inline]
    pub fn bases(&self) -> (usize, usize) {
        (self.objs.len(), self.arrs.len())
    }

    /// LIFO free: drop everything a frame allocated (called by `pop_frame`).
    #[inline]
    pub fn truncate(&mut self, obj_base: usize, arr_base: usize) {
        self.objs.truncate(obj_base);
        self.arrs.truncate(arr_base);
    }

    /// Allocate an object; returns its arena index (paired with `frame_id` into a
    /// `Value::StackObject { idx, frame_id }` handle by the caller).
    #[inline]
    pub fn alloc_obj(&mut self, frame_id: u32, obj: OwnedObject) -> u32 {
        let idx = self.objs.len() as u32;
        self.objs.push(StackObjSlot { frame_id, obj });
        self.obj_allocs += 1;
        idx
    }

    #[inline]
    pub fn alloc_arr(&mut self, frame_id: u32, elems: Vec<Value>) -> u32 {
        let idx = self.arrs.len() as u32;
        self.arrs.push(StackArrSlot { frame_id, elems });
        self.arr_allocs += 1;
        idx
    }

    /// Validated shared access to a stack object.
    pub fn with_obj<R>(&self, idx: u32, frame_id: u32, f: impl FnOnce(&ScriptObject) -> R) -> Result<R> {
        let slot = self.objs.get(idx as usize)
            .ok_or_else(|| stack_stale_err("object", idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stack_stale_err("object", idx, frame_id));
        }
        Ok(f(&slot.obj))
    }

    pub fn with_arr<R>(&self, idx: u32, frame_id: u32, f: impl FnOnce(&[Value]) -> R) -> Result<R> {
        let slot = self.arrs.get(idx as usize)
            .ok_or_else(|| stack_stale_err("array", idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stack_stale_err("array", idx, frame_id));
        }
        Ok(f(&slot.elems))
    }

    pub fn with_arr_mut<R>(&mut self, idx: u32, frame_id: u32, f: impl FnOnce(&mut [Value]) -> R) -> Result<R> {
        let slot = self.arrs.get_mut(idx as usize)
            .ok_or_else(|| stack_stale_err("array", idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stack_stale_err("array", idx, frame_id));
        }
        Ok(f(&mut slot.elems))
    }

    /// GC root scan: visit every heap `Value` reachable from live stack objects /
    /// arrays (their slots/elements may hold `ArrRef`s to heap objects that must
    /// stay marked).
    pub fn scan_roots(&self, visit: &mut dyn FnMut(&Value)) {
        for s in &self.objs {
            // Every reference edge: the 16 B side table AND the 8 B reference words in
            // `bytes`. **fix-stackalloc-misses-inlined-refs**: this once scanned only the side
            // table while direct reference fields had moved into `bytes`; a non-escaping
            // object's array fields were then reachable from no root, a minor swept them while
            // the owner was live, and the stale handle resolved to whatever reused the slot
            // (`long[]` read back a `Char`). `Z42_GC_NURSERY_BYTES=65536` reproduces such a
            // miss in seconds; `stack_alloc_tests` pins it.
            s.obj.visit_refs(visit);
        }
        for s in &self.arrs {
            for value in &s.elems {
                visit(value);
            }
        }
    }
}

/// Clear diagnostic for a stale / out-of-range stack handle — the escape-analysis
/// optimization's one dangerous failure mode, surfaced instead of silent UB.
fn stack_stale_err(kind: &str, idx: u32, frame_id: u32) -> anyhow::Error {
    anyhow::anyhow!(
        "stack-alloc {kind} handle used after its creating frame exited \
         (idx={idx}, frame_id={frame_id}) — escape analysis miscompiled; \
         run with Z42_STACKALLOC=off to confirm and bisect"
    )
}

// ── Runtime toggle / diagnostics ────────────────────────────────────────────────

/// Resolved by the facade; execution never interprets process configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackAllocPolicy { On, Off, Stats }

/// Whether this VM permits interpreter stack-allocation hints.
#[inline]
pub fn stack_alloc_enabled(ctx: &crate::vm::VmContext) -> bool {
    ctx.core.exec.stack_alloc != StackAllocPolicy::Off
}

/// Whether this VM requests the existing stack-allocation statistics.
#[inline]
pub fn stats_enabled(ctx: &crate::vm::VmContext) -> bool {
    ctx.core.exec.stack_alloc == StackAllocPolicy::Stats
}

#[cfg(all(test, z42_test_facade))]
#[path = "stack_alloc_tests.rs"]
mod stack_alloc_tests;
