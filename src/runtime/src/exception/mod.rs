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
//! `VmContext.call_stack` holds one [`FrameInfo`] per active script frame,
//! pushed by `interp::exec_function` (paired with `frame_states` via the
//! existing `FrameGuard`). Caller frames record the line of the call site
//! before they invoke a callee, so a snapshot at throw time produces a
//! complete `<func> at <file>:<line>` chain.
//!
//! When a thrown value is an instance of `Std.Exception` (or subclass) and
//! its `StackTrace` field is `Value::Null`, the throw site populates the
//! field with the formatted trace. Re-thrown exceptions keep their
//! original trace (the null-check is the deduplication mechanism).

use std::cell::Cell;

use anyhow::{anyhow, Result};

use crate::metadata::types::{NativeData, TypeDesc};
use crate::metadata::{default_value_for, Module, Value};
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
/// # Safety / lifetime
///
/// `regs` / `env_arena` are raw pointers into a `JitFrame` or interp
/// `Frame` that lives on the Rust call stack. They are valid for the
/// duration of the corresponding `exec_function` / `JitModule::run_fn`
/// activation — RAII (`FrameGuard` for interp, explicit pair in JIT
/// helpers) guarantees the pop runs before the owning frame's stack slot
/// goes away. GC scans the call_stack only while a z42 frame is live
/// (collect is invoked from inside script code), so all pointers it
/// sees are still in-bounds.
///
/// `line` / `column` are mutable via [`Cell`] so callers can stamp the
/// current call-site position just before invoking a callee, without
/// re-borrowing the surrounding `RefCell<Vec<VmFrame>>`. `column`
/// (zbc 1.1+) is 1-based; value 0 means unknown — `format_stack_trace`
/// then degrades to `(file:line)`.
#[derive(Debug)]
pub struct VmFrame {
    // `Arc<str>` (not `String`): JIT `FnEntry` already holds `Arc<str>` name +
    // file, so per-call `push_frame` clones the Arc (O(1) atomic refcount) on
    // the hot path instead of allocating + copying a fresh `String` every call
    // (perf-jit-frame-strings, 2026-06-20 — `jit_vcall` was the #1 hotspot).
    pub func_name: std::sync::Arc<str>,
    pub file:      std::sync::Arc<str>,
    pub line:      Cell<u32>,
    pub column:    Cell<u32>,
    /// add-offline-symbolication: linearized code offset of the frame's current
    /// site (see `Function::linear_offset`). Stamped alongside `line`/`column`
    /// by `update_caller_line` / the throw path. `u32::MAX` = unset. Used by
    /// `format_stack_trace` to emit `+0x<offset>` for stripped frames (no line
    /// info) so a captured trace carries an offline-resolvable key.
    pub offset:    Cell<u32>,
    /// Pointer to the frame's register file. The Vec content is the
    /// canonical place where this frame's z42 values live.
    pub regs:      *const Vec<Value>,
    /// Pointer to the frame's stack-closure env arena (or null when the
    /// frame does not host any stack closures).
    pub env_arena: *const Vec<Vec<Value>>,
    /// add-escape-analysis-stack-alloc: the per-context stack-arena lengths when
    /// this frame was pushed. `pop_frame` truncates the arena back to these,
    /// bulk-freeing this frame's stack-allocated objects/arrays (LIFO). Stamped
    /// by `push_frame` (not the `new()` call sites) → all frame kinds get it for
    /// free; JIT frames never stack-allocate so their truncate is a no-op.
    pub stack_obj_base: usize,
    pub stack_arr_base: usize,
    /// add-struct-value-semantics: value-struct byte-arena length when this frame
    /// was pushed; `pop_frame` truncates back to it (LIFO-frees this frame's blobs).
    /// Stamped by `push_frame`.
    pub struct_base: usize,
}

// SAFETY (add-multithreading-foundation Phase 3, 2026-05-20):
// `VmFrame` holds raw pointers (`regs` / `env_arena`) into the owning
// interp / JIT frame's Rust stack. These are valid for the frame's
// lifetime, which is enclosed by `FrameGuard` RAII. The GC scanner is
// the only cross-thread reader (mark phase invoked from a possible GC
// worker thread); it only ever reads these pointers while the owning
// thread is paused at a safepoint (future invariant; today GC only
// runs from the same thread that owns the frame). The `Cell<u32>`
// `line` / `column` are wrapped in this single-thread invariant.
// Once `add-vmcontext-registry` lands per-thread VmContexts with proper
// safepoints, this invariant is enforced by the safepoint protocol.
unsafe impl Send for VmFrame {}
unsafe impl Sync for VmFrame {}

impl VmFrame {
    pub fn new(
        func_name: std::sync::Arc<str>, file: std::sync::Arc<str>,
        regs: *const Vec<Value>, env_arena: *const Vec<Vec<Value>>,
    ) -> Self {
        Self {
            func_name, file,
            line: Cell::new(0), column: Cell::new(0),
            offset: Cell::new(u32::MAX),
            regs, env_arena,
            // add-escape-analysis-stack-alloc: overwritten by push_frame.
            stack_obj_base: 0, stack_arr_base: 0,
            struct_base: 0,   // add-struct-value-semantics: overwritten by push_frame.
        }
    }

    /// Snapshot used at throw time (no Cell — values are frozen). Strips
    /// the raw pointers — snapshots are not GC-root-scanner targets.
    pub fn snapshot(&self) -> FrameSnapshot {
        FrameSnapshot {
            func_name: self.func_name.clone(),
            file:      self.file.clone(),
            line:      self.line.get(),
            column:    self.column.get(),
            offset:    self.offset.get(),
        }
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

/// Walk the runtime base-class chain and decide whether `desc` is
/// `Std.Exception` or a subclass thereof. Used to gate StackTrace
/// population — only Exception-derived classes have the field.
pub fn is_exception_subclass(desc: &TypeDesc, module: &Module) -> bool {
    if desc.name == "Std.Exception" { return true; }
    let mut cur = desc.base_name.as_deref();
    while let Some(name) = cur {
        if name == "Std.Exception" { return true; }
        match module.type_registry.get(name) {
            Some(parent) => cur = parent.base_name.as_deref(),
            None => return false,
        }
    }
    false
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
        let is_exc = is_exception_subclass(&borrowed.type_desc, module);
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
pub fn read_stack_trace(value: &Value, module: &Module) -> Option<String> {
    let rc = match value {
        Value::Object(rc) => rc,
        _ => return None,
    };
    let borrowed = rc.borrow();
    if !is_exception_subclass(&borrowed.type_desc, module) { return None; }
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
pub fn format_uncaught(value: &Value, module: &Module) -> String {
    let header = match read_message(value, module) {
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
    match read_stack_trace(value, module) {
        Some(trace) => format!("{header}\n{trace}"),
        None        => header,
    }
}

/// Read `Std.Exception.Message` from a thrown value, falling back to a
/// generic representation if the value isn't an Exception subclass.
pub fn read_message(value: &Value, module: &Module) -> Option<String> {
    let rc = match value {
        Value::Object(rc) => rc,
        _ => return None,
    };
    let borrowed = rc.borrow();
    if !is_exception_subclass(&borrowed.type_desc, module) { return None; }
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

#[cfg(test)]
mod tests;
