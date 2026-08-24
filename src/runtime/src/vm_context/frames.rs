use super::*;

impl VmContext {
    // ── Interp exec stack（Phase 3f） ─────────────────────────────────────

    /// Push current frame's regs pointer onto exec_stack, used by GC root
    /// scanning. Caller must guarantee pointer stays valid until matching
    /// `pop_frame_regs()` (typically via `FrameGuard` RAII).
    /// 2026-05-02 add-method-group-conversion (D1b): ensure VmContext has at
    /// least `n` FuncRef cache slots allocated. Idempotent — only grows.
    pub fn alloc_func_ref_slots(&self, n: u32) {
        let mut s = self.func_ref_slots.lock();
        if s.len() < n as usize {
            s.resize(n as usize, Value::Null);
        }
    }

    /// LoadFnCached read: returns slot value, or `Value::Null` if uninitialised
    /// (caller's responsibility to fill on first miss). Bounds-checked.
    pub(crate) fn func_ref_slot(&self, idx: u32) -> Value {
        self.func_ref_slots
            .lock()
            .get(idx as usize)
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// LoadFnCached write: store a `Value::FuncRef` into the slot for future hits.
    pub(crate) fn set_func_ref_slot(&self, idx: u32, value: Value) {
        let mut s = self.func_ref_slots.lock();
        if (idx as usize) >= s.len() {
            s.resize((idx as usize) + 1, Value::Null);
        }
        s[idx as usize] = value;
    }

    // ── Frame chain (2026-05-10 unify-frame-chain) ────────────────────────
    //
    // Single push_frame / pop_frame replaces the previously-separate
    // (push_frame_state / pop_frame_regs) + (push_call_frame / pop_call_frame)
    // pairs. Atomic push of one VmFrame holds GC roots + trace metadata
    // together — caller cannot "forget half".

    /// Push one [`crate::exception::VmFrame`] onto the active script frame
    /// chain. Pop is the caller's responsibility (typically via the
    /// interp `FrameGuard` RAII or the explicit pair in JIT helpers).
    // ── perf interp-frame-lock-slim: arena-alloc funnel ─────────────────────────
    // Every arena allocation MUST route through one of these four wrappers so the
    // published length atomic stays in lock-step with the inner Vec. A raw
    // `ctx.stack_arena.lock().alloc_obj(..)` that bypassed the wrapper would leave
    // `stack_obj_len` stale → `pop_frame` would wrongly skip a needed truncate (an
    // arena leak, not a crash — the `frame_id` staleness check still guards reads).
    // The length store happens UNDER the arena lock (single-writer publish).

    /// Allocate a stack object; publishes the new `objs` length. See `stack_obj_len`.
    pub(crate) fn stack_alloc_obj(&self, frame_id: u32, obj: crate::metadata::types::ScriptObject) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.stack_arena.lock();
        let idx = a.alloc_obj(frame_id, obj);
        self.stack_obj_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a stack array; publishes the new `arrs` length. See `stack_arr_len`.
    pub(crate) fn stack_alloc_arr(&self, frame_id: u32, arr: crate::metadata::types::ArrayObj) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.stack_arena.lock();
        let idx = a.alloc_arr(frame_id, arr);
        self.stack_arr_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a value-struct blob; publishes the new length. See `struct_len`.
    pub(crate) fn struct_alloc(
        &self, frame_id: u32, type_name: std::sync::Arc<str>,
        layout: std::sync::Arc<crate::metadata::types::StructTypeLayout>,
    ) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.struct_arena.lock();
        let idx = a.alloc(frame_id, type_name, layout);
        self.struct_len.store(idx as usize + 1, Relaxed);
        idx
    }

    /// Allocate a transient payload; publishes the new length. See `transient_len`.
    pub(crate) fn transient_alloc(
        &self, frame_id: u32, payload: crate::interp::transient_arena::TransientPayload,
    ) -> u32 {
        use std::sync::atomic::Ordering::Relaxed;
        let mut a = self.transient_arena.lock();
        let idx = a.alloc(frame_id, payload);
        self.transient_len.store(idx as usize + 1, Relaxed);
        idx
    }

    pub(crate) fn push_frame(&self, mut frame: crate::exception::VmFrame) {
        use std::sync::atomic::Ordering::Relaxed;
        // perf interp-frame-lock-slim: capture each arena's truncation base from its
        // published-length atomic — a lock-free `Relaxed` load — instead of locking
        // the three arenas. This is the mutator thread (the sole writer of these
        // atomics), so the load observes its own latest publish; `pop_frame`
        // LIFO-truncates each arena back to the base captured here.
        frame.stack_obj_base = self.stack_obj_len.load(Relaxed);
        frame.stack_arr_base = self.stack_arr_len.load(Relaxed);
        frame.struct_base = self.struct_len.load(Relaxed);
        frame.transient_base = self.transient_len.load(Relaxed);
        self.call_stack.lock().push(frame);
    }

    /// Pop the most recently pushed frame. No-op when empty (defensive).
    pub(crate) fn pop_frame(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        // Pop the call_stack first (release its lock) before touching the arenas.
        let popped = self.call_stack.lock().pop();
        if let Some(f) = popped {
            // perf interp-frame-lock-slim: for each arena, lock + truncate ONLY when
            // this frame actually grew it (published len ≠ stamped base). The
            // call-heavy common case allocates nothing on these arenas, so all three
            // comparisons short-circuit and pop_frame takes just the one call_stack
            // lock above. Re-publish the post-truncate length under the arena lock.
            if self.stack_obj_len.load(Relaxed) != f.stack_obj_base
                || self.stack_arr_len.load(Relaxed) != f.stack_arr_base
            {
                let mut a = self.stack_arena.lock();
                a.truncate(f.stack_obj_base, f.stack_arr_base);
                let (o, r) = a.bases();
                self.stack_obj_len.store(o, Relaxed);
                self.stack_arr_len.store(r, Relaxed);
            }
            if self.struct_len.load(Relaxed) != f.struct_base {
                let mut a = self.struct_arena.lock();
                a.truncate(f.struct_base);
                self.struct_len.store(a.base(), Relaxed);
            }
            if self.transient_len.load(Relaxed) != f.transient_base {
                let mut a = self.transient_arena.lock();
                a.truncate(f.transient_base);
                self.transient_len.store(a.base(), Relaxed);
            }
        }
    }

    /// add-escape-analysis-stack-alloc: allocate a fresh monotonic frame id
    /// (stamped onto each interp `Frame` at entry; keys stack-arena slots).
    pub(crate) fn next_frame_id(&self) -> u32 {
        self.next_frame_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    /// Update the *top* (currently executing) frame's source position.
    /// Called by callers right before they invoke a callee, so the snapshot
    /// at a downstream `throw` shows the call site, not 0.
    ///
    /// `column = 0` means unknown — the snapshot formats as `(file:line)`
    /// rather than `(file:line:col)`.
    /// add-offline-symbolication: `offset` = the frame's linearized code offset
    /// (`Function::linear_offset`), stamped in the **same** lock as line/column
    /// so a stripped-release trace (no line info) can print `+0x<offset>` — an
    /// offline-resolvable key for `z42d symbolicate` — at zero extra locking
    /// cost. Pass `u32::MAX` when a caller has no offset to record.
    pub(crate) fn update_top_frame_pos(&self, line: u32, column: u32, offset: u32) {
        if let Some(top) = self.call_stack.lock().last() {
            top.line.set(line);
            top.column.set(column);
            top.offset.set(offset);
        }
    }

    /// Snapshot the entire call stack for stack-trace formatting at a
    /// `throw` site. Cheap clone (small-string + u32 per frame); only
    /// invoked on the throw path so per-instruction overhead is zero.
    pub(crate) fn snapshot_call_stack(&self) -> Vec<crate::exception::FrameSnapshot> {
        self.call_stack.lock().iter().map(|f| f.snapshot()).collect()
    }

    /// Current depth of the call stack — debugging / tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn call_stack_depth(&self) -> usize {
        self.call_stack.lock().len()
    }

    /// Spec impl-ref-out-in-runtime (Decision R1): index into the frame
    /// chain and return a raw pointer to that frame's `regs` Vec.
    /// Used by `Value::Ref { kind: RefKind::Stack { frame_idx, .. } }`
    /// transparent deref in `Frame::get/set`.
    ///
    /// # Safety
    /// Caller must:
    ///   1. Use the returned pointer only while the corresponding frame is
    ///      still alive (guaranteed by spec design Decision 9: refs never
    ///      escape the call stack — popped frames cannot be referenced).
    ///   2. Not race with concurrent push/pop on the same VmContext (single
    ///      RefCell borrow boundary; deref is synchronous within a frame).
    pub(crate) fn frame_state_at(&self, idx: usize) -> Option<*const Vec<Value>> {
        let stack = self.call_stack.lock();
        stack.get(idx).map(|f| f.regs)
    }

    /// Current depth of the frame chain. `frame_state_at(depth - 1)` is
    /// the most recent frame. Used by codegen-generated `LoadLocalAddr`
    /// to produce a `RefKind::Stack { frame_idx }` referencing the
    /// current frame at emission time.
    pub(crate) fn frame_stack_depth(&self) -> usize {
        self.call_stack.lock().len()
    }
}
