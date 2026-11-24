//! Execution state owned by one running context, independent of concrete engines.
use std::sync::{Arc, atomic::{AtomicU32, AtomicUsize}};
use parking_lot::Mutex;
use crate::object::Value;

pub struct ThreadExecState {
    /// GC-rooted handoff across a JIT error boundary.
    pub pending_exception: Arc<Mutex<Option<Value>>>,
    /// Callback builtins preserve the original thrown object through this root.
    pub pending_thrown: Mutex<Option<Value>>,
    /// Owner-thread only; other threads read only after the owner is parked.
    pub call_stack: super::call::frame_stack::FrameStack,
    pub reg_pool: super::call::reg_pool::RegPool,
    /// Installed at the bottom push and restored after the last pop.
    engine_guards: super::call::engine_guards::EngineGuards,
    /// LIFO frame-scoped payloads, scanned under each arena lock during a pause.
    pub stack_arena: Arc<Mutex<super::call::stack_alloc::StackArena>>,
    pub struct_arena: Arc<Mutex<super::call::struct_arena::StructArena>>,
    pub transient_arena: Arc<Mutex<super::call::transient_arena::TransientArena>>,
    /// Single-mutator published lengths; GC reads arena data under its locks.
    /// Relaxed reads let push/pop skip locks for unchanged arena lengths.
    pub stack_obj_len: AtomicUsize,
    pub stack_arr_len: AtomicUsize,
    pub struct_len: AtomicUsize,
    pub transient_len: AtomicUsize,
    /// Owner-thread source; zero is reserved for a frame that has taken no id.
    pub next_frame_id: AtomicU32,
    pub jit_head: JitHead,
}

impl ThreadExecState {
    pub(crate) fn new(cctors: &Arc<crate::metadata::statics::CctorRegistry>, engines: &super::engine::EngineRegistry) -> Self {
        Self {
            pending_exception: Arc::new(Mutex::new(None)),
            pending_thrown: Mutex::new(None),
            call_stack: Default::default(),
            reg_pool: Default::default(),
            engine_guards: Default::default(),
            stack_arena: Arc::new(Mutex::new(Default::default())),
            struct_arena: Arc::new(Mutex::new(Default::default())),
            transient_arena: Arc::new(Mutex::new(Default::default())),
            stack_obj_len: AtomicUsize::new(0),
            stack_arr_len: AtomicUsize::new(0),
            struct_len: AtomicUsize::new(0),
            transient_len: AtomicUsize::new(0),
            next_frame_id: AtomicU32::new(1),
            jit_head: JitHead::new(cctors, engines),
        }
    }
}

/// The part of a [`crate::vm::VmContext`] that JIT-compiled code reads at fixed offsets.
///
/// Every compiled function and JIT helper receives the **running thread's own**
/// `VmContext` as its context parameter, so these are per-thread values read
/// without a helper call:
///
/// - `stack_limit` — the prologue's stack-overflow bound
///   ([`crate::vm::VM_CONTEXT_STACK_LIMIT_OFFSET`]);
/// - `cctors` — the VM's cctor registry, whose package-init gate counters the
///   inline string intrinsics read ([`crate::vm::VM_CONTEXT_CCTORS_OFFSET`]).
///
/// It also caches the VM's neutral tier header pointer (`jit`). Backend state
/// remains owned by the registered tier; compiled reads use layout-derived offsets.
///
/// The safepoint throttle counter is the third JIT-read field; it is stored in
/// the context's ThreadLocalState (the interpreter decrements it too) and is
/// addressed by [`crate::vm::VM_CONTEXT_SAFEPOINT_SKIP_OFFSET`].
#[repr(C)]
pub struct JitHead {
    /// Lowest stack address a JIT function may start at: `exec::stack_guard::limit()`
    /// of the thread driving this context, stored at its bottom frame push
    /// (`push_frame` → `EngineGuards::install`), i.e. before any compiled code
    /// runs on it. `0` until then (`sp < 0` never holds: no check). Written and
    /// read by the owner thread only; atomic only to keep the context `Sync`.
    pub stack_limit: std::sync::atomic::AtomicUsize,
    /// `Arc::as_ptr(cctors)` — stable for the core's life, set at
    /// construction.
    pub cctors: *const crate::metadata::statics::CctorRegistry,
    /// The registered neutral tier header (null until installed):
    /// taken at construction, published by the tier installer for the
    /// installing context, and refreshed at the bottom frame push for a context
    /// that existed before the install. Points into the core this context keeps
    /// alive. Published with release ordering and borrowed with acquire ordering.
    #[cfg(feature = "jit")]
    pub(super) jit: std::sync::atomic::AtomicPtr<crate::exec::engine::TierApi>,
}

// SAFETY: `cctors` points into the `Arc<CctorRegistry>` the context's own core
// keeps alive; the registry is `Sync` and only read through the pointer.
unsafe impl Send for JitHead {}
unsafe impl Sync for JitHead {}

impl JitHead {
    fn new(cctors: &Arc<crate::metadata::statics::CctorRegistry>, engines: &super::engine::EngineRegistry) -> Self {
        #[cfg(not(feature = "jit"))]
        let _ = engines;
        JitHead {
            stack_limit: std::sync::atomic::AtomicUsize::new(0),
            cctors: Arc::as_ptr(cctors),
            #[cfg(feature = "jit")]
            jit: std::sync::atomic::AtomicPtr::new(
                engines.tier().map_or(std::ptr::null_mut(), |rt| rt as *const _ as *mut _)),
        }
    }
}


impl ThreadExecState {
    // ── JIT exception bridge ──────────────────────────────────────────────

    /// JIT helpers store a thrown user value here; the JIT entry sees the
    /// `extern "C"` return code = 1 and pulls the value via
    /// `take_exception()` to propagate as `ExecOutcome::Thrown`.
    pub fn set_exception(&self, val: Value) {
        *self.pending_exception.lock() = Some(val);
    }

    /// Pop the pending exception (called once per `extern "C"` failure).
    pub fn take_exception(&self) -> Option<Value> {
        self.pending_exception.lock().take()
    }

    /// Peek at the pending exception without removing it. Used by JIT catch-type
    /// dispatch (catch-by-generic-type): the throw helper has set the
    /// exception, the dispatch helper inspects its class to decide which catch
    /// handler to jump to, and a later `take_exception` (via `jit_install_catch`)
    /// hands the value to the chosen catch register.
    pub fn peek_exception(&self) -> Option<Value> {
        self.pending_exception.lock().clone()
    }
    // ── Pending reflected throw (add-method-invoke-non-generic) ──────────────

    /// Stash a z42 exception value to propagate through a callback builtin's
    /// error channel (see `pending_thrown`). Set immediately before the builtin
    /// returns `Err`; consumed once by `take_pending_thrown`.
    pub fn set_pending_thrown(&self, val: Value) {
        *self.pending_thrown.lock() = Some(val);
    }

    /// Take (and clear) a pending thrown exception value, if any. Called by
    /// `exec_call::builtin` in its error handler so the ORIGINAL thrown value
    /// (with its real type) re-enters z42 exception handling.
    pub fn take_pending_thrown(&self) -> Option<Value> {
        self.pending_thrown.lock().take()
    }

}


#[cfg(all(test, z42_test_facade))]
#[path = "thread_state_tests.rs"]
mod tests;



impl ThreadExecState {
    // ── Frame chain (unify-frame-chain) ────────────────────────
    //
    // Single push_frame / pop_frame replaces the previously-separate
    // (push_frame_state / pop_frame_regs) + (push_call_frame / pop_call_frame)
    // pairs. Atomic push of one VmFrame holds GC roots + trace metadata
    // together — caller cannot "forget half".

    /// Push one [`crate::exec::frames::VmFrame`] onto the active script frame
    /// chain. Pop is the caller's responsibility (typically via the
    /// interp `FrameGuard` RAII or the explicit pair in JIT helpers).
    // ── perf interp-frame-lock-slim: arena-alloc funnel ─────────────────────────
    // Every arena allocation MUST route through one of these four wrappers so the
    // published length atomic stays in lock-step with the inner Vec. A raw
    // `ctx.exec.stack_arena.lock().alloc_obj(..)` that bypassed the wrapper would leave
    // `stack_obj_len` stale → `pop_frame` would wrongly skip a needed truncate (an
    // arena leak, not a crash — the `frame_id` staleness check still guards reads).
    // The length store happens UNDER the arena lock (single-writer publish).

    /// Allocate a stack object; publishes the new `objs` length. See `stack_obj_len`.
    pub fn stack_alloc_obj(&self, frame_id: u32, obj: crate::object::OwnedObject) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.stack_arena.lock();
        let idx = a.alloc_obj(frame_id, obj);
        self.stack_obj_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a stack array; publishes the new `arrs` length. See `stack_arr_len`.
    pub fn stack_alloc_arr(&self, frame_id: u32, arr: crate::exec::stack_alloc::StackArray) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.stack_arena.lock();
        let idx = a.alloc_arr(frame_id, arr);
        self.stack_arr_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a value-struct blob; publishes the new length. See `struct_len`.
    pub fn struct_alloc(
        &self, frame_id: u32, type_name: std::sync::Arc<str>,
        layout: std::sync::Arc<crate::object::StructTypeLayout>,
    ) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.struct_arena.lock();
        let idx = a.alloc(frame_id, type_name, layout);
        self.struct_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a transient payload; publishes the new length. See `transient_len`.
    pub fn transient_alloc(
        &self, frame_id: u32, payload: crate::exec::transient_arena::TransientPayload,
    ) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.transient_arena.lock();
        let idx = a.alloc(frame_id, payload);
        self.transient_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Pop the most recently pushed frame. No-op when empty (defensive).
    pub fn pop_frame(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let popped = self.call_stack.pop();
        if let Some(f) = popped {
            let (obj_base, arr_base) = (f.stack_obj_base as usize, f.stack_arr_base as usize);
            let (struct_base, transient_base) = (f.struct_base as usize, f.transient_base as usize);
            // perf interp-frame-lock-slim: for each arena, lock + truncate ONLY when
            // this frame actually grew it (published len ≠ stamped base). The
            // call-heavy common case allocates nothing on these arenas, so all three
            // comparisons short-circuit and pop_frame takes no lock at all.
            // Re-publish the post-truncate length under the arena lock.
            if self.stack_obj_len.load(Relaxed) != obj_base
                || self.stack_arr_len.load(Relaxed) != arr_base
            {
                let mut a = self.stack_arena.lock();
                a.truncate(obj_base, arr_base);
                let (o, r) = a.bases();
                self.stack_obj_len.store(o, Relaxed);
                self.stack_arr_len.store(r, Relaxed);
            }
            if self.struct_len.load(Relaxed) != struct_base {
                let mut a = self.struct_arena.lock();
                a.truncate(struct_base);
                self.struct_len.store(a.base(), Relaxed);
            }
            if self.transient_len.load(Relaxed) != transient_base {
                let mut a = self.transient_arena.lock();
                a.truncate(transient_base);
                self.transient_len.store(a.base(), Relaxed);
            }
            if self.call_stack.depth() == 0 {
                self.engine_guards.release();
            }
        }
    }

    /// A fresh frame id, never `0` (`0` = "this frame has not taken one yet").
    /// Called the first time a frame allocates an arena slot (interp
    /// `Frame::frame_id`, JIT `struct_ops::frame_id_of`). Owner thread only, so a
    /// plain load + store, no read-modify-write.
    pub fn next_frame_id(&self) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let id = self.next_frame_id.load(Relaxed);
        let next = id.wrapping_add(1);
        self.next_frame_id.store(if next == 0 { 1 } else { next }, Relaxed);
        id
    }

    /// Stamp the *top* (currently executing) frame's code offset — the packed
    /// `Function::linear_offset(block, instr)` of the call / throw site. Called
    /// right before invoking a callee (and at a throw), so a downstream
    /// snapshot shows that site rather than an earlier one. Line / column are
    /// resolved from it only when a stack trace is built.
    #[inline]
    pub fn set_top_frame_pc(&self, pc: u32) {
        self.call_stack.set_top_pc(pc);
    }

    /// Snapshot the entire call stack for stack-trace formatting at a
    /// `throw` site — names, files and line/column are resolved here.
    /// Only invoked on the throw / fatal-report path.
    pub(crate) fn snapshot_call_stack(&self) -> Vec<crate::exec::stack::FrameSnapshot> {
        self.call_stack.with_frames(crate::exec::stack::snapshots)
    }

    /// Spec impl-ref-out-in-runtime (Decision R1): index into the frame
    /// chain and return a raw pointer to that frame's `regs` Vec.
    /// Used by `Value::Ref { kind: RefKind::Stack { frame_idx, .. } }`
    /// transparent deref in `Frame::get/set`.
    ///
    /// # Safety
    /// Caller must use the returned pointer only while the corresponding frame
    /// is still alive (guaranteed by spec design Decision 9: refs never
    /// escape the call stack — popped frames cannot be referenced). Like every
    /// frame-stack accessor here, it runs on the context's owner thread.
    pub fn frame_state_at(&self, idx: usize) -> Option<*const Vec<Value>> {
        self.call_stack.regs_at(idx)
    }

    /// Current depth of the frame chain. `frame_state_at(depth - 1)` is
    /// the most recent frame. Used by codegen-generated `LoadLocalAddr`
    /// to produce a `RefKind::Stack { frame_idx }` referencing the
    /// current frame at emission time.
    pub fn frame_stack_depth(&self) -> usize {
        self.call_stack.depth()
    }

    /// Read this context's frames from the GC root scanners. Panics in debug
    /// builds when the scan is neither on the owner thread nor inside a
    /// collection (`collector_active`).
    ///
    /// # Safety
    /// As [`crate::exec::frame_stack::FrameStack::scan_parked`]: the caller is the
    /// owner thread, or the owner is parked for the whole call.
    pub(crate) unsafe fn scan_frames_parked<R>(
        &self, collector_active: bool, f: impl FnOnce(&[crate::exec::frames::VmFrame]) -> R,
    ) -> R {
        debug_assert!(
            self.call_stack.published_depth() == 0
                || self.call_stack.owned_by_current_thread()
                || collector_active,
            "another thread's frame stack scanned outside a GC pause"
        );
        // SAFETY: the caller's contract.
        unsafe { self.call_stack.scan_parked(f) }
    }
}

/// An arena length as a frame's `u32` truncation base. Arena slot indices are
/// `u32` (`stack_alloc_obj` & co. return one), so a published length always fits.
#[inline]
fn arena_base(len: usize) -> u32 {
    debug_assert!(len <= u32::MAX as usize, "arena length {len} exceeds u32");
    len as u32
}

/// Register a frame in the running context and install its guards at the bottom frame.
pub fn push_frame(ctx: &crate::vm::VmContext, mut frame: crate::exec::frames::VmFrame) {
    use std::sync::atomic::Ordering::Relaxed;
    // perf interp-frame-lock-slim: capture each arena's truncation base from its
    // published-length atomic — a lock-free `Relaxed` load — instead of locking
    // the three arenas. This is the mutator thread (the sole writer of these
    // atomics), so the load observes its own latest publish; `pop_frame`
    // LIFO-truncates each arena back to the base captured here.
    let state = ctx.exec();
    frame.stack_obj_base = arena_base(state.stack_obj_len.load(Relaxed));
    frame.stack_arr_base = arena_base(state.stack_arr_len.load(Relaxed));
    frame.struct_base = arena_base(state.struct_len.load(Relaxed));
    frame.transient_base = arena_base(state.transient_len.load(Relaxed));
    if state.call_stack.push(frame) {
        // Bottom frame: a new activation chain on this thread. Scope this
        // context's VM / heap into the thread-locals until the stack is empty
        // again; nested frames run under them (see `engine_guards`).
        state.engine_guards.install(ctx);
    }
}

/// Borrow the active tier from the running context.
#[inline]
pub fn current_tier(ctx: &crate::vm::VmContext) -> Option<&super::engine::TierApi> {
    #[cfg(feature = "jit")]
    {
        let pointer = ctx.exec().jit_head.jit.load(std::sync::atomic::Ordering::Acquire);
        // SAFETY: construction and refresh only cache tiers from the core the
        // context owns. The registry retains the installed tier for its lifetime.
        unsafe { pointer.as_ref() }
    }
    #[cfg(not(feature = "jit"))]
    {
        ctx.core().exec().engines.tier()
    }
}

/// Cache the tier from this context's own Core after attachment or at frame entry.
#[inline]
pub fn refresh_tier(ctx: &crate::vm::VmContext) {
    #[cfg(feature = "jit")]
    {
        use std::sync::atomic::Ordering::{Relaxed, Release};

        let slot = &ctx.exec().jit_head.jit;
        if slot.load(Relaxed).is_null() {
            if let Some(tier) = ctx.core().exec().engines.tier() {
                slot.store(std::ptr::from_ref(tier).cast_mut(), Release);
            }
        }
    }
    #[cfg(not(feature = "jit"))]
    let _ = ctx;
}
