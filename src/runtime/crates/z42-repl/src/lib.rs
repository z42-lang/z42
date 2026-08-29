//! z42-repl — host-only toolchain native extension backing the interactive REPL
//! (`z42i`)'s line editor. Wraps rustyline; z42vm dlopens it lazily on first
//! `__repl_readline` and drives it through a **pure C ABI**.
//!
//! # Boundary invariant
//!
//! No z42 internal type (`Value` / `VmContext`) crosses this boundary. All VM
//! re-entrancy — completion candidates and key-editing decisions — crosses back
//! via the C function pointers in [`ReplCallbacks`], which the z42vm side supplies
//! for one `readline` span. `ctx` is an opaque `*mut VmContext` the callbacks cast
//! back internally; this crate never dereferences it. This decoupling means the
//! crate has zero dependency on the z42 main crate (no Cargo cycle), mirroring
//! `z42-compression` — except the REPL also needs the *reverse* direction (native →
//! VM), which the callbacks provide.
//!
//! # Discovery / packaging
//!
//! Unlike `z42-compression` (a cross-platform stdlib extension in `<sdk>/native/`),
//! the REPL editor is **host-only toolchain**: the built `libz42_repl.{so,dylib,dll}`
//! ships next to `z42i` in the toolchain/interactive dir and is found by z42vm's
//! repl-specific lazy loader (`corelib::repl_native`), NOT the general
//! `native_search_paths()`. wasm/mobile never load it (they keep the plain-stdin
//! fallback). (extract-repl-native-cdylib)

use std::cell::Cell;
use std::ffi::{c_char, c_void, CStr, CString};

mod editing;
mod helper;
mod history;

// ── Result kinds (written to `*out_kind` by `z42_repl_readline`) ─────────────

/// A line/statement was read; the return value is an owned C string.
pub const Z42_REPL_LINE: i32 = 0;
/// Ctrl-D / EOF: return value is null.
pub const Z42_REPL_EOF: i32 = 1;
/// Ctrl-C / interrupt: return value is null (z42 side re-prompts).
pub const Z42_REPL_INTERRUPT: i32 = 2;
/// Editor error (message in [`z42_repl_last_error`]); return value is null.
pub const Z42_REPL_ERROR: i32 = 3;
/// The editor could not initialize (no tty / unsupported terminal); return value
/// is null. Distinct from `Z42_REPL_ERROR` so the z42vm side falls back to a plain
/// stdin read (matching the old in-VM behavior where a failed `Editor::with_config`
/// dropped to `plain_readline`) instead of surfacing an error.
pub const Z42_REPL_NO_EDITOR: i32 = 4;

// ── Re-entrancy callbacks (VM side supplies; this crate calls back) ──────────

/// C callback table the z42vm side installs for one `z42_repl_readline` span.
/// The fn pointers re-enter the VM to run the z42 completer / key-editor; `ctx`
/// is an opaque `*mut VmContext` passed straight back. Strings the callbacks
/// return are owned by the VM side and must be released with `free_str`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ReplCallbacks {
    /// Opaque `*mut VmContext` — passed back verbatim, never dereferenced here.
    pub ctx: *mut c_void,
    /// `complete(ctx, line, pos)` → owned C string of `\n`-joined candidates
    /// (`""` or null = no candidates). Release with `free_str`.
    pub complete: extern "C" fn(*mut c_void, *const c_char, usize) -> *mut c_char,
    /// `key_edit(ctx, key, line, pos)` → owned C string action string
    /// (`""` or null = perform the key's default). Release with `free_str`.
    pub key_edit: extern "C" fn(*mut c_void, *const c_char, *const c_char, usize) -> *mut c_char,
    /// Release a string returned by `complete` / `key_edit`.
    pub free_str: extern "C" fn(*mut c_char),
}

thread_local! {
    /// The active callbacks for the current `z42_repl_readline` span; read by the
    /// rustyline trait impls (`helper` / `editing`). Null outside a readline span.
    /// Same soundness argument as z42vm's old `ACTIVE_CTX`: the pointer is valid
    /// for exactly the duration of `ed.readline()` on this thread.
    static CBS: Cell<*const ReplCallbacks> = const { Cell::new(std::ptr::null()) };
}

/// Invoke the completion callback for `(line, pos)`; returns candidates (empty on
/// no-editor / error / no candidates). Used by the `Completer` and the identifier
/// hinter. Splits the VM side's `\n`-joined payload.
pub(crate) fn call_complete(line: &str, pos: usize) -> Vec<String> {
    let p = CBS.with(|c| c.get());
    if p.is_null() {
        return Vec::new();
    }
    // SAFETY: CBS holds a live `&ReplCallbacks` for the readline span; trait impls
    // only run on this thread during that span.
    let cbs = unsafe { &*p };
    let cline = match CString::new(line) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let raw = (cbs.complete)(cbs.ctx, cline.as_ptr(), pos);
    if raw.is_null() {
        return Vec::new();
    }
    let s = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    (cbs.free_str)(raw);
    if s.is_empty() {
        Vec::new()
    } else {
        s.split('\n').map(str::to_string).collect()
    }
}

/// Invoke the key-editor callback for `(key, line, pos)`; returns the action
/// string, or `None` for the key's default (no editor / error / empty action).
pub(crate) fn call_key_edit(key: &str, line: &str, pos: usize) -> Option<String> {
    let p = CBS.with(|c| c.get());
    if p.is_null() {
        return None;
    }
    // SAFETY: as `call_complete`.
    let cbs = unsafe { &*p };
    let ckey = CString::new(key).ok()?;
    let cline = CString::new(line).ok()?;
    let raw = (cbs.key_edit)(cbs.ctx, ckey.as_ptr(), cline.as_ptr(), pos);
    if raw.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned();
    (cbs.free_str)(raw);
    Some(s)
}

// ── Last-error slot (queried by z42vm on Z42_REPL_ERROR) ─────────────────────

thread_local! {
    static LAST_ERROR: std::cell::RefCell<CString> =
        std::cell::RefCell::new(CString::new("").unwrap());
}

fn set_last_error(msg: impl Into<Vec<u8>>) {
    let c = CString::new(msg).unwrap_or_else(|_| CString::new("repl error").unwrap());
    LAST_ERROR.with(|e| *e.borrow_mut() = c);
}

/// Pointer to the current thread's last-error C string (empty when none). Valid
/// until the next entry call on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn z42_repl_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr())
}

/// Free a string returned by [`z42_repl_readline`].
///
/// # Safety
/// `s` must be a pointer previously returned by `z42_repl_readline` (or null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_repl_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

// ── The editor entry (host-only; the whole crate is only built for host) ─────

/// Read one edited (possibly multi-line) statement.
///
/// - `prompt`: NUL-terminated prompt string.
/// - `cbs`: the VM re-entrancy callbacks for this span (non-null).
/// - `out_kind`: written with `Z42_REPL_LINE` / `_EOF` / `_INTERRUPT` / `_ERROR`.
///
/// Returns an owned C string (the line) on `Z42_REPL_LINE` — release with
/// [`z42_repl_free`] — otherwise null.
///
/// # Safety
/// `prompt`, `cbs`, `out_kind` must be valid; `cbs`'s fn pointers must stay valid
/// for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_repl_readline(
    prompt: *const c_char,
    cbs: *const ReplCallbacks,
    out_kind: *mut i32,
) -> *mut c_char {
    let prompt = if prompt.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(prompt) }.to_string_lossy().into_owned()
    };
    // Publish the callbacks for the trait impls, strictly for this span.
    CBS.with(|c| c.set(cbs));
    let result = read_one_line(&prompt);
    CBS.with(|c| c.set(std::ptr::null()));

    match result {
        Ok(Some(line)) => {
            unsafe { *out_kind = Z42_REPL_LINE };
            match CString::new(line) {
                Ok(c) => c.into_raw(),
                Err(_) => {
                    // Interior NUL — treat as error rather than truncating silently.
                    set_last_error("line contained an interior NUL byte");
                    unsafe { *out_kind = Z42_REPL_ERROR };
                    std::ptr::null_mut()
                }
            }
        }
        Ok(None) => {
            unsafe { *out_kind = Z42_REPL_EOF };
            std::ptr::null_mut()
        }
        Err(kind) => {
            unsafe { *out_kind = kind };
            std::ptr::null_mut()
        }
    }
}

/// Process-global editor (shared history + helper across calls). Lazily created;
/// `None` when rustyline can't init (no tty) → the caller's plain-stdin fallback
/// is used on the z42vm side (this returns `Err(Z42_REPL_ERROR)` with a message
/// so the shim can decide; but in practice z42vm only reaches here when a tty is
/// present — it checks `Console.IsTerminal()` first).
type Editor = rustyline::Editor<helper::ReplHelper, rustyline::history::DefaultHistory>;

fn editor() -> &'static std::sync::Mutex<Option<Editor>> {
    use std::sync::OnceLock;
    static EDITOR: OnceLock<std::sync::Mutex<Option<Editor>>> = OnceLock::new();
    EDITOR.get_or_init(|| std::sync::Mutex::new(build_editor()))
}

fn build_editor() -> Option<Editor> {
    use rustyline::{CompletionType, Config, EventHandler, KeyCode, KeyEvent, Modifiers};
    // List completion + 4-space indent grid, matching the previous in-VM setup.
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .indent_size(4)
        .build();
    let mut ed = Editor::with_config(config).ok()?;
    ed.set_helper(Some(helper::ReplHelper::new()));
    // Indent-aware editing keys: the decision logic lives in z42 (reached via the
    // key_edit callback); KeyEditHandler only relays (key, line, pos) and maps the
    // returned action string to a redo-immune Cmd. Backspace / Tab / Enter / `}`.
    for key in ["backspace", "tab", "enter"] {
        let code = match key {
            "backspace" => KeyCode::Backspace,
            "tab" => KeyCode::Tab,
            _ => KeyCode::Enter,
        };
        ed.bind_sequence(
            KeyEvent(code, Modifiers::NONE),
            EventHandler::Conditional(Box::new(editing::KeyEditHandler::new(key))),
        );
    }
    if let Some(p) = history::history_path() {
        let _ = ed.load_history(&p);
    }
    Some(ed)
}

/// Returns `Ok(Some(line))` for a line, `Ok(None)` for EOF, `Err(kind)` for
/// interrupt / error.
fn read_one_line(prompt: &str) -> Result<Option<String>, i32> {
    use rustyline::error::ReadlineError;
    let cell = editor();
    // Recover from a poisoned lock rather than panicking (a panic in a prior
    // readline shouldn't wedge the REPL); the editor state is still usable.
    let mut guard = cell.lock().unwrap_or_else(|e| e.into_inner());
    let ed = match guard.as_mut() {
        Some(ed) => ed,
        None => {
            // Editor never initialized (no tty / unsupported terminal). Signal the
            // z42vm side to use its plain-stdin fallback rather than erroring.
            return Err(Z42_REPL_NO_EDITOR);
        }
    };
    match ed.readline(prompt) {
        Ok(line) => {
            let _ = ed.add_history_entry(line.as_str());
            if let Some(p) = history::history_path() {
                let _ = ed.save_history(&p);
            }
            Ok(Some(line))
        }
        Err(ReadlineError::Interrupted) => Err(Z42_REPL_INTERRUPT),
        Err(ReadlineError::Eof) => Ok(None),
        Err(e) => {
            set_last_error(format!("{e}"));
            Err(Z42_REPL_ERROR)
        }
    }
}
