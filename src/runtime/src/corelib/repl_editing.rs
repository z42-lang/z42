//! REPL indent-aware key editing (add-repl-indent-editing).
//!
//! **Policy-free adapter shell.** The decision logic all lives in z42
//! (`Std.Scripting.ReplEditing.KeyEdit`, reached via the registered free function
//! `replKeyEdit`). Here we only, on each controlled key (Backspace / Tab / `}`):
//!   1. re-enter the VM to ask z42 for an "action string", then
//!   2. translate that string into a rustyline `Cmd`.
//! Mirrors the Tab-completion callback (`SetCompleter` / `replComplete`) re-entrancy:
//! the FQN comes from a process-global, the live `&VmContext` from the readline-span
//! thread-local `ACTIVE_CTX` (published by `repl::read_one_line`).
//!
//! Action-string protocol (Rust is a dumb translator — no decisions here):
//!   ""                    → `None` (perform the key's default: Tab→complete, }→insert,
//!                                    Backspace→delete one char)
//!   "kill <n>"            → `Cmd::Kill(BackwardChar(n))`
//!   "insert <text>"       → `Cmd::Insert(1, text)`        (text may contain spaces)
//!   "replace <n> <text>"  → `Cmd::Replace(BackwardChar(n), Some(text))`

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// Registered key-editor FQN (set by z42 via `__repl_set_key_editor`); read by the
/// conditional handlers on each controlled keypress. Process-global (one REPL per
/// process), same pattern as `repl::REGISTERED_COMPLETER`.
static REGISTERED_KEY_EDITOR: std::sync::OnceLock<parking_lot::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

/// `__repl_set_key_editor(fqn: string) -> void` — register the z42 key-editor the
/// indent-aware handlers invoke (signature `string f(string key, string line, int pos)`).
/// Empty string clears it. Not host-gated so it resolves on wasm too; the handlers
/// that read it are host-only and simply never run there.
pub fn builtin_repl_set_key_editor(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fqn = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__repl_set_key_editor: arg 0 must be the key-editor's fully-qualified name (string)"),
    };
    let slot = REGISTERED_KEY_EDITOR.get_or_init(|| parking_lot::Mutex::new(None));
    *slot.lock() = if fqn.is_empty() { None } else { Some(fqn) };
    Ok(Value::Null)
}

// ── Host-only: rustyline key handlers (wasm falls back to plain stdin, no editing) ──

#[cfg(not(target_arch = "wasm32"))]
use crate::interp::{exec_function, ExecOutcome};
#[cfg(not(target_arch = "wasm32"))]
use rustyline::{Cmd, ConditionalEventHandler, Event, EventContext, Movement, RepeatCount};

/// Invoke the z42 key-editor `fqn(key, line, pos)` and return its action string.
/// Mirrors `repl::complete_via_callback` but with (string, string, int) args and a
/// string result. A `throw` inside propagates via `set_pending_thrown`.
#[cfg(not(target_arch = "wasm32"))]
fn key_edit_via_callback(
    ctx: &VmContext,
    fqn: &str,
    key: &str,
    line: &str,
    pos: i64,
) -> Result<String> {
    let module_arc = ctx
        .core
        .module
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("__repl_set_key_editor: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();
    let call_args = [Value::Str(key.into()), Value::Str(line.into()), Value::I64(pos)];

    let outcome = match module.func_index.get(fqn) {
        Some(&idx) => {
            let f = &module.functions[idx];
            key_edit_arity_check(fqn, f.param_count)?;
            exec_function(ctx, module, f, &call_args)?
        }
        None => {
            let f = ctx
                .try_lookup_function(fqn)
                .ok_or_else(|| anyhow::anyhow!("__repl_set_key_editor: key-editor `{fqn}` not found"))?;
            key_edit_arity_check(fqn, f.param_count)?;
            exec_function(ctx, module, f.as_ref(), &call_args)?
        }
    };

    match outcome {
        ExecOutcome::Returned(Some(Value::Str(s))) => Ok(s.to_string()),
        ExecOutcome::Returned(_) => Ok(String::new()),
        ExecOutcome::Thrown(val) => {
            ctx.set_pending_thrown(val);
            bail!("__z42_reflected_throw__")
        }
    }
}

/// The key-editor must take exactly `(string key, string line, int pos)` — 3 params.
#[cfg(not(target_arch = "wasm32"))]
fn key_edit_arity_check(fqn: &str, param_count: usize) -> Result<()> {
    if param_count != 3 {
        bail!("__repl_set_key_editor: key-editor `{fqn}` must take (string key, string line, int pos) — 3 params, got {param_count}");
    }
    Ok(())
}

/// Translate a z42 action string into a rustyline `Cmd`; `None` = the key's default.
///
/// Only two actions, both mapping to **redo-immune** commands. rustyline runs a
/// custom binding's repeatable command through `redo(Some(n))`, where `n` is the
/// numeric-argument prefix (1 for a plain keypress). That clobbers any count baked
/// into a movement — e.g. `Kill(BackwardChar(4))` degrades to `BackwardChar(1)`,
/// deleting a single char. `Cmd::Indent`/`Dedent(WholeLine)` sidestep this: they
/// take a count-less movement and dedent/indent by `config.indent_size()` (set to 4
/// in `read_one_line`), so the level survives redo intact.
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_action(s: &str) -> Option<Cmd> {
    match s {
        "indent" => Some(Cmd::Indent(Movement::WholeLine)),
        "dedent" => Some(Cmd::Dedent(Movement::WholeLine)),
        _ => None,
    }
}

/// Conditional handler bound to a controlled key (Backspace / Tab / `}`). Asks the z42
/// key-editor for an action, then maps it to a `Cmd`. Returns `None` (default behavior)
/// when no editor is registered, no live ctx, the callback errors, or the action is
/// empty — so normal editing is never blocked.
#[cfg(not(target_arch = "wasm32"))]
pub struct KeyEditHandler {
    key: &'static str,
}

#[cfg(not(target_arch = "wasm32"))]
impl KeyEditHandler {
    pub fn new(key: &'static str) -> Self {
        Self { key }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ConditionalEventHandler for KeyEditHandler {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ectx: &EventContext,
    ) -> Option<Cmd> {
        let fqn = REGISTERED_KEY_EDITOR.get().and_then(|m| m.lock().clone())?;
        let ctx_ptr = super::repl::active_ctx_ptr();
        if ctx_ptr.is_null() {
            return None;
        }
        // SAFETY: same window/thread invariant as `Completer::complete` — ACTIVE_CTX
        // holds a live `&VmContext` for the duration of `ed.readline()`, cleared right
        // after; handlers only run during that span on this thread.
        let vmctx: &VmContext = unsafe { &*ctx_ptr };
        // Fires mid-readline, inside the outer NativeParkGuard; temporarily un-park so
        // the callback runs as a normal mutator (same as the completer).
        let _unpark = crate::gc::NativeUnparkGuard::exit(vmctx);
        match key_edit_via_callback(vmctx, &fqn, self.key, ectx.line(), ectx.pos() as i64) {
            Ok(action) => parse_action(&action),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
#[path = "repl_editing_tests.rs"]
mod repl_editing_tests;
