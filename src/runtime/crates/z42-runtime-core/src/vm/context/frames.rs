use super::*;

impl VmContext {
    // ── Frame chain (unify-frame-chain) ────────────────────────
    //
    // Single push_frame / pop_frame replaces the previously-separate
    // (push_frame_state / pop_frame_regs) + (push_call_frame / pop_call_frame)
    // pairs. Atomic push of one VmFrame holds GC roots + trace metadata
    // together — caller cannot "forget half".

    /// Push one [`crate::vm::exec::call::frames::VmFrame`] onto the active script frame
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
        let mut a = self.exec.stack_arena.lock();
        let idx = a.alloc_obj(frame_id, obj);
        self.exec.stack_obj_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a stack array; publishes the new `arrs` length. See `stack_arr_len`.
    pub fn stack_alloc_arr(&self, frame_id: u32, arr: crate::vm::exec::call::stack_alloc::StackArray) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.exec.stack_arena.lock();
        let idx = a.alloc_arr(frame_id, arr);
        self.exec.stack_arr_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a value-struct blob; publishes the new length. See `struct_len`.
    pub fn struct_alloc(
        &self, frame_id: u32, type_name: std::sync::Arc<str>,
        layout: std::sync::Arc<crate::object::StructTypeLayout>,
    ) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.exec.struct_arena.lock();
        let idx = a.alloc(frame_id, type_name, layout);
        self.exec.struct_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a transient payload; publishes the new length. See `transient_len`.
    pub fn transient_alloc(
        &self, frame_id: u32, payload: crate::vm::exec::call::transient_arena::TransientPayload,
    ) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.exec.transient_arena.lock();
        let idx = a.alloc(frame_id, payload);
        self.exec.transient_len.store(idx as usize + 1, Relaxed);
        idx
    }

    pub fn push_frame(&self, mut frame: crate::vm::exec::call::frames::VmFrame) {
        use std::sync::atomic::Ordering::Relaxed;
        // perf interp-frame-lock-slim: capture each arena's truncation base from its
        // published-length atomic — a lock-free `Relaxed` load — instead of locking
        // the three arenas. This is the mutator thread (the sole writer of these
        // atomics), so the load observes its own latest publish; `pop_frame`
        // LIFO-truncates each arena back to the base captured here.
        frame.stack_obj_base = arena_base(self.exec.stack_obj_len.load(Relaxed));
        frame.stack_arr_base = arena_base(self.exec.stack_arr_len.load(Relaxed));
        frame.struct_base = arena_base(self.exec.struct_len.load(Relaxed));
        frame.transient_base = arena_base(self.exec.transient_len.load(Relaxed));
        if self.exec.call_stack.push(frame) {
            // Bottom frame: a new activation chain on this thread. Scope this
            // context's VM / heap into the thread-locals until the stack is empty
            // again; nested frames run under them (see `engine_guards`).
            self.exec.engine_guards.install(self);
        }
    }

    /// Pop the most recently pushed frame. No-op when empty (defensive).
    pub fn pop_frame(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let popped = self.exec.call_stack.pop();
        if let Some(f) = popped {
            let (obj_base, arr_base) = (f.stack_obj_base as usize, f.stack_arr_base as usize);
            let (struct_base, transient_base) = (f.struct_base as usize, f.transient_base as usize);
            // perf interp-frame-lock-slim: for each arena, lock + truncate ONLY when
            // this frame actually grew it (published len ≠ stamped base). The
            // call-heavy common case allocates nothing on these arenas, so all three
            // comparisons short-circuit and pop_frame takes no lock at all.
            // Re-publish the post-truncate length under the arena lock.
            if self.exec.stack_obj_len.load(Relaxed) != obj_base
                || self.exec.stack_arr_len.load(Relaxed) != arr_base
            {
                let mut a = self.exec.stack_arena.lock();
                a.truncate(obj_base, arr_base);
                let (o, r) = a.bases();
                self.exec.stack_obj_len.store(o, Relaxed);
                self.exec.stack_arr_len.store(r, Relaxed);
            }
            if self.exec.struct_len.load(Relaxed) != struct_base {
                let mut a = self.exec.struct_arena.lock();
                a.truncate(struct_base);
                self.exec.struct_len.store(a.base(), Relaxed);
            }
            if self.exec.transient_len.load(Relaxed) != transient_base {
                let mut a = self.exec.transient_arena.lock();
                a.truncate(transient_base);
                self.exec.transient_len.store(a.base(), Relaxed);
            }
            if self.exec.call_stack.depth() == 0 {
                self.exec.engine_guards.release();
            }
        }
    }

    /// A fresh frame id, never `0` (`0` = "this frame has not taken one yet").
    /// Called the first time a frame allocates an arena slot (interp
    /// `Frame::frame_id`, JIT `struct_ops::frame_id_of`). Owner thread only, so a
    /// plain load + store, no read-modify-write.
    pub fn next_frame_id(&self) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let id = self.exec.next_frame_id.load(Relaxed);
        let next = id.wrapping_add(1);
        self.exec.next_frame_id.store(if next == 0 { 1 } else { next }, Relaxed);
        id
    }

    /// Stamp the *top* (currently executing) frame's code offset — the packed
    /// `Function::linear_offset(block, instr)` of the call / throw site. Called
    /// right before invoking a callee (and at a throw), so a downstream
    /// snapshot shows that site rather than an earlier one. Line / column are
    /// resolved from it only when a stack trace is built.
    #[inline]
    pub fn set_top_frame_pc(&self, pc: u32) {
        self.exec.call_stack.set_top_pc(pc);
    }

    /// Snapshot the entire call stack for stack-trace formatting at a
    /// `throw` site — names, files and line/column are resolved here.
    /// Only invoked on the throw / fatal-report path.
    pub fn snapshot_call_stack(&self) -> Vec<crate::vm::exec::stack::FrameSnapshot> {
        self.exec.call_stack.with_frames(crate::vm::exec::stack::snapshots)
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
        self.exec.call_stack.regs_at(idx)
    }

    /// Current depth of the frame chain. `frame_state_at(depth - 1)` is
    /// the most recent frame. Used by codegen-generated `LoadLocalAddr`
    /// to produce a `RefKind::Stack { frame_idx }` referencing the
    /// current frame at emission time.
    pub fn frame_stack_depth(&self) -> usize {
        self.exec.call_stack.depth()
    }

    /// Read this context's frames from the GC root scanners. Panics in debug
    /// builds when the scan is neither on the owner thread nor inside a
    /// collection (`collector_active`).
    ///
    /// # Safety
    /// As [`crate::vm::exec::call::frame_stack::FrameStack::scan_parked`]: the caller is the
    /// owner thread, or the owner is parked for the whole call.
    pub unsafe fn scan_frames_parked<R>(
        &self, f: impl FnOnce(&[crate::vm::exec::call::frames::VmFrame]) -> R,
    ) -> R {
        debug_assert!(
            self.exec.call_stack.published_depth() == 0
                || self.exec.call_stack.owned_by_current_thread()
                || self.core.threads.collector_active.load(std::sync::atomic::Ordering::Acquire),
            "another thread's frame stack scanned outside a GC pause"
        );
        // SAFETY: the caller's contract.
        unsafe { self.exec.call_stack.scan_parked(f) }
    }
}

/// An arena length as a frame's `u32` truncation base. Arena slot indices are
/// `u32` (`stack_alloc_obj` & co. return one), so a published length always fits.
#[inline]
fn arena_base(len: usize) -> u32 {
    debug_assert!(len <= u32::MAX as usize, "arena length {len} exceeds u32");
    len as u32
}
