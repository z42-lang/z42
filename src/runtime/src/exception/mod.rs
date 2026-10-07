//! Exception runtime — z42 exception object layout, propagation model,
//! and stack-trace capture.
//!
//! ## Propagation model
//!
//! - **interp**: exceptions flow as `ExecOutcome::Thrown(Value)`; no
//!   thread_local intermediary.
//! - **JIT**: in-flight exception lives in `VmContext::pending_exception`,
//!   reached from helper extern "C" via `(*jit_ctx).vm_ctx`.
//!
//! All previously-existing thread_local exception slots
//! (`interp::PENDING_EXCEPTION`, `jit::helpers::PENDING_EXCEPTION`) and
//! the `UserException` sentinel + `user_throw` / `user_exception_take` /
//! `sync_in_from_ctx` / `sync_out_to_ctx` bridges are gone (review2 §5.5
//! closed; review2 §3 fully tackled).
//!
//! ## Stack-trace capture (2026-05-10 exception-stack-trace)
//!
//! `VmContext.call_stack` holds one [`VmFrame`] per active script frame,
//! pushed by `interp::exec_support::enter_frame` / `jit::invoke::call_native`.
//! Caller frames record the code offset of the call site before they invoke a
//! callee, so a snapshot at throw time produces a complete
//! `<func> at <file>:<line>` chain (line resolved from the offset then).
//!
//! When a thrown value is an instance of `Std.Exception` (or subclass) and
//! its `StackTrace` field is `Value::Null`, the throw site populates the
//! field with the formatted trace. Re-thrown exceptions keep their
//! original trace (the null-check is the deduplication mechanism).

use std::cell::Cell;

use anyhow::{anyhow, Result};

use crate::metadata::types::{NativeData, TypeDesc};
use crate::metadata::{default_value_for, Function, Module, Value};
use crate::vm_context::VmContext;

/// One unified per-frame entry — single source of truth for
/// (a) GC root scanning, (b) stack-trace formatting, and (c) interp
/// `RefKind::Stack` cross-frame deref.
///
/// 2026-05-10 unify-frame-chain replaces three previously-parallel
/// vectors (`exec_stack`, `env_arena_stack`, `call_stack`) with a single
/// `Vec<VmFrame>`. Push and pop happen in lockstep — no caller can
/// "forget half" and leak a partial frame.
///
/// The frame is kept thin (40 B, no refcounted fields) because one is pushed
/// per call: it records only *which* function runs and *where* it is (`pc`).
/// The display name, file and line/column are derived from `func` when a
/// stack trace is built ([`VmFrame::snapshot`]), never on the call path.
///
/// # Safety / lifetime
///
/// `func` / `regs` are raw pointers. `regs` points into a `JitFrame` or
/// interp `Frame` on the Rust call stack; `func`
/// points at the executing `Function`, which the caller (interp) or the
/// `JitModuleCtx` (JIT, merged module or a lazily-loaded function kept
/// alive by its lazy slot) holds for at least the activation. All are valid
/// for the duration of the corresponding `exec_function` / native call —
/// RAII (`FrameGuard` for interp, `jit::invoke::call_native` for JIT)
/// guarantees the pop runs before the owning frame's stack slot goes away.
///
/// `pc` is mutable via [`Cell`] so callers can stamp the current call-site
/// position just before invoking a callee, without re-borrowing the
/// surrounding `Vec<VmFrame>`.
#[derive(Debug)]
pub struct VmFrame {
    /// The executing function — source of the frame's name, file and line table.
    pub func:      *const Function,
    /// Pointer to the frame's register file. The Vec content is the
    /// canonical place where this frame's z42 values live.
    pub regs:      *const Vec<Value>,
    /// add-offline-symbolication: packed code offset of the frame's current
    /// site, `Function::linear_offset(block, instr)` = `block << 16 | instr`.
    /// Stamped by the Call / VCall / CallIndirect sites and the throw path
    /// (both backends). [`PC_UNSET`] = never stamped. Line/column are resolved
    /// from it at snapshot time; a stripped frame (no line info) prints it as
    /// `+0x<offset>` — the offline-resolvable key for `z42d symbolicate`.
    pub pc:        Cell<u32>,
    /// add-escape-analysis-stack-alloc: the per-context stack-arena lengths when
    /// this frame was pushed. `pop_frame` truncates the arena back to these,
    /// bulk-freeing this frame's stack-allocated objects/arrays (LIFO). Stamped
    /// by `push_frame` (not the `new()` call sites) → all frame kinds get it for
    /// free; JIT frames never stack-allocate so their truncate is a no-op.
    /// Arena slot indices are `u32` (`stack_alloc_obj` etc.), so a length fits.
    pub stack_obj_base: u32,
    pub stack_arr_base: u32,
    /// add-struct-value-semantics: value-struct byte-arena length when this frame
    /// was pushed; `pop_frame` truncates back to it (LIFO-frees this frame's blobs).
    /// Stamped by `push_frame`.
    pub struct_base: u32,
    /// make-value-copy: transient-arena length when this frame was pushed; `pop_frame`
    /// truncates back to it (LIFO-frees this frame's Ref/PinnedView/StructRefHeap
    /// payloads). Stamped by `push_frame`.
    pub transient_base: u32,
}

/// [`VmFrame::pc`] value of a frame that has not stamped a site yet.
pub const PC_UNSET: u32 = u32::MAX;

// SAFETY (add-multithreading-foundation Phase 3, 2026-05-20):
// `VmFrame` holds raw pointers (`func` / `regs`) valid for the
// frame's lifetime, which is enclosed by `FrameGuard` RAII / `call_native`.
// The GC scanner is the only cross-thread reader (mark phase invoked from a
// possible GC worker thread); it only ever reads these pointers while the
// owning thread is paused at a safepoint. The `Cell<u32>` `pc` is covered by
// the same single-writer invariant.
unsafe impl Send for VmFrame {}
unsafe impl Sync for VmFrame {}

impl VmFrame {
    pub fn new(func: *const Function, regs: *const Vec<Value>) -> Self {
        Self {
            func, regs,
            pc: Cell::new(PC_UNSET),
            // Arena bases are overwritten by push_frame.
            stack_obj_base: 0, stack_arr_base: 0,
            struct_base: 0, transient_base: 0,
        }
    }

    /// The executing function.
    #[inline]
    pub fn func(&self) -> &Function {
        // SAFETY: see the type-level note — `func` outlives the frame.
        unsafe { &*self.func }
    }

    /// Source `(line, column)` of the frame's current site, `(0, 0)` when no
    /// site was stamped or the function carries no line info. No allocation
    /// (the crash-signal handler calls it).
    pub fn line_col(&self) -> (u32, u32) {
        let pc = self.pc.get();
        if pc == PC_UNSET { return (0, 0); }
        crate::interp::resolve_line(self.func().line_table(), pc >> 16, pc & 0xffff)
    }

    /// Snapshot used at throw time — name / file / line / column computed from
    /// `func` + `pc` here. Strips the raw pointers — snapshots are not
    /// GC-root-scanner targets.
    pub fn snapshot(&self) -> FrameSnapshot {
        let (func_name, file) = self.func().frame_name_file();
        let (line, column) = self.line_col();
        FrameSnapshot { func_name, file, line, column, offset: self.pc.get() }
    }
}

/// Backward-compatibility alias. Phase 1 exception-stack-trace introduced
/// `FrameInfo`; unify-frame-chain renamed to `VmFrame` to reflect its
/// broader role (GC roots + closure envs + trace metadata in one row).
pub type FrameInfo = VmFrame;

/// Frozen view of a [`FrameInfo`] suitable for formatting / passing across
/// borrow scopes.
#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub func_name: std::sync::Arc<str>,
    pub file:      std::sync::Arc<str>,
    pub line:      u32,
    pub column:    u32,
    /// add-offline-symbolication: linearized code offset (`u32::MAX` = unset).
    pub offset:    u32,
}

/// Format a captured trace as a multi-line string.
///
/// Frames are presented in caller-to-throw order — the throwing function
/// is **last**, matching .NET / Java convention (most-recent at the bottom).
/// Each line: `  at <func> (<file>:<line>[:<col>])`.
///
/// `column` (zbc 1.1+) appears only when > 0; legacy frames without column
/// gracefully degrade to `(file:line)`. When both file and column are
/// missing the trailing `(...)` is omitted entirely.
/// fix-silent-symbol-resolution：构造 `Std.MissingSymbolException`。
///
/// 必须是**类型化**异常：裸 `Value::Str` 只能被无类型 `catch {}` 捕获，永远匹配不上
/// `catch (MissingSymbolException e)` 甚至 `catch (Exception e)`。逐级回落保证 stdlib
/// 缺任一类时仍把错误传出去，而不是静默吞掉。
pub fn make_missing_symbol_exception(
    ctx: &crate::vm_context::VmContext,
    module: &crate::metadata::Module,
    msg: String,
) -> crate::metadata::Value {
    // add-sdk-libs D6：`${Z42_HOME}` 条目解析不到时附「是否没装 SDK」提示（无此类条目 ⇒ 原样）。
    let msg = crate::probing::with_sdk_missing_hint(msg);
    if let Ok(e) = make_stdlib_exception(ctx, module, "Std.MissingSymbolException", msg.clone()) {
        return e;
    }
    if let Ok(e) = make_stdlib_exception(ctx, module, "Std.Exception", msg.clone()) {
        return e;
    }
    crate::metadata::Value::Str(msg.into())
}

pub fn format_stack_trace(frames: &[FrameSnapshot]) -> String {
    let mut out = String::new();
    for f in frames.iter().rev() {
        out.push_str("  at ");
        out.push_str(&f.func_name);
        if !f.file.is_empty() {
            out.push_str(" (");
            out.push_str(&f.file);
            if f.line > 0 {
                out.push(':');
                out.push_str(&f.line.to_string());
                if f.column > 0 {
                    out.push(':');
                    out.push_str(&f.column.to_string());
                }
            }
            out.push(')');
        } else if f.line > 0 {
            out.push_str(" (line ");
            out.push_str(&f.line.to_string());
            if f.column > 0 {
                out.push_str(", col ");
                out.push_str(&f.column.to_string());
            }
            out.push(')');
        } else if f.offset != u32::MAX {
            // add-offline-symbolication: no line info (release-stripped, no
            // adjacent .zsym merged) — emit the linearized code offset as an
            // offline-resolvable key. `z42d symbolicate <trace> --syms <.zsym>`
            // maps `+0x<offset>` back to file:line:col via the archived sidecar.
            out.push_str(" +0x");
            out.push_str(&format!("{:x}", f.offset));
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Decide whether `desc` is `Std.Exception` or a subclass thereof. Used to
/// gate StackTrace / Message access — only Exception-derived classes have
/// the fields. Goes through the shared type test (`isa_td`), so a base class
/// that lives in a lazily loaded package is found too.
pub fn is_exception_subclass(desc: &TypeDesc, ctx: &VmContext, module: &Module) -> bool {
    crate::interp::dispatch::isa_td(ctx, &module.type_registry, desc, "Std.Exception")
}

/// Populate `value.StackTrace` with a snapshot of the current call stack
/// **iff** `value` is an instance of `Std.Exception` (or subclass) and
/// the field is currently `Value::Null`. Re-thrown exceptions keep their
/// original trace because the field is non-null after the first populate.
///
/// Phase 1: throws of non-Exception values (`throw "raw string"`) are a
/// no-op — they have no `StackTrace` field to populate.
pub fn populate_stack_trace(value: &Value, ctx: &VmContext, module: &Module) {
    let rc = match value {
        Value::Object(rc) => rc,
        _ => return,
    };

    // Step 1: read-only borrow to check shape + decide whether to populate.
    let (is_exc, slot_opt, is_null) = {
        let borrowed = rc.borrow();
        let is_exc = is_exception_subclass(&borrowed.type_desc, ctx, module);
        let slot   = borrowed.type_desc.field_index.get("StackTrace").copied();
        let is_null = match (is_exc, slot) {
            (true, Some(s)) => matches!(borrowed.field_value(s), Value::Null),
            _               => false,
        };
        (is_exc, slot, is_null)
    };
    if !(is_exc && is_null) { return; }
    let slot = match slot_opt { Some(s) => s, None => return };

    // Step 2: snapshot stack outside the borrow (reads call_stack RefCell).
    let frames = ctx.snapshot_call_stack();
    let trace  = format_stack_trace(&frames);

    // Step 3: write-only borrow to set the field.
    let mut bm = rc.borrow_mut();
    if matches!(bm.field_value(slot), Value::Null) {
        bm.set_field_value(slot, &Value::Str(trace.into()));
    }
}

/// Read `Std.Exception.StackTrace` from a thrown value, if present and non-null.
/// Used by uncaught-exception output formatting.
pub fn read_stack_trace(value: &Value, ctx: &VmContext, module: &Module) -> Option<String> {
    let rc = match value {
        Value::Object(rc) => rc,
        _ => return None,
    };
    let borrowed = rc.borrow();
    if !is_exception_subclass(&borrowed.type_desc, ctx, module) { return None; }
    let slot = borrowed.type_desc.field_index.get("StackTrace").copied()?;
    match borrowed.field_value(slot) {
        Value::Str(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    }
}

/// Format an uncaught exception value for top-level display. Combines the
/// thrown value's display form with its `StackTrace` field if it's an
/// Exception subclass that has one populated.
///
/// Used by [`crate::interp::run`] / [`crate::interp::run_returning`] /
/// [`crate::interp::run_with_static_init`] in their `Thrown` arm so all
/// three entry points produce consistent uncaught output.
pub fn format_uncaught(value: &Value, ctx: &VmContext, module: &Module) -> String {
    let header = match read_message(value, ctx, module) {
        // Prefix with the FQ type name: "<FQ_TYPE>: <msg>". Tooling
        // (the test-runner's `[ShouldThrow<E>]` matcher) extracts the
        // thrown type from this line — without the type prefix it would
        // parse the first word of the message instead. `read_message`
        // only returns Some for Exception-subclass Objects, so the
        // Object match below always succeeds on that path; the fallback
        // keeps the bare message for any non-Object edge case.
        Some(msg) => match value {
            Value::Object(rc) =>
                format!("uncaught exception: {}: {}", rc.type_desc().name, msg),
            _ => format!("uncaught exception: {}", msg),
        },
        None      => format!("uncaught exception: {}", crate::corelib::convert::value_to_str(value)),
    };
    match read_stack_trace(value, ctx, module) {
        Some(trace) => format!("{header}\n{trace}"),
        None        => header,
    }
}

/// Read `Std.Exception.Message` from a thrown value, falling back to a
/// generic representation if the value isn't an Exception subclass.
pub fn read_message(value: &Value, ctx: &VmContext, module: &Module) -> Option<String> {
    let rc = match value {
        Value::Object(rc) => rc,
        _ => return None,
    };
    let borrowed = rc.borrow();
    if !is_exception_subclass(&borrowed.type_desc, ctx, module) { return None; }
    let slot = borrowed.type_desc.field_index.get("Message").copied()?;
    match borrowed.field_value(slot) {
        Value::Str(s) => Some(s.to_string()),
        _ => None,
    }
}

/// Construct a stdlib exception instance (e.g. `Std.InvalidMarshalException`)
/// from inside the VM. Returns a `Value::Object` ready to be propagated up
/// through `exec_instr`'s `Ok(Some(value))` channel — the existing throw
/// machinery will then run the local handler lookup + `populate_stack_trace`
/// fill on first throw.
///
/// 2026-05-11 retire-z-codes: introduced so Rust-side throw sites (marshal
/// NUL, PinPtr type mismatch …) can hand the user a typed, catchable
/// exception instead of an anyhow! error string with a `Z####:` prefix.
///
/// The helper sets `Message` directly rather than invoking the z42 ctor —
/// reentering `exec_function` from a marshal context would require a
/// non-trivial extra frame push/pop pair, and every `Std.*Exception` ctor
/// in the stdlib only assigns `this.Message = message` anyway.
pub fn make_stdlib_exception(
    ctx: &VmContext, module: &Module, type_fq: &str, message: String,
) -> Result<Value> {
    let type_desc = module
        .type_registry
        .get(type_fq)
        .cloned()
        .or_else(|| ctx.try_lookup_type(type_fq))
        .ok_or_else(|| anyhow!("stdlib type `{type_fq}` not loaded; cannot construct exception"))?;

    let mut slots: Vec<Value> = type_desc
        .fields
        .iter()
        .map(|f| default_value_for(&f.type_tag))
        .collect();

    let msg_slot = type_desc
        .field_index
        .get("Message")
        .copied()
        .ok_or_else(|| anyhow!("stdlib type `{type_fq}` has no `Message` field"))?;
    if let Some(slot) = slots.get_mut(msg_slot) {
        *slot = Value::Str(message.into());
    }

    Ok(ctx.heap().alloc_object(type_desc, slots, NativeData::None))
}

/// Construct a `Std.OutOfMemoryException` after a strict-mode allocation
/// returned `Value::Null` (heap limit exceeded).
///
/// add-gc-oom-exception: the exception object itself needs to allocate, so
/// strict OOM is toggled off for the duration of the construction and then
/// restored — otherwise building the OOM exception would recursively trip the
/// same limit. Returns the exception `Value` (or `Value::Null` if even the
/// exception type is unavailable), ready to hand up through `exec_instr`'s
/// `Ok(Some(value))` throw channel.
pub fn make_oom_exception(ctx: &VmContext, module: &Module, message: String) -> Value {
    ctx.heap().set_strict_oom(false);
    let exc = make_stdlib_exception(ctx, module, "Std.OutOfMemoryException", message)
        .unwrap_or(Value::Null);
    ctx.heap().set_strict_oom(true);
    exc
}

#[cfg(test)]
pub(crate) mod tests;
