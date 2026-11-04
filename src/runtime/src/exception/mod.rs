//! Exception runtime — z42 exception object layout, propagation model,
//! and stack-trace capture.
//!
//! ## Propagation model
//!
//! - **interp**: exceptions flow as `ExecOutcome::Thrown(Value)`; no
//!   thread_local intermediary.
//! - **JIT**: in-flight exception lives in `VmContext.exec.pending_exception`,
//!   reached by a JIT helper through its context parameter (the thread's `VmContext`).
//!
//! All previously-existing thread_local exception slots
//! (`interp::PENDING_EXCEPTION`, `jit::helpers::PENDING_EXCEPTION`) and
//! the `UserException` sentinel + `user_throw` / `user_exception_take` /
//! `sync_in_from_ctx` / `sync_out_to_ctx` bridges are gone (review2 §5.5
//! closed; review2 §3 fully tackled).
//!
//! ## Stack-trace capture (exception-stack-trace)
//!
//! `VmContext.exec.call_stack` holds one [`crate::vm::exec::call::frames::VmFrame`] per active script frame,
//! pushed by `interp::exec_support::enter_frame` / `jit::invoke::enter_compiled`.
//! Caller frames record the code offset of the call site before they invoke a
//! callee, so a snapshot at throw time produces a complete
//! `<func> at <file>:<line>` chain (line resolved from the offset then).
//!
//! When a thrown value is an instance of `Std.Exception` (or subclass) and
//! its `StackTrace` field is `Value::Null`, the throw site populates the
//! field with the formatted trace. Re-thrown exceptions keep their
//! original trace (the null-check is the deduplication mechanism).

use anyhow::{anyhow, Result};

use crate::object::{NativeData, TypeDesc};
use crate::vm::runtime::Module;
use crate::object::{default_value_for, Value};
use crate::vm_context::VmContext;
use crate::vm::exec::stack::{FrameSnapshot, format_stack_trace};

/// fix-silent-symbol-resolution：构造 `Std.MissingSymbolException`。
///
/// 必须是**类型化**异常：裸 `Value::Str` 只能被无类型 `catch {}` 捕获，永远匹配不上
/// `catch (MissingSymbolException e)` 甚至 `catch (Exception e)`。逐级回落保证 stdlib
/// 缺任一类时仍把错误传出去，而不是静默吞掉。
pub fn make_missing_symbol_exception(
    ctx: &crate::vm_context::VmContext,
    module: &crate::vm::runtime::Module,
    msg: String,
) -> crate::object::Value {
    // add-sdk-libs D6：`${Z42_HOME}` 条目解析不到时附「是否没装 SDK」提示（无此类条目 ⇒ 原样）。
    let msg = crate::vm::loader::locate::probing::with_sdk_missing_hint(msg);
    if let Ok(e) = make_stdlib_exception(ctx, module, "Std.MissingSymbolException", msg.clone()) {
        return e;
    }
    if let Ok(e) = make_stdlib_exception(ctx, module, "Std.Exception", msg.clone()) {
        return e;
    }
    crate::object::Value::Str(msg.into())
}


/// Decide whether `desc` is `Std.Exception` or a subclass thereof. Used to
/// gate StackTrace / Message access — only Exception-derived classes have
/// the fields. Goes through the shared type test (`isa_td`), so a base class
/// that lives in a lazily loaded package is found too.
pub fn is_exception_subclass(desc: &TypeDesc, ctx: &VmContext, module: &Module) -> bool {
    static KEY: crate::vm::runtime::ids::tokens::TypeKeyCell = crate::vm::runtime::ids::tokens::TypeKeyCell::new();
    crate::vm::exec::ops::cast::isa_td(ctx, &module.type_registry, desc, "Std.Exception", &KEY)
}

/// Populate `value.StackTrace` with a snapshot of the current call stack
/// **iff** `value` is an instance of `Std.Exception` (or subclass) and
/// the field is currently `Value::Null`. Re-thrown exceptions keep their
/// original trace because the field is non-null after the first populate.
///
/// Phase 1: throws of non-Exception values (`throw "raw string"`) are a
/// no-op — they have no `StackTrace` field to populate.
pub fn populate_stack_trace(value: &Value, ctx: &VmContext, module: &Module) {
    populate_stack_trace_with(value, ctx, module, &[]);
}

/// [`populate_stack_trace`] with `extra` appended as the innermost rows — the
/// `[Native]` wrapper rows of a builtin call that skipped the wrapper's activation
/// (`vm::exec::call::forward`), so the trace reads as if the wrapper had run.
pub fn populate_stack_trace_with(value: &Value, ctx: &VmContext, module: &Module, extra: &[FrameSnapshot]) {
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
    let mut frames = ctx.snapshot_call_stack();
    frames.extend_from_slice(extra);
    let trace  = format_stack_trace(&frames);

    // Step 3: write the field — through objops, so the write barrier fires. The exception is
    // usually old news by the time it is thrown (built, stored, promoted, thrown later), and the
    // trace string is brand new: an old→young store. Writing it with a bare `set_field_value`
    // left the card clean, so the next minor swept the trace while the exception still held it.
    if matches!(rc.borrow().field_value(slot), Value::Null) {
        let trace = Value::Str(trace.into());
        // A `str` field always takes a `Str`; the only rejection `store_slot` has is a primitive
        // slot type mismatch, which cannot happen here.
        let _ = crate::objops::field::store_slot(ctx, rc, slot, &trace);
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
        None      => format!("uncaught exception: {}", crate::object::value::format::value_to_str(value)),
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
/// retire-z-codes: introduced so Rust-side throw sites (marshal
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
    make_stdlib_exception_with(ctx, module, type_fq, message, &[])
}

/// [`make_stdlib_exception`] plus public fields besides `Message` (a package exception's
/// own data, `ProcessTimeoutException.TimeoutMs`). A field the class does not declare is
/// skipped — the caller may be building a fallback base class.
pub fn make_stdlib_exception_with(
    ctx: &VmContext, module: &Module, type_fq: &str, message: String, fields: &[(&str, Value)],
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
    for (name, v) in fields {
        if let Some(slot) = type_desc.field_index.get(*name).and_then(|&i| slots.get_mut(i)) {
            *slot = *v;
        }
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
