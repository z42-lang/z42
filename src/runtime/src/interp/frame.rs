//! Frame（寄存器文件 + 池化）与 ref 解引用 / 写回、行号解析。refactor-split-interp-mod（2026-09-03）：自 1155 行的 `interp/mod.rs` 逐行搬出，
//! mod.rs 只留模块表与执行主循环 `exec_function_body`；本模块经 mod.rs 的 `pub(crate) use` 全量再导出，
//! 兄弟模块的 `super::X` 路径不变。

#![allow(unused_imports)]
use super::*;
use crate::metadata::{BranchTargets, Function, Module, Terminator, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;

pub(crate) struct Frame {
    pub regs: Vec<Value>,
    /// Spec impl-ref-out-in-runtime (Decision R2 architecture E):
    /// `(param_reg, original_ref_kind)` pairs. When the function was called
    /// with a `ref`/`out`/`in` argument, the entry path deref'd the Ref
    /// (storing the underlying value into `regs[param_reg]` so all
    /// instruction handlers see a normal value) and stashed the original
    /// `RefKind` here. At function exit, every entry's final `regs[param_reg]`
    /// value is stored back through the corresponding Ref to the caller's
    /// lvalue. Net semantic: caller sees the post-call value of its
    /// `ref`/`out` lvalue, identical to true cross-frame refs but without
    /// requiring 80+ instruction handlers to be deref-aware.
    pub ref_writebacks: Vec<(u32, crate::metadata::types::RefKind)>,
    /// add-osr-loop-tiering: loop back-edges taken in THIS activation. When it
    /// reaches `JitModuleCtx.osr_threshold` (jit mode only), the running loop is
    /// hot enough to hand off to native code (OSR). Per-activation (reset each
    /// `Frame::new*`) so "called once, loops a lot" upgrades while "called a lot,
    /// loops a little" does not (that's the call-count path's job).
    pub back_edge_count: u32,
    /// This activation's id, taken lazily (see [`Frame::frame_id`]); `0` until
    /// then. `ObjNew`/`ArrayNew` with `stack_alloc`, `Ref`s, `PinnedView`s
    /// and struct values tag their arena slots with it, and the
    /// handle carries it, so a stale access (after this frame truncated the
    /// arena) is caught by the id mismatch. Most frames never allocate there and
    /// never take one.
    id: std::cell::Cell<u32>,
    /// add-generic-methods: resolved concrete FQ type-argument names for the
    /// generic method call that created this frame (from `CallInsn::method_type_args`).
    /// Empty for non-generic calls. Read by `MethodTypeArg` / `MethodDefault` in the
    /// body to materialize `typeof(T)` / `new T()` / `default(T)`. Mirrors the
    /// class-level `Object.type_args` carrier, but on the frame (static methods have
    /// no `this`). Set after construction in `exec_call` from the call instruction.
    pub method_type_args: Box<[String]>,
}

impl Frame {
    /// A frame whose first registers are `args`. The register file comes from
    /// the context's register pool (`VmContext::reg_pool`, all `Null`); the
    /// frame's [`FrameGuard`] hands it back after popping the frame. A `Frame`
    /// dropped without ever being entered just frees it.
    pub fn new(ctx: &VmContext, args: &[Value], max_reg: u32) -> Self {
        let size = if max_reg > 0 { max_reg as usize } else { args.len() };
        let need = size.max(args.len());
        let mut regs = ctx.reg_pool.take(need);
        for (i, v) in args.iter().enumerate() {
            regs[i] = v.clone();
        }
        Frame {
            regs,
            ref_writebacks: Vec::new(),
            back_edge_count: 0,
            id: std::cell::Cell::new(0),
            method_type_args: Box::default(),
        }
    }

    /// perf-vm-iteration Phase 1 (Decision 3): build a frame filling the
    /// register file directly from `caller_regs[arg_indices[i]]` — one clone per
    /// arg, no intermediate args `Vec`. Same pooling as `new`. Returns an error
    /// (not a panic) on an out-of-range register index, matching `collect_args`.
    pub fn new_from_regs(
        ctx: &VmContext, caller_regs: &[Value], arg_indices: &[u32], max_reg: u32,
    ) -> Result<Self> {
        let argc = arg_indices.len();
        let size = if max_reg > 0 { max_reg as usize } else { argc };
        let need = size.max(argc);
        let mut regs = ctx.reg_pool.take(need);
        for (i, &r) in arg_indices.iter().enumerate() {
            let v = caller_regs.get(r as usize)
                .ok_or_else(|| anyhow::anyhow!("undefined register %{r}"))?;
            regs[i] = v.clone();
        }
        Ok(Frame {
            regs,
            ref_writebacks: Vec::new(),
            back_edge_count: 0,
            id: std::cell::Cell::new(0),
            method_type_args: Box::default(),
        })
    }

    /// perf-vm-iteration Phase 1 (Decision 3): like `new_from_regs` but with a
    /// prepended receiver (`this`) in slot 0 — for the virtual-call hot path
    /// (`exec_vcall`), which passes `regs[0] = receiver`, `regs[1+i] = args[i]`.
    /// Eliminates the vcall path's `vec![receiver]` + `collect_args` Vecs and the
    /// arg double-clone; receiver + each arg cloned exactly once.
    pub fn new_from_receiver_regs(
        ctx: &VmContext, receiver: &Value, caller_regs: &[Value], arg_indices: &[u32], max_reg: u32,
    ) -> Result<Self> {
        let argc = arg_indices.len();
        let total = argc + 1; // + receiver in slot 0
        let size = if max_reg > 0 { max_reg as usize } else { total };
        let need = size.max(total);
        let mut regs = ctx.reg_pool.take(need);
        regs[0] = receiver.clone();
        for (i, &r) in arg_indices.iter().enumerate() {
            let v = caller_regs.get(r as usize)
                .ok_or_else(|| anyhow::anyhow!("undefined register %{r}"))?;
            regs[i + 1] = v.clone();
        }
        Ok(Frame {
            regs,
            ref_writebacks: Vec::new(),
            back_edge_count: 0,
            id: std::cell::Cell::new(0),
            method_type_args: Box::default(),
        })
    }

    /// This frame's id, taken from `ctx` the first time it is asked for. Ask
    /// only when tagging a new arena slot / handle with it.
    #[inline]
    pub fn frame_id(&self, ctx: &VmContext) -> u32 {
        let id = self.id.get();
        if id != 0 {
            return id;
        }
        let id = ctx.next_frame_id();
        self.id.set(id);
        id
    }

    /// The id as it stands, `0` if none was taken yet (OSR hands it on as is).
    #[inline]
    pub fn frame_id_if_taken(&self) -> u32 {
        self.id.get()
    }

    /// Set a register's raw value (no deref). For ref-aware store-through
    /// (transparently writing through `Value::Ref` to the underlying
    /// caller slot / array elem / object field), use `set_thru_ref`
    /// (spec impl-ref-out-in-runtime).
    /// Write a register. The frame is pre-sized to `max_reg` at construction, so
    /// the in-bounds path is overwhelmingly hot — keep it inlinable into the exec
    /// loop (this is one of the two hottest interp functions, per profiling). The
    /// grow path (a reg beyond the pre-sized file, e.g. hand-built test functions)
    /// is `#[cold]` + out-of-line so it doesn't bloat the hot path.
    #[inline]
    pub fn set(&mut self, reg: u32, val: Value) {
        let idx = reg as usize;
        if idx < self.regs.len() {
            self.regs[idx] = val;
        } else {
            self.set_grow(idx, val);
        }
    }

    #[cold]
    #[inline(never)]
    fn set_grow(&mut self, idx: usize, val: Value) {
        self.regs.resize(idx + 1, Value::Null);
        self.regs[idx] = val;
    }

    /// Get a register's raw value (no deref). For ref-aware read-through
    /// (transparently dereferencing `Value::Ref`), use `get_deref`
    /// (spec impl-ref-out-in-runtime).
    #[inline(always)]
    pub fn get(&self, reg: u32) -> Result<&Value> {
        let idx = reg as usize;
        if idx < self.regs.len() {
            Ok(&self.regs[idx])
        } else {
            anyhow::bail!("undefined register %{reg}")
        }
    }

    /// Spec impl-ref-out-in-runtime (Decision R2): read a register's value
    /// with transparent deref. If the register holds a `Value::Ref`, the
    /// underlying value (from caller frame / array elem / object field) is
    /// returned. Otherwise behaves like `get` but returns owned `Value`.
    /// Use this in instruction handlers that read user-visible values.
    #[allow(dead_code)]
    pub fn get_deref(&self, reg: u32, ctx: &VmContext) -> Result<Value> {
        match self.get(reg)? {
            // make-value-copy: resolve the Ref handle → RefKind via the transient arena,
            // release the arena lock (clone) before deref touches heap / other locks.
            Value::Ref { idx, frame_id } => {
                let kind = ctx.transient_arena.lock().ref_kind(*idx, *frame_id)?;
                deref_ref(&kind, ctx)
            }
            other => Ok(other.clone()),
        }
    }

    /// Spec impl-ref-out-in-runtime (Decision R2): write a register with
    /// transparent store-through. If the register currently holds a
    /// `Value::Ref`, the write is forwarded to the underlying location and
    /// the Ref itself is preserved so subsequent reads/writes still
    /// indirect. Otherwise the register is overwritten.
    #[allow(dead_code)]
    pub fn set_thru_ref(&mut self, reg: u32, val: Value, ctx: &VmContext) -> Result<()> {
        let idx = reg as usize;
        if idx >= self.regs.len() {
            self.regs.resize(idx + 1, Value::Null);
        }
        let kind_to_store = match &self.regs[idx] {
            // make-value-copy: resolve the Ref handle → RefKind via the transient arena.
            Value::Ref { idx: ai, frame_id } => Some(ctx.transient_arena.lock().ref_kind(*ai, *frame_id)?),
            _ => None,
        };
        match kind_to_store {
            Some(kind) => store_thru_ref(&kind, val, ctx),
            None => { self.regs[idx] = val; Ok(()) }
        }
    }
}

/// Spec impl-ref-out-in-runtime: dereference a `Value::Ref { kind }` into
/// the underlying value. Stack kind looks up `ctx.frame_state_at(frame_idx)`
/// (raw pointer, safe under design Decision 9: refs don't escape call
/// stack). Array/Field kinds borrow the held GcRef and read.
pub(crate) fn deref_ref(
    kind: &crate::metadata::types::RefKind, ctx: &VmContext,
) -> Result<Value> {
    use crate::metadata::types::RefKind;
    match kind {
        RefKind::Stack { frame_idx, slot } => {
            let regs_ptr = ctx.frame_state_at(*frame_idx as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref points to popped frame (frame_idx={frame_idx})"))?;
            // SAFETY: spec Decision 9 — refs never escape the call stack;
            // when this deref runs, the target frame is still active.
            let regs = unsafe { &*regs_ptr };
            let v = regs.get(*slot as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref target slot %{slot} out of frame range"))?;
            // Sanity guard: ref-to-ref nesting not supported (codegen never
            // produces it; defend in case of malformed bytecode).
            if let Value::Ref { .. } = v {
                anyhow::bail!("ref-to-ref nesting not supported");
            }
            Ok(v.clone())
        }
        RefKind::Array { gc_ref, idx } => {
            let arr = gc_ref.borrow();
            arr.get(*idx)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref array index {idx} out of bounds (len={})", arr.len()))
        }
        RefKind::Field { gc_ref, field_name } => {
            let obj = gc_ref.borrow();
            let slot = *obj.type_desc.field_index.get(field_name)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref field `{field_name}` not found on type `{}`",
                    obj.type_desc.name))?;
            Ok(obj.field_value(slot))
        }
    }
}

/// Spec impl-ref-out-in-runtime: store a value through a `Value::Ref` to
/// the underlying location. Mirror of `deref_ref` for the write path.
pub(crate) fn store_thru_ref(
    kind: &crate::metadata::types::RefKind, val: Value, ctx: &VmContext,
) -> Result<()> {
    use crate::metadata::types::RefKind;
    match kind {
        RefKind::Stack { frame_idx, slot } => {
            let regs_ptr = ctx.frame_state_at(*frame_idx as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref points to popped frame (frame_idx={frame_idx})"))?;
            // SAFETY: same as deref_ref; the frame is still active.
            // We need *mut here — cast from *const Vec<Value>. The frame's
            // regs Vec is borrowed from `Frame` which is `&mut` during exec
            // of its instructions, so the Vec is uniquely owned by that
            // frame. Cross-frame mutation requires us to coerce to mut.
            let regs = unsafe { &mut *(regs_ptr as *mut Vec<Value>) };
            let slot_idx = *slot as usize;
            if slot_idx >= regs.len() {
                regs.resize(slot_idx + 1, Value::Null);
            }
            regs[slot_idx] = val;
            Ok(())
        }
        RefKind::Array { gc_ref, idx } => {
            {
                let mut arr = gc_ref.borrow_mut();
                if *idx >= arr.len() {
                    anyhow::bail!(
                        "ref array index {idx} out of bounds (len={})", arr.len());
                }
                arr.set_boxed(*idx, val.clone());
            }
            // fix-missing-array-write-barriers (2026-09-10): writing through a `ref` to an
            // array element is an array store like any other — `ArraySet` fires the barrier,
            // this path never did.
            if val.is_heap_ref() {
                ctx.heap().write_barrier_array_elem(
                    &Value::Array(gc_ref.clone()), *idx, &val,
                );
            }
            Ok(())
        }
        RefKind::Field { gc_ref, field_name } => {
            let mut obj = gc_ref.borrow_mut();
            let slot_opt = obj.type_desc.field_index.get(field_name).copied();
            match slot_opt {
                // unify-object-byte-layout (PR-2): encode into bytes / refs.
                //
                // fix-ref-field-write-barrier: writing through a `ref` to an object field is a
                // field store like any other — `FieldSet` fires the barrier (exec_object.rs),
                // this path never did. Latent until z42c starts emitting `LoadFieldAddr`
                // (0xA2); with generational GC on by default, storing a young ref into an old
                // object without recording it in the remembered set collects it prematurely.
                // Mirrors the `RefKind::Array` arm above and the three `FieldSet` call sites:
                // filter at the call site, fire post-write, release the borrow first.
                Some(slot) => {
                    let wrote_ref = obj.set_field_value(slot, &val);
                    drop(obj);
                    if wrote_ref && val.is_heap_ref() {
                        ctx.heap().write_barrier_field(
                            &Value::Object(gc_ref.clone()), slot, &val,
                        );
                    }
                    Ok(())
                }
                None => anyhow::bail!(
                    "ref field `{field_name}` not found on type `{}`",
                    obj.type_desc.name),
            }
        }
    }
}

// ── Debug: source line resolution ─────────────────────────────────────────────

/// Resolve `(line, column)` covering a given (block, instr) site by walking
/// the function's line table forward to the latest entry that doesn't
/// overshoot. Column is 0 when the source position predates zbc 1.1 or the
/// emitter didn't capture it (gracefully degraded by `format_stack_trace`
/// — `(file:line)` instead of `(file:line:col)`).
pub(crate) fn resolve_line(table: &[crate::metadata::bytecode::LineEntry], block: u32, instr: u32) -> (u32, u32) {
    // S2a (perf-interp-hot-paths): the line table is emitted in non-decreasing
    // (block, instr) order — the previous linear forward-scan already relied on
    // that (it `break`s on the first overshoot). Binary-search the last entry
    // whose (block, instr) <= the target site: O(log n) instead of O(n) per
    // Call / VCall / throw. Behaviour is byte-identical to the linear scan for a
    // sorted table (which is the only shape the emitter produces).
    let hi = table.partition_point(|e| (e.block, e.instr) <= (block, instr));
    match hi.checked_sub(1).and_then(|i| table.get(i)) {
        Some(e) => (e.line, e.column),
        None    => (0, 0),
    }
}

// ── Core execution loop ──────────────────────────────────────────────────────

#[cfg(test)]
#[path = "frame_tests.rs"]
mod frame_tests;
