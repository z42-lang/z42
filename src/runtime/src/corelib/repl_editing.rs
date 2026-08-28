//! REPL indent-aware key editing — VM-side re-entrancy (add-repl-indent-editing;
//! editor extracted to cdylib by extract-repl-native-cdylib).
//!
//! **Policy-free.** The decision logic all lives in z42
//! (`Std.Repl.ReplEditing.KeyEdit`, reached via the registered free function
//! `replKeyEdit`). The rustyline `KeyEditHandler` + action→`Cmd` translation now
//! live in the cdylib (`crates/z42-repl/src/editing.rs`); this module keeps only
//! the VM side:
//!   - `__repl_set_key_editor` — register the z42 key-editor FQN, and
//!   - `key_edit_via_callback` — re-enter the VM to run it, plus
//!   - `keyedit_trampoline` — the `extern "C"` shim the cdylib calls back through
//!     on each controlled key to fetch the action string.
//!
//! Action-string protocol (the cdylib is a dumb translator — no *language*
//! decisions Rust-side; the one cursor mechanic it owns is the Enter at-end gate):
//!   ""              → the key's default (Tab→complete, Backspace→delete 1, Enter→submit)
//!   "dedent"        → remove one `indent_size`
//!   "insert:<text>" → insert literal text (Tab grid-snap-ceil)
//!   "newline:<ind>" → insert newline + continuation indent (Enter, incomplete buffer)
//!   "accept"        → submit (Enter, complete buffer)
//! The mapping of these strings to redo-immune `Cmd`s lives in the cdylib.

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
use std::ffi::{c_char, c_void, CStr, CString};

/// Registered key-editor FQN (set by z42 via `__repl_set_key_editor`); read by
/// `keyedit_trampoline` on each controlled keypress. Process-global (one REPL per
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

// ── Host-only: VM re-entrancy for the cdylib key handlers ────────────────────
// (wasm / no-native-interop fall back to plain stdin, no editing)

#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
use crate::interp::{exec_function, ExecOutcome};

/// Invoke the z42 key-editor `fqn(key, line, pos)` and return its action string.
/// Mirrors `repl::complete_via_callback` but with (string, string, int) args and a
/// string result. A `throw` inside propagates via `set_pending_thrown`.
#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
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
#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
fn key_edit_arity_check(fqn: &str, param_count: usize) -> Result<()> {
    if param_count != 3 {
        bail!("__repl_set_key_editor: key-editor `{fqn}` must take (string key, string line, int pos) — 3 params, got {param_count}");
    }
    Ok(())
}

/// `extern "C"` key-edit trampoline the cdylib calls back through
/// `ReplCallbacks.key_edit` on each controlled key (Backspace / Tab / Enter). Casts
/// the opaque `ctx` back to `&VmContext`, un-parks GC (the outer `NativeParkGuard`
/// from `builtin_repl_readline` is active), runs the z42 key-editor via
/// `key_edit_via_callback`, and returns the action string as an owned C string.
/// Returns null on no-editor / empty action / error → the cdylib performs the key's
/// default (preserving the old "throw becomes a no-op" semantics). The returned
/// string is freed by the cdylib via `ReplCallbacks.free_str`
/// (= `repl_native::z42vm_free_str`). The Enter at-end gate lives in the cdylib.
///
/// # Safety
/// `ctx` must be the live `*mut VmContext` installed for this `readline` span; `key`
/// and `line` valid NUL-terminated C strings. Same window/thread invariant as
/// `repl::complete_trampoline`.
#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
pub(crate) extern "C" fn keyedit_trampoline(
    ctx: *mut c_void,
    key: *const c_char,
    line: *const c_char,
    pos: usize,
) -> *mut c_char {
    if ctx.is_null() || key.is_null() || line.is_null() {
        return std::ptr::null_mut();
    }
    let fqn = match REGISTERED_KEY_EDITOR.get().and_then(|m| m.lock().clone()) {
        Some(f) => f,
        None => return std::ptr::null_mut(),
    };
    // SAFETY: as `repl::complete_trampoline`.
    let vmctx: &VmContext = unsafe { &*(ctx as *const VmContext) };
    let key_str = unsafe { CStr::from_ptr(key) }.to_string_lossy().into_owned();
    let line_str = unsafe { CStr::from_ptr(line) }.to_string_lossy().into_owned();
    // Fires mid-readline, inside the outer NativeParkGuard; temporarily un-park so
    // the callback runs as a normal mutator (same as the completer).
    let _unpark = crate::gc::NativeUnparkGuard::exit(vmctx);
    match key_edit_via_callback(vmctx, &fqn, &key_str, &line_str, pos as i64) {
        Ok(action) if !action.is_empty() => match CString::new(action) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        // Empty action (key's default) or a swallowed throw → null → cdylib default.
        _ => std::ptr::null_mut(),
    }
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "native-interop"))]
#[path = "repl_editing_tests.rs"]
mod repl_editing_tests;
