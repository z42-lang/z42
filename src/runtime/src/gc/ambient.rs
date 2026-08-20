//! Ambient GC heap — a thread-local pointer to the active VM's heap so heap-less
//! call sites can allocate GC blocks without threading `&dyn MagrGC` through them
//! (unify-gc-heap PR-4, design **D11**: "the heap is a first-class ambient
//! allocation service", the CLR/JVM model).
//!
//! # Why this exists
//!
//! Migrating `Str` (`metadata/vstr.rs`) from a hand-rolled Arc into a GC
//! variable-length block means `Str::new(s)` — reached from ~189 `.into()` /
//! `From<&str>` sites — must allocate from the GC heap. Those sites do **not**
//! carry a `&VmContext`/`&dyn MagrGC` (the whole point of `.into()` is to be
//! context-free), so rather than rewrite all of them we expose the active heap as
//! an ambient thread-local. Every z42 frame entry (interp `exec_function`, JIT
//! `run_fn`) scopes the heap in via [`HeapGuard`]; `Str::new` reads it via
//! [`current_heap`].
//!
//! # Coverage
//!
//! - **Interp**: `interp::exec_function` installs a guard per frame. Nested JIT
//!   calls (direct native calls, no `run_fn`) run under the enclosing interp
//!   guard, which stays set across the call.
//! - **JIT-at-top**: `jit::JitModule::run_fn` installs a guard so a JIT-first
//!   entry (and any interp it re-enters) is covered.
//! - **Heap-less contexts** (unit tests without a VM, and — before lazy interning
//!   — module load): [`current_heap`] returns `None`; `Str::new` falls back to a
//!   standalone leaked block. Production execution always has an active guard, so
//!   the fallback is not taken on any hot path.
//!
//! Distinct from `native/exports.rs`'s `CURRENT_VM` (which scopes a `*const
//! VmContext` only for native-interop callbacks): that is feature-gated and
//! carries the whole VM; this carries just the heap and is always compiled.

use std::cell::Cell;
use std::ptr::NonNull;

use super::MagrGC;

thread_local! {
    /// The active heap for GC allocations on this thread, or `None` when no z42
    /// frame is executing. A fat `NonNull<dyn MagrGC>` (16 B) — set/restored by
    /// [`HeapGuard`]. Lazy (non-`const`) init: the fat-pointer niche has no cheap
    /// null literal, and the one-time per-thread init cost is negligible.
    static CURRENT_HEAP: Cell<Option<NonNull<dyn MagrGC>>> = const { Cell::new(None) };

    /// **fix-wasm-string-ops**: the active heap's [`MagrGC::heap_epoch`] snapshot (0 = no heap).
    /// Maintained in lockstep with `CURRENT_HEAP` by [`HeapGuard`] so a cheap `u64` read
    /// ([`current_heap_epoch`]) identifies "which heap" without a per-access vtable call.
    /// Address-keyed caches ([`crate::corelib::str_meta`]) read it to detect a heap switch and
    /// drop entries that would otherwise false-hit a malloc-recycled address (wasm32 cross-VM
    /// string corruption). Monotonic epoch ⇒ a recycled address in a new heap never matches.
    static CURRENT_EPOCH: Cell<u64> = const { Cell::new(0) };
}

/// RAII guard scoping `heap` into [`CURRENT_HEAP`] for the guard's lifetime.
/// Nests safely: `enter` saves the previous heap, `drop` restores it, so a nested
/// frame (interp → JIT → interp) leaves the outer heap in place on exit.
pub struct HeapGuard {
    prev: Option<NonNull<dyn MagrGC>>,
    /// **fix-wasm-string-ops**: the epoch to restore on drop (saved alongside `prev`).
    prev_epoch: u64,
    /// `false` when `enter` found the ambient heap already set to this same heap
    /// (a nested frame under the same VM/thread): the store was skipped, so drop
    /// must NOT restore and skips its own TLS access. Only the outermost frame per
    /// heap has `active == true`.
    active: bool,
}

impl HeapGuard {
    /// Scope `heap` as the ambient heap until the returned guard drops.
    #[inline]
    pub fn enter(heap: &dyn MagrGC) -> Self {
        // `NonNull::from(&dyn)` keeps the fat pointer (data + vtable). Erase the
        // borrow's lifetime to `'static` for storage: every read via `current_heap`
        // is transient and completes before this guard drops (the guard lives for
        // the whole frame), and the heap outlives the guard (it lives in `VmCore`).
        let ptr: NonNull<dyn MagrGC> = NonNull::from(heap);
        // SAFETY: the transmute only widens the trait object's lifetime (identical
        // fat-pointer representation); soundness rests on the transient-use contract.
        let ptr: NonNull<dyn MagrGC + 'static> = unsafe { std::mem::transmute(ptr) };
        // perf: interp/JIT install a guard PER FRAME, but the ambient heap is
        // constant across a call tree (same `VmCore` heap). When a nested frame
        // re-enters with the SAME heap, skip both the store and the drop-time
        // restore — halving per-frame TLS traffic (macOS resolves each `.with()`
        // through a `_tlv_get_addr` call). A genuinely different heap (cross-VM
        // native re-entry on the same thread) still saves+installs+restores.
        CURRENT_HEAP.with(|c| {
            let cur = c.get();
            if cur == Some(ptr) {
                // Nested frame under the same heap: epoch unchanged, skip TLS churn.
                HeapGuard { prev: cur, prev_epoch: 0, active: false }
            } else {
                c.set(Some(ptr));
                // fix-wasm-string-ops: install this heap's epoch in lockstep. Read once here
                // (per heap switch), not per allocation, so the hot path stays vtable-free.
                let prev_epoch = CURRENT_EPOCH.with(|e| e.replace(heap.heap_epoch()));
                HeapGuard { prev: cur, prev_epoch, active: true }
            }
        })
    }
}

impl Drop for HeapGuard {
    #[inline]
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        CURRENT_HEAP.with(|c| c.set(self.prev));
        // fix-wasm-string-ops: restore the epoch saved at enter (mirror of CURRENT_HEAP).
        CURRENT_EPOCH.with(|e| e.set(self.prev_epoch));
    }
}

/// Borrow the ambient heap for this thread, or `None` if no z42 frame is active
/// (heap-less contexts: pre-VM module load, unit tests without a VM).
///
/// The returned reference's lifetime is unbounded (`'static`) — matching
/// `native::exports::current_vm`'s pattern — and is sound only for **transient**
/// use that completes before the installing [`HeapGuard`] drops. Callers
/// (`Str::new`) allocate and return an owned handle without letting the borrow
/// escape.
#[inline]
pub fn current_heap() -> Option<&'static dyn MagrGC> {
    CURRENT_HEAP.with(|c| {
        c.get().map(|p| {
            // SAFETY: a live guard set this pointer from a `&dyn MagrGC` whose heap
            // outlives the guard (the heap lives in `VmCore`, the frame's ancestor);
            // the borrow is used transiently within the current call.
            unsafe { &*p.as_ptr() }
        })
    })
}

/// **fix-wasm-string-ops**: the active heap's [`MagrGC::heap_epoch`] (0 when no z42 frame is
/// executing). A cheap `u64` TLS read — the epoch is snapshotted into `CURRENT_EPOCH` by
/// [`HeapGuard`] on each heap switch, so this avoids a vtable call on hot per-char callers.
/// Used by [`crate::corelib::str_meta`] to scope its address-keyed cache to a single heap:
/// a monotonic, never-reused epoch means a malloc-recycled block address in a *new* heap can
/// never collide with a stale entry from a torn-down heap.
#[inline]
pub fn current_heap_epoch() -> u64 {
    CURRENT_EPOCH.with(|e| e.get())
}
