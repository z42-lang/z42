//! REPL line editor builtins — back `Std.Repl.ReadLine` used by the native
//! interactive REPL (`z42i`, add-z42-repl).
//!
//! `__repl_readline(prompt, initial)` → one edited line (history, emacs keys,
//! Ctrl-D EOF), with the edit buffer pre-filled by `initial` (empty for a fresh
//! line; a computed indent string for an auto-indented continuation line).
//!
//! Multi-line accumulation, completeness judgment, AND the continuation-indent
//! computation now all live in the script layer (sink-repl-indent-to-script,
//! following add-repl-parser-completeness): the parser is the authority on "is the
//! input complete?" and the z42-side `Std.Scripting.Completeness` computes the
//! indent from the accumulated text (via the existing Lexer). This builtin is a
//! plain "read one line, pre-filled with the given string" primitive — no bracket
//! state machine remains Rust-side.
//!
//! Return convention: `Value::Str` for a line/block; `Value::Null` on EOF
//! (Ctrl-D) or interrupt (Ctrl-C) so the z42 side can treat null as "exit".
//! An empty line is `Value::Str("")` — distinct from EOF.
//!
//! Host-only: rustyline backs the editor on non-wasm targets; wasm32 (where the
//! rustyline dep is cfg-gated out) falls back to a plain stdin read so the
//! builtins still resolve. The REPL itself is host-only (scripting-charter 2b).

use crate::corelib::reflection::{builtin_type_members, make_type_from_name};
use crate::interp::{exec_function, ExecOutcome};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

/// Extract argument `idx` as an owned prompt string (empty when absent/non-str).
fn prompt_arg(args: &[Value], idx: usize) -> String {
    match args.get(idx) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}

/// `__repl_readline(prompt: string, initial: string) -> string?` — read one edited
/// line, pre-filling the edit buffer with `initial` (empty → a fresh line; a computed
/// indent string → an auto-indented continuation line, cursor landing after it).
/// Returns null on Ctrl-D (EOF) / Ctrl-C (interrupt).
pub fn builtin_repl_readline(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let prompt = prompt_arg(args, 0);
    let initial = prompt_arg(args, 1);
    // add-repl-prewarm: GC-safe park for the blocking read so a background
    // prewarm thread's GC can proceed while this thread waits on stdin.
    let _park = crate::gc::NativeParkGuard::enter(ctx);
    read_one_line(ctx, &prompt, &initial)
}

/// `__repl_set_completer(fqn: string) -> void` — register the z42 completer the Tab
/// key invokes (signature `string[] complete(string line, int pos)`). Empty string
/// clears it. Process-global; the rustyline `Completer` reads it on each Tab.
/// D5 spike (add-completion-query-api, risk B).
pub fn builtin_repl_set_completer(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fqn = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__repl_set_completer: arg 0 must be the completer's fully-qualified name (string)"),
    };
    let slot = REGISTERED_COMPLETER.get_or_init(|| parking_lot::Mutex::new(None));
    *slot.lock() = if fqn.is_empty() { None } else { Some(fqn) };
    Ok(Value::Null)
}

/// Registered completer FQN (set by `__repl_set_completer`); read by the rustyline
/// `Completer` on Tab. Process-global (one REPL per process).
static REGISTERED_COMPLETER: std::sync::OnceLock<parking_lot::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

/// `__repl_complete_probe(fqn: string, line: string, pos: int) -> string[]` —
/// D5 spike (add-completion-query-api): prove the VM re-entrancy path for Tab
/// completion WITHOUT rustyline/PTY. Invokes the z42 completion callback named
/// `fqn` (signature `string[] complete(string line, int pos)`) with the current
/// line + cursor, and relays its returned `string[]` back verbatim. Mirrors
/// `reflection::builtin_invoke_static` but with two call args; the completer
/// builds the array z42-side so no Rust-side array construction is needed.
///
/// This is the callback core the rustyline `Completer` (risk B) will reuse; here
/// it is exposed as a direct builtin so a piped z42 program can verify risk A
/// (builtin → z42 callback with args → string[] back) end-to-end on the real VM.
pub fn builtin_repl_complete_probe(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fqn = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__repl_complete_probe: arg 0 must be the completer's fully-qualified name (string)"),
    };
    let line = match args.get(1) {
        Some(v @ Value::Str(_)) => v.clone(),
        _ => bail!("__repl_complete_probe: arg 1 (line) must be a string"),
    };
    let pos = match args.get(2) {
        Some(Value::I64(n)) => *n,
        _ => bail!("__repl_complete_probe: arg 2 (pos) must be an int"),
    };
    complete_via_callback(ctx, &fqn, line, pos)
}

/// Shared callback core: invoke the z42 completer `fqn(line, pos)` and return its
/// `string[]` result Value. Reused by the probe (risk A) and — once wired — the
/// rustyline `Completer` (risk B). A `throw` inside the completer propagates with
/// its original type via `ctx.set_pending_thrown`, same convention as
/// `__invoke_static`.
fn complete_via_callback(ctx: &VmContext, fqn: &str, line: Value, pos: i64) -> Result<Value> {
    let module_arc = ctx
        .core
        .module
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("__repl_complete_probe: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();
    let call_args = [line, Value::I64(pos)];

    let outcome = match module.func_index.get(fqn) {
        Some(&idx) => {
            let f = &module.functions[idx];
            complete_arity_check(fqn, f.param_count)?;
            exec_function(ctx, module, f, &call_args)?
        }
        None => {
            let f = ctx
                .try_lookup_function(fqn)
                .ok_or_else(|| anyhow::anyhow!("__repl_complete_probe: completer `{fqn}` not found"))?;
            complete_arity_check(fqn, f.param_count)?;
            exec_function(ctx, module, f.as_ref(), &call_args)?
        }
    };

    match outcome {
        ExecOutcome::Returned(Some(v)) => Ok(v),
        ExecOutcome::Returned(None) => Ok(Value::Null),
        ExecOutcome::Thrown(val) => {
            ctx.set_pending_thrown(val);
            bail!("__z42_reflected_throw__")
        }
    }
}

/// `__repl_member_names(staticFieldFqn: string) -> string[]` — D2 live-reflection
/// member completion (add-completion-query-api 阶段 3b). Reads a REPL session
/// variable's **live value** from its static-field slot (key
/// `Repl.R{N}.Vars{N}.{var}`, see loader.rs static-field FQN convention), then
/// returns the names of its runtime type's members (fields/methods/properties/
/// nested — same set as `Type.GetMembers()`). Reading a stored static field is
/// side-effect-free (no expression re-evaluation), which is why session-variable
/// `obj.` completion is safe here (D2). Null / primitive / unloaded → empty.
pub fn builtin_repl_member_names(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fqn = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__repl_member_names: arg 0 must be the static-field FQN (string)"),
    };
    let v = ctx.static_get(&fqn);
    let tname = match &v {
        Value::Object(rc) => rc.type_desc().name.clone(),
        // add-repl-completion-iter2: primitive-typed session vars → the **same** canonical
        // stdlib class `GetType()` resolves (Std.String / Std.Int32 / boxed's exact class),
        // so member reflection returns the real member set (string.Length/CharAt,
        // int.ToString, …). Mirrors `object::builtin_obj_get_type` so the names actually
        // resolve in `make_type_from_name` (hardcoding "string"/"int" would not).
        Value::Str(_) => crate::metadata::well_known_names::STD_STRING.to_string(),
        // unify Phase 2 R3: 装箱值类型（struct 或基元）→ 精确类型名（type_desc.name）。
        Value::BoxedStruct(b) => b.type_desc().name.to_string(),
        v @ (Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_)) => {
            match crate::interp::primitive_class_name(v) {
                Some(cn) => cn.to_string(),
                None => return Ok(ctx.heap().alloc_array(Vec::new())),
            }
        }
        // Null / arrays / other → no member completion.
        _ => return Ok(ctx.heap().alloc_array(Vec::new())),
    };
    let type_val = make_type_from_name(ctx, &tname);
    let members = builtin_type_members(ctx, &[type_val])?;
    let mut names: Vec<Value> = Vec::new();
    if let Value::Array(a) = members {
        for m in a.borrow().iter_boxed() {
            if let Some(n) = member_name_of(&m) {
                names.push(Value::Str(n.into()));
            }
        }
    }
    Ok(ctx.heap().alloc_array(names))
}

/// Read the `Name` slot of a reflection `MemberInfo` (or subclass) object.
fn member_name_of(v: &Value) -> Option<String> {
    if let Value::Object(rc) = v {
        let idx = rc.type_desc().field_index.get("Name").copied();
        if let Some(i) = idx {
            if let Some(Value::Str(s)) = rc.borrow().slots.get(i) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// The completer must take exactly `(string line, int pos)` — 2 params, no receiver.
fn complete_arity_check(fqn: &str, param_count: usize) -> Result<()> {
    if param_count != 2 {
        bail!("__repl_complete_probe: completer `{fqn}` must take (string line, int pos) — 2 params, got {param_count}");
    }
    Ok(())
}

// ── Tab completion (D5 spike, add-completion-query-api) ──────────────────────
//
// rustyline's `Completer` runs Rust-side mid-`readline()`. To fetch candidates it
// must call back into the VM. We reuse the (Risk-A-proven) `complete_via_callback`;
// the two pieces it needs are supplied out-of-band:
//   • the completer FQN — process-global `REGISTERED_COMPLETER` (set by z42 via
//     `__repl_set_completer`);
//   • the live `&VmContext` — a thread-local raw pointer set for exactly the span
//     of `ed.readline()` (below). Sound because `read_one_line` holds `&VmContext`
//     alive across that call and clears the pointer immediately after; `complete()`
//     only ever runs on the same thread during that window.
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static ACTIVE_CTX: std::cell::Cell<*const VmContext> = const { std::cell::Cell::new(std::ptr::null()) };
}

#[cfg(not(target_arch = "wasm32"))]
struct ReplHelper {
    /// fish-style ghost from prior inputs; the fallback when no live identifier
    /// completion extends the current word. (add-repl-multiline-completion)
    history: rustyline::hint::HistoryHinter,
}

#[cfg(not(target_arch = "wasm32"))]
impl ReplHelper {
    fn new() -> Self {
        Self { history: rustyline::hint::HistoryHinter::new() }
    }

    /// Inline completion hint for the word under the cursor: ask the session completer
    /// for candidates and ghost the suffix of the first one that *strictly extends* the
    /// typed word. Works for both bare identifiers (`Con`→`sole`) **and member context**
    /// (`Console.W`→`riteLine`) — the latter is what the user expects when typing a
    /// receiver's method. The completer reconciles the receiver type on demand (cached
    /// after the first hit), so the per-keystroke cost matches Tab's and is no worse than
    /// the bare-word ghost already does. Prefix-only filter (`starts_with`) means a
    /// non-extending candidate list simply yields no hint — the ghost is never wrong, at
    /// worst absent. (add-repl-multiline-completion; member ghost: add-repl-type-metacommand)
    fn identifier_hint(&self, line: &str, pos: usize) -> Option<String> {
        let start = word_start(line, pos);
        let word = &line[start..pos];
        // Only skip the empty word (nothing typed yet after a `.` or at line start) —
        // hinting all members off an empty prefix would be noise.
        if word.is_empty() {
            return None;
        }
        let fqn = REGISTERED_COMPLETER.get().and_then(|m| m.lock().clone())?;
        let ctx_ptr = ACTIVE_CTX.with(|c| c.get());
        if ctx_ptr.is_null() {
            return None;
        }
        // SAFETY: same window/thread invariant as `Completer::complete` — ACTIVE_CTX
        // holds a live `&VmContext` for the duration of `ed.readline()`, and the hinter
        // only runs during that span on this thread.
        let ctx: &VmContext = unsafe { &*ctx_ptr };
        let _unpark = crate::gc::NativeUnparkGuard::exit(ctx);
        let line_val = Value::Str(line.into());
        let cands = match complete_via_callback(ctx, &fqn, line_val, pos as i64) {
            Ok(Value::Array(a)) => a
                .borrow()
                .iter_boxed()
                .filter_map(|v| match v {
                    Value::Str(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect::<Vec<String>>(),
            _ => return None,
        };
        cands
            .into_iter()
            .find(|c| c.len() > word.len() && c.starts_with(word))
            .map(|c| c[word.len()..].to_string())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl rustyline::completion::Completer for ReplHelper {
    type Candidate = String;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _rlctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let fqn = match REGISTERED_COMPLETER.get().and_then(|m| m.lock().clone()) {
            Some(f) => f,
            None => return Ok((pos, Vec::new())),
        };
        let ctx_ptr = ACTIVE_CTX.with(|c| c.get());
        if ctx_ptr.is_null() {
            return Ok((pos, Vec::new()));
        }
        // SAFETY: set to a live `&VmContext` for the duration of `ed.readline()` in
        // `read_one_line`, cleared right after; `complete` only runs during that span
        // on this thread.
        let ctx: &VmContext = unsafe { &*ctx_ptr };
        // add-repl-prewarm: this callback fires mid-`readline`, i.e. inside the
        // outer NativeParkGuard. Temporarily un-park so the completer runs as a
        // normal mutator (parking at its own safepoints if a background GC is
        // requested); re-parks on drop before returning to the blocking read.
        let _unpark = crate::gc::NativeUnparkGuard::exit(ctx);
        let start = word_start(line, pos);
        let line_val = Value::Str(line.into());
        match complete_via_callback(ctx, &fqn, line_val, pos as i64) {
            Ok(Value::Array(a)) => {
                let cands = a
                    .borrow()
                    .iter_boxed()
                    .filter_map(|v| match v {
                        Value::Str(s) => Some(s.to_string()),
                        _ => None,
                    })
                    .collect();
                Ok((start, cands))
            }
            _ => Ok((pos, Vec::new())),
        }
    }
}

// Hinter: inline ghost suggestion. Tries live identifier completion first (ghost the
// unique/first prefix-extending candidate), then falls back to fish-style history.
#[cfg(not(target_arch = "wasm32"))]
impl rustyline::hint::Hinter for ReplHelper {
    type Hint = String;
    fn hint(&self, line: &str, pos: usize, rlctx: &rustyline::Context<'_>) -> Option<String> {
        // Only at end-of-line on a non-empty buffer (matches rustyline's own ghosting
        // convention — never hint mid-line where it would collide with existing text).
        if line.is_empty() || pos != line.len() {
            return None;
        }
        self.identifier_hint(line, pos)
            .or_else(|| self.history.hint(line, pos, rlctx))
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl rustyline::highlight::Highlighter for ReplHelper {
    // Render the inline hint dimmed (ANSI bright-black) so the ghost suggestion reads
    // as a suggestion, not as text the user typed. (add-repl-multiline-completion)
    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        std::borrow::Cow::Owned(format!("\x1b[90m{hint}\x1b[0m"))
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl rustyline::validate::Validator for ReplHelper {}
#[cfg(not(target_arch = "wasm32"))]
impl rustyline::Helper for ReplHelper {}

/// Cross-session REPL history file: `$HOME/.z42_history` (falls back to
/// `%USERPROFILE%` on Windows). `None` when neither is set — history then stays
/// in-process only. Kept dependency-free (no `dirs` crate).
/// (add-repl-history-keyword-completion)
#[cfg(not(target_arch = "wasm32"))]
fn history_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|h| std::path::PathBuf::from(h).join(".z42_history"))
}

/// Start index of the identifier word ending at `pos` (letters/digits/`_`); the
/// replacement span for a chosen candidate. **Excludes `.`** so a member candidate
/// (`WriteLine`, returned bare by the z42 completer) replaces only the post-`.` prefix
/// — not the whole `receiver.prefix`, which would wipe the receiver. Matches the z42
/// side `_wordStart` (Completer.z42), keeping both ends' replacement spans consistent.
/// (fix-repl-completion-span-and-index)
#[cfg(not(target_arch = "wasm32"))]
fn word_start(line: &str, pos: usize) -> usize {
    let bytes = line.as_bytes();
    let mut i = pos;
    while i > 0 {
        let c = bytes[i - 1];
        if c.is_ascii_alphanumeric() || c == b'_' {
            i -= 1;
        } else {
            break;
        }
    }
    i
}

// ── Line source: rustyline on host, plain stdin on wasm / when unavailable ──

#[cfg(not(target_arch = "wasm32"))]
fn read_one_line(ctx: &VmContext, prompt: &str, initial: &str) -> Result<Value> {
    use parking_lot::Mutex;
    use rustyline::error::ReadlineError;
    use rustyline::history::DefaultHistory;
    use rustyline::{CompletionType, Config, Editor};
    use std::sync::OnceLock;

    // One editor for the process → shared history + completer across calls. Lazily
    // created; if rustyline can't init (e.g. no tty), we fall back to plain stdin.
    static EDITOR: OnceLock<Mutex<Option<Editor<ReplHelper, DefaultHistory>>>> = OnceLock::new();
    let cell = EDITOR.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock();
    if guard.is_none() {
        // List completion (bash-style: first Tab → longest common prefix, next Tab →
        // list candidates). rustyline's default is `Circular`, which cycles through
        // candidates on each Tab and wraps back to the *original* input — read as
        // Tab "going backward" / losing text. (add-repl-completion-iter2)
        let config = Config::builder().completion_type(CompletionType::List).build();
        if let Ok(mut ed) = Editor::<ReplHelper, DefaultHistory>::with_config(config) {
            ed.set_helper(Some(ReplHelper::new()));
            // Load prior sessions' history (best-effort: missing file / parse error is
            // fine — a fresh session just starts empty). (add-repl-history-keyword-completion)
            if let Some(p) = history_path() {
                let _ = ed.load_history(&p);
            }
            *guard = Some(ed);
        }
    }
    match guard.as_mut() {
        Some(ed) => {
            // Publish the live ctx for the completer, strictly for this readline span.
            ACTIVE_CTX.with(|c| c.set(ctx as *const VmContext));
            // `initial` (non-empty on auto-indented continuation lines) pre-fills the
            // edit buffer with the cursor after it; empty → a normal fresh line.
            let res = if initial.is_empty() {
                ed.readline(prompt)
            } else {
                ed.readline_with_initial(prompt, (initial, ""))
            };
            ACTIVE_CTX.with(|c| c.set(std::ptr::null()));
            match res {
                Ok(line) => {
                    let _ = ed.add_history_entry(line.as_str());
                    // Persist after each line so history survives across sessions (and
                    // crashes). Best-effort: a write failure never breaks the REPL.
                    if let Some(p) = history_path() {
                        let _ = ed.save_history(&p);
                    }
                    Ok(Value::Str(line.into()))
                }
                Err(ReadlineError::Eof) | Err(ReadlineError::Interrupted) => Ok(Value::Null),
                Err(e) => bail!("__repl_readline: {e}"),
            }
        }
        None => plain_readline(prompt, initial),
    }
}

#[cfg(target_arch = "wasm32")]
fn read_one_line(_ctx: &VmContext, prompt: &str, initial: &str) -> Result<Value> {
    plain_readline(prompt, initial)
}

/// Fallback: print the prompt to stderr, read one line from stdin. EOF → null.
/// `initial` (auto-indent pre-fill) is ignored here: a non-interactive / no-tty
/// stream carries its own literal text and cannot host an editable pre-fill.
fn plain_readline(prompt: &str, _initial: &str) -> Result<Value> {
    use std::io::Write;
    let mut err = std::io::stderr();
    let _ = err.write_all(prompt.as_bytes());
    let _ = err.flush();
    let mut line = String::new();
    let n = std::io::stdin().read_line(&mut line)?;
    if n == 0 {
        return Ok(Value::Null); // EOF
    }
    Ok(Value::Str(line.trim_end_matches(['\n', '\r']).to_string().into()))
}

#[cfg(test)]
#[path = "repl_tests.rs"]
mod repl_tests;
