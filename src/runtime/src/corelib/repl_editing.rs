//! REPL indent-aware key editing (add-repl-indent-editing).
//!
//! **Policy-free adapter shell.** The decision logic all lives in z42
//! (`Std.Repl.ReplEditing.KeyEdit`, reached via the registered free function
//! `replKeyEdit`). Here we only, on each controlled key (Backspace / Tab):
//!   1. re-enter the VM to ask z42 for an "action string", then
//!   2. translate that string into a rustyline `Cmd`.
//! Mirrors the Tab-completion callback (`SetCompleter` / `replComplete`) re-entrancy:
//! the FQN comes from a process-global, the live `&VmContext` from the readline-span
//! thread-local `ACTIVE_CTX` (published by `repl::read_one_line`).
//!
//! Action-string protocol (Rust is a dumb translator — no *language* decisions here;
//! the one cursor mechanic Rust owns is the Enter at-end gate, see the handler). Non-empty
//! actions map to **redo-immune** commands (see `parse_action`):
//!   ""              → `None` (perform the key's default: Tab→complete,
//!                             Backspace→delete one char, Enter→submit)
//!   "dedent"        → `Cmd::Dedent(Movement::WholeLine)`   (remove one `indent_size`)
//!   "insert:<text>" → `Cmd::Insert(1, text)`               (kill-ring-free; Tab grid-snap-ceil)
//!   "newline:<ind>" → `Cmd::Insert(1, "\n"+ind)`           (Enter on an incomplete buffer:
//!                                                            insert newline + continuation indent)
//!   "accept"        → `Cmd::AcceptLine`                     (Enter on a complete buffer: submit)
//!
//! Whole-buffer multiline (add-repl-multiline-editing): Enter is bound to the same
//! handler with key `"enter"`. The z42 side judges whether the *whole buffer* is complete
//! (`Completeness.IsIncomplete`) and returns "accept" (complete) or "newline:<indent>"
//! (incomplete → keep editing). `EventContext::line()` is the entire multi-line buffer
//! (with `\n`), `pos()` the global cursor offset — so one `readline()` spans a whole
//! statement and the z42 loop no longer accumulates line-by-line.

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
/// Both actions map to **redo-immune** commands. rustyline runs a custom binding's
/// repeatable command through `redo(Some(n))`, where `n` is the numeric-argument
/// prefix (1 for a plain keypress). That clobbers any count baked into a *movement* —
/// e.g. `Kill(BackwardChar(4))` degrades to `BackwardChar(1)`. Immunity comes from:
///   - `Dedent(Movement::WholeLine)`: `WholeLine` carries no count; dedents by
///     `config.indent_size()` (=4 in `read_one_line`), so the level survives redo.
///     Used for Backspace (one-level dedent, kill-ring-free, cursor correct).
///   - `Insert(1, text)`: the count is on the repeat, not the text — `redo(Some(1))`
///     re-inserts the full `text` once, cursor lands after it. Used for Tab
///     grid-snap-ceil: insert `next_stop - col` spaces (kill-ring-free). `insert:`
///     carries the literal spaces after the prefix (no escaping; never a colon).
///
/// Enter (add-repl-multiline-editing):
///   - `"accept"`        → `Cmd::AcceptLine`. `command.rs` matches `(Cmd::AcceptLine, ..)`
///     → **unconditional submit** (validator/cursor ignored), so it needs no validator.
///     The handler additionally gates on cursor-at-end (see `handle`) so mid-buffer Enter
///     on a complete buffer splits instead of submitting.
///   - `"newline:<ind>"` → `Cmd::Insert(1, "\n"+ind)`: insert a newline plus the
///     continuation indent, cursor landing after it (same redo-immune `Insert` as Tab).
///
/// Not here: a `Replace(WholeLine, text)` action for variable-width backspace floor /
/// `}` auto-dedent. That command IS redo-immune, but `edit_insert_text` doesn't advance
/// the cursor, so it homes to column 0 — breaking typing after `}` (`} else {`). Those
/// stay Deferred pending a rustyline `edit_insert_text` cursor fix. (add-repl-tab-grid-snap)
#[cfg(not(target_arch = "wasm32"))]
pub fn parse_action(s: &str) -> Option<Cmd> {
    match s {
        "dedent" => Some(Cmd::Dedent(Movement::WholeLine)),
        "accept" => Some(Cmd::AcceptLine),
        _ if s.starts_with("insert:") => Some(Cmd::Insert(1, s["insert:".len()..].to_string())),
        _ if s.starts_with("newline:") => {
            Some(Cmd::Insert(1, format!("\n{}", &s["newline:".len()..])))
        }
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
            // Enter at-end gate (the one cursor mechanic Rust owns): "accept" means z42
            // judged the whole buffer complete. Submit only when the cursor is at the very
            // end (byte-robust, so UTF-8 in string literals is handled correctly); a
            // mid-buffer Enter on a complete buffer inserts a newline instead
            // (accept_in_the_middle: false — the recommended multiline UX).
            Ok(action) if action == "accept" && ectx.pos() != ectx.line().len() => {
                Some(Cmd::Insert(1, "\n".to_string()))
            }
            Ok(action) => parse_action(&action),
            Err(_) => None,
        }
    }
}

#[cfg(test)]
#[path = "repl_editing_tests.rs"]
mod repl_editing_tests;
