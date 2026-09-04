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
//! trivially — no cross-frame handle machinery. (Stack closures use a per-frame
//! `env_arena` because closures have no ctor sub-call; objects differ.)
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

use crate::metadata::types::{ArrayObj, ScriptObject, Value};
use anyhow::Result;
use std::sync::atomic::{AtomicU32, Ordering};

/// One stack-allocated object; `frame_id` = creating frame's id (staleness guard).
pub(crate) struct StackObjSlot {
    pub frame_id: u32,
    pub obj: ScriptObject,
}

/// One stack-allocated array; `frame_id` = creating frame's id.
pub(crate) struct StackArrSlot {
    pub frame_id: u32,
    pub arr: ArrayObj,
}

/// Per-`VmContext` stack-allocation arena. Guarded by a `Mutex` on the context
/// (owner-thread accesses are uncontended; the GC scanner reads it at a safepoint).
#[derive(Default)]
pub(crate) struct StackArena {
    objs: Vec<StackObjSlot>,
    arrs: Vec<StackArrSlot>,
    /// Diagnostics: number of stack allocations this run (`Z42_STACKALLOC=stats`).
    pub obj_allocs: u64,
    pub arr_allocs: u64,
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
        if obj_base < self.objs.len() {
            self.objs.truncate(obj_base);
        }
        if arr_base < self.arrs.len() {
            self.arrs.truncate(arr_base);
        }
    }

    /// Allocate an object; returns its arena index (paired with `frame_id` into a
    /// `Value::StackObject { idx, frame_id }` handle by the caller).
    #[inline]
    pub fn alloc_obj(&mut self, frame_id: u32, obj: ScriptObject) -> u32 {
        let idx = self.objs.len() as u32;
        self.objs.push(StackObjSlot { frame_id, obj });
        self.obj_allocs += 1;
        idx
    }

    #[inline]
    pub fn alloc_arr(&mut self, frame_id: u32, arr: ArrayObj) -> u32 {
        let idx = self.arrs.len() as u32;
        self.arrs.push(StackArrSlot { frame_id, arr });
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

    /// Validated mutable access to a stack object.
    pub fn with_obj_mut<R>(&mut self, idx: u32, frame_id: u32, f: impl FnOnce(&mut ScriptObject) -> R) -> Result<R> {
        let slot = self.objs.get_mut(idx as usize)
            .ok_or_else(|| stack_stale_err("object", idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stack_stale_err("object", idx, frame_id));
        }
        Ok(f(&mut slot.obj))
    }

    pub fn with_arr<R>(&self, idx: u32, frame_id: u32, f: impl FnOnce(&ArrayObj) -> R) -> Result<R> {
        let slot = self.arrs.get(idx as usize)
            .ok_or_else(|| stack_stale_err("array", idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stack_stale_err("array", idx, frame_id));
        }
        Ok(f(&slot.arr))
    }

    pub fn with_arr_mut<R>(&mut self, idx: u32, frame_id: u32, f: impl FnOnce(&mut ArrayObj) -> R) -> Result<R> {
        let slot = self.arrs.get_mut(idx as usize)
            .ok_or_else(|| stack_stale_err("array", idx, frame_id))?;
        if slot.frame_id != frame_id {
            return Err(stack_stale_err("array", idx, frame_id));
        }
        Ok(f(&mut slot.arr))
    }

    /// GC root scan: visit every heap `Value` reachable from live stack objects /
    /// arrays (their slots/elements may hold `GcRef`s to heap objects that must
    /// stay marked). Mirrors the frame `env_arena` root scan.
    pub fn scan_roots(&self, visit: &mut dyn FnMut(&Value)) {
        for s in &self.objs {
            // unify-object-byte-layout (PR-2): all reference leaves live in `refs`;
            // `bytes` holds only primitives.
            for r in s.obj.refs().iter() {
                visit(r);
            }
        }
        for s in &self.arrs {
            // add-struct-heap-inline (P3b): gc_refs covers Boxed elements + struct[] refs.
            for e in s.arr.gc_refs() {
                visit(e);
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

const MODE_UNSET: u32 = 0;
const MODE_ON: u32 = 1;
const MODE_OFF: u32 = 2;
const MODE_STATS: u32 = 3;
static MODE: AtomicU32 = AtomicU32::new(MODE_UNSET);

fn mode() -> u32 {
    let m = MODE.load(Ordering::Relaxed);
    if m != MODE_UNSET {
        return m;
    }
    let resolved = match std::env::var("Z42_STACKALLOC").ok().as_deref() {
        Some("off") | Some("0") | Some("heap") => MODE_OFF,
        Some("stats") => MODE_STATS,
        _ => MODE_ON,
    };
    MODE.store(resolved, Ordering::Relaxed);
    resolved
}

/// Runtime gate: when `Z42_STACKALLOC=off`, the interp ignores every `stack_alloc`
/// flag and heap-allocates (triage bypass, no recompile). On by default.
#[inline]
pub fn stack_alloc_enabled() -> bool {
    mode() != MODE_OFF
}

/// `Z42_STACKALLOC=stats` — print per-run stack-allocation counts.
#[inline]
pub fn stats_enabled() -> bool {
    mode() == MODE_STATS
}

#[cfg(test)]
#[path = "stack_alloc_tests.rs"]
mod stack_alloc_tests;
