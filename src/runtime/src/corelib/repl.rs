//! REPL line editor builtins — back `Std.Repl.ReadLine` used by the native
//! interactive REPL (`z42i`).
//!
//! `__repl_readline(prompt, initial)` → one edited line (history, emacs keys,
//! Ctrl-D EOF). Multi-line accumulation, completeness judgment, AND the
//! continuation-indent computation all live in the script layer
//! (sink-repl-indent-to-script): this builtin is a plain "read one edited line"
//! primitive — no bracket state machine remains Rust-side.
//!
//! # Line editor: dlopen'd cdylib, not in-VM (extract-repl-native-cdylib)
//!
//! The rustyline-backed editor was extracted into a host-only toolchain cdylib
//! (`crates/z42-repl` → `libz42_repl.{so,dylib,dll}`). This module keeps only the
//! **VM-side** pieces:
//!   - the `__repl_*` builtins the z42 stdlib calls,
//!   - the re-entrancy core `complete_via_callback` (VM → z42 completer), and
//!   - `complete_trampoline` — the `extern "C"` shim the cdylib calls back through
//!     to fetch completion candidates for the word under the cursor.
//! The lazy dlopen, callback wiring, and plain-stdin fallback live in the sibling
//! `repl_native` module; the editor itself (rustyline `Completer`/`Hinter`/keys)
//! lives in the cdylib. No z42 type (`Value`/`VmContext`) crosses the C boundary —
//! candidates cross as a `\n`-joined C string.
//!
//! Return convention: `Value::Str` for a line/block; `Value::Null` on EOF
//! (Ctrl-D) so the z42 side can treat null as "exit". Ctrl-C (interrupt) returns
//! `Value::Str("")` — distinct from EOF. An empty line is `Value::Str("")` too.
//!
//! Host-only: the cdylib backs the editor on host targets; wasm32 falls back to a
//! plain stdin read so the builtins still resolve. The REPL itself is host-only
//! (scripting-charter 2b).

use crate::corelib::reflection::{builtin_type_members, make_type_from_name};
use crate::interp::{exec_function, ExecOutcome};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
use std::ffi::{c_char, c_void, CStr, CString};

/// Extract argument `idx` as an owned prompt string (empty when absent/non-str).
fn prompt_arg(args: &[Value], idx: usize) -> String {
    match args.get(idx) {
        Some(Value::Str(s)) => s.to_string(),
        _ => String::new(),
    }
}

/// `__repl_readline(prompt: string, initial: string) -> string?` — read one edited
/// whole (possibly multi-line) statement — one `readline()` spans an entire statement,
/// with the Enter key deciding submit-vs-continue via the z42 key-editor
/// (add-repl-multiline-editing). Returns null on Ctrl-D (EOF) / Ctrl-C (interrupt).
pub fn builtin_repl_readline(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let prompt = prompt_arg(args, 0);
    // add-repl-prewarm: GC-safe park for the blocking read so a background
    // prewarm thread's GC can proceed while this thread waits on stdin. The
    // cdylib's callbacks (completer / key-editor) un-park via
    // `NativeUnparkGuard` in the trampolines before re-entering the VM.
    let _park = crate::gc::NativeParkGuard::enter(ctx);
    read_one_line(ctx, &prompt)
}

/// `__repl_set_completer(fqn: string) -> void` — register the z42 completer the Tab
/// key invokes (signature `string[] complete(string line, int pos)`). Empty string
/// clears it. Process-global; the cdylib's `Completer` reads it (via
/// `complete_trampoline`) on each Tab. D5 spike (add-completion-query-api, risk B).
pub fn builtin_repl_set_completer(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fqn = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => bail!("__repl_set_completer: arg 0 must be the completer's fully-qualified name (string)"),
    };
    let slot = REGISTERED_COMPLETER.get_or_init(|| parking_lot::Mutex::new(None));
    *slot.lock() = if fqn.is_empty() { None } else { Some(fqn) };
    Ok(Value::Null)
}

/// Registered completer FQN (set by `__repl_set_completer`); read by
/// `complete_trampoline` on Tab. Process-global (one REPL per process).
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
/// This is the callback core the cdylib `Completer` (risk B) reuses via
/// `complete_trampoline`; here it is exposed as a direct builtin so a piped z42
/// program can verify risk A (builtin → z42 callback with args → string[] back)
/// end-to-end on the real VM.
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
/// `string[]` result Value. Reused by the probe (risk A) and the cdylib `Completer`
/// (risk B, via `complete_trampoline`). A `throw` inside the completer propagates
/// with its original type via `ctx.set_pending_thrown`, same convention as
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

/// `extern "C"` completion trampoline the cdylib calls back through
/// `ReplCallbacks.complete` mid-`readline()`. Casts the opaque `ctx` back to
/// `&VmContext`, un-parks GC (the outer `NativeParkGuard` from `builtin_repl_readline`
/// is active for the blocking read), invokes the registered z42 completer via
/// `complete_via_callback`, and returns the candidates as an owned `\n`-joined C
/// string (the cdylib splits on `\n`). Returns null on no-editor / no candidates /
/// error — preserving the completer's "a throw becomes a silent no-op" semantics.
/// The returned string is freed by the cdylib via `ReplCallbacks.free_str`
/// (= `repl_native::z42vm_free_str`).
///
/// # Safety
/// `ctx` must be the live `*mut VmContext` the z42vm side installed for this
/// `readline` span; `line` a valid NUL-terminated C string. Both hold on the same
/// thread for the duration of the call (same window/thread invariant as the old
/// `ACTIVE_CTX`).
#[cfg(all(not(target_arch = "wasm32"), feature = "native-interop"))]
pub(crate) extern "C" fn complete_trampoline(
    ctx: *mut c_void,
    line: *const c_char,
    pos: usize,
) -> *mut c_char {
    if ctx.is_null() || line.is_null() {
        return std::ptr::null_mut();
    }
    let fqn = match REGISTERED_COMPLETER.get().and_then(|m| m.lock().clone()) {
        Some(f) => f,
        None => return std::ptr::null_mut(),
    };
    // SAFETY: `ctx` is the live `&VmContext` installed for this readline span; the
    // trampoline only runs on that thread during the span.
    let vmctx: &VmContext = unsafe { &*(ctx as *const VmContext) };
    let line_str = unsafe { CStr::from_ptr(line) }.to_string_lossy().into_owned();
    // Fires mid-`readline`, inside the outer NativeParkGuard; temporarily un-park so
    // the completer runs as a normal mutator (parking at its own safepoints if a
    // background GC is requested); re-parks on drop before returning to the read.
    let _unpark = crate::gc::NativeUnparkGuard::exit(vmctx);
    let line_val = Value::Str(line_str.into());
    let cands: Vec<String> = match complete_via_callback(vmctx, &fqn, line_val, pos as i64) {
        Ok(Value::Array(a)) => a
            .borrow()
            .iter_boxed()
            .filter_map(|v| match v {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        _ => return std::ptr::null_mut(),
    };
    if cands.is_empty() {
        return std::ptr::null_mut();
    }
    // Candidates are identifiers (no `\n`, no interior NUL); join for the C boundary.
    match CString::new(cands.join("\n")) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
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
            if let Value::Str(s) = rc.borrow().field_value(i) {
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

// ── Line source: cdylib editor on host, plain stdin on wasm / when unavailable ──

/// Host: drive the dlopen'd `libz42_repl` editor (lazy load; plain-stdin fallback
/// when the lib is absent or the terminal can't host an editor). See `repl_native`.
#[cfg(not(target_arch = "wasm32"))]
fn read_one_line(ctx: &VmContext, prompt: &str) -> Result<Value> {
    super::repl_native::readline(ctx, prompt)
}

#[cfg(target_arch = "wasm32")]
fn read_one_line(_ctx: &VmContext, prompt: &str) -> Result<Value> {
    plain_readline(prompt)
}

/// Fallback: print the prompt to stderr, read one physical line from stdin. EOF → null.
/// No line editing here (non-interactive / no-tty). Whole-buffer multi-line editing is a
/// tty-only feature; a piped stream still works because the z42 loop accumulates lines and
/// asks `Completeness.IsIncomplete` when to stop. (add-repl-multiline-editing)
pub(crate) fn plain_readline(prompt: &str) -> Result<Value> {
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
