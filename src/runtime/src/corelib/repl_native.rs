//! Lazy dlopen loader + callback wiring for the host-only REPL editor cdylib
//! (`libz42_repl`, crate `z42-repl`). (extract-repl-native-cdylib)
//!
//! # Why a separate loader from `native/ext.rs`
//!
//! The compression ext loader (`native/ext.rs`) is **eager** (all `libz42_*` in
//! `<sdk>/native/` are dlopen'd at VM startup) and **one-directional** (VM → native).
//! The REPL editor is different on both axes:
//!   - **Lazy**: dlopen'd on the *first* `__repl_readline`, so a non-interactive run
//!     (script / test) never touches it and startup stays untouched.
//!   - **Bidirectional**: the editor calls *back* into the VM mid-`readline` (Tab
//!     completion, indent-aware keys) via the C function pointers in `ReplCallbacks`.
//!   - **Host-only toolchain, not `<sdk>/native/`**: it ships beside `z42i`/`z42vm`
//!     (SDK `bin/`, dev cargo target dir) and is found by *this* repl-specific probe,
//!     never `ext::native_search_paths()`. wasm/mobile never load it (plain fallback).
//!
//! # Boundary invariant
//!
//! No z42 type crosses the C boundary. The VM re-entrancy (`complete_trampoline`,
//! `keyedit_trampoline`) marshals to/from C strings on the VM side; the opaque
//! `*mut VmContext` is passed through `ReplCallbacks.ctx` verbatim and cast back
//! only inside the trampolines. Owned C strings the trampolines return are freed by
//! the cdylib via `ReplCallbacks.free_str` (= [`z42vm_free_str`]).

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;

/// Read one edited (possibly multi-line) statement, driving the dlopen'd editor
/// cdylib. Falls back to a plain stdin read when the lib is absent, the terminal
/// can't host an editor (`Z42_REPL_NO_EDITOR`), or `native-interop` is compiled out.
/// Return convention matches the old in-VM path: `Str` line / `Null` EOF / `Str("")`
/// interrupt; a genuine editor error bails.
pub fn readline(ctx: &VmContext, prompt: &str) -> Result<Value> {
    #[cfg(feature = "native-interop")]
    {
        native::readline(ctx, prompt)
    }
    #[cfg(not(feature = "native-interop"))]
    {
        let _ = ctx;
        super::repl::plain_readline(prompt)
    }
}

#[cfg(feature = "native-interop")]
mod native {
    use super::*;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::path::PathBuf;
    use std::sync::OnceLock;

    // ── Result kinds — MUST match `crates/z42-repl/src/lib.rs` ────────────────
    const Z42_REPL_LINE: i32 = 0;
    const Z42_REPL_EOF: i32 = 1;
    const Z42_REPL_INTERRUPT: i32 = 2;
    const Z42_REPL_ERROR: i32 = 3;
    const Z42_REPL_NO_EDITOR: i32 = 4;

    /// C callback table the z42vm side installs for one `z42_repl_readline` span.
    /// **MUST stay byte-identical to `crates/z42-repl/src/lib.rs` `ReplCallbacks`.**
    /// Version-locked: z42vm and libz42_repl ship from the same source tree, so a
    /// mismatch surfaces at packaging (both built together), not silently at runtime.
    #[repr(C)]
    struct ReplCallbacks {
        /// Opaque `*mut VmContext`, passed back to the trampolines verbatim.
        ctx: *mut c_void,
        complete: extern "C" fn(*mut c_void, *const c_char, usize) -> *mut c_char,
        key_edit: extern "C" fn(*mut c_void, *const c_char, *const c_char, usize) -> *mut c_char,
        free_str: extern "C" fn(*mut c_char),
    }

    // Entry signatures — MUST match the cdylib's `#[unsafe(no_mangle)]` exports.
    type ReadlineFn =
        unsafe extern "C" fn(*const c_char, *const ReplCallbacks, *mut i32) -> *mut c_char;
    type FreeFn = unsafe extern "C" fn(*mut c_char);
    type LastErrorFn = extern "C" fn() -> *const c_char;

    /// Resolved cdylib entries + the `Library` handle kept alive for the process so
    /// the fn ptrs stay valid.
    struct LoadedRepl {
        readline: ReadlineFn,
        free: FreeFn,
        last_error: LastErrorFn,
        _lib: libloading::Library,
    }

    /// Process-global, resolved on first `readline`. `Some` = loaded; `None` = no lib
    /// found / load failed → plain-stdin fallback (memoized so we probe the FS once).
    static LOADED: OnceLock<Option<LoadedRepl>> = OnceLock::new();

    fn loaded() -> Option<&'static LoadedRepl> {
        LOADED.get_or_init(load).as_ref()
    }

    /// Probe the repl-specific candidate paths (env override → sibling of the running
    /// binary) and dlopen the first that resolves the entry set. Never aborts — a
    /// missing lib just means the plain fallback.
    fn load() -> Option<LoadedRepl> {
        for path in candidates() {
            if !path.is_file() {
                continue;
            }
            // SAFETY: `path` names a version-locked libz42_repl built from this tree.
            match unsafe { try_load(&path) } {
                Ok(l) => {
                    tracing::debug!("repl: loaded editor cdylib from {}", path.display());
                    return Some(l);
                }
                Err(e) => tracing::warn!("repl: failed to load {}: {:#}", path.display(), e),
            }
        }
        tracing::debug!("repl: no libz42_repl found; using plain stdin fallback");
        None
    }

    unsafe fn try_load(path: &std::path::Path) -> Result<LoadedRepl> {
        let lib = libloading::Library::new(path)?;
        // Copy the fn ptrs out (Copy); keep `lib` alive in the struct so they resolve.
        let readline: ReadlineFn = *lib.get::<ReadlineFn>(b"z42_repl_readline")?;
        let free: FreeFn = *lib.get::<FreeFn>(b"z42_repl_free")?;
        let last_error: LastErrorFn = *lib.get::<LastErrorFn>(b"z42_repl_last_error")?;
        Ok(LoadedRepl { readline, free, last_error, _lib: lib })
    }

    /// Repl-specific search order — NOT `ext::native_search_paths()`:
    ///   1. `Z42_REPL_NATIVE` — explicit override (a full lib path, or a dir holding it).
    ///   2. the directory of the running binary (SDK `bin/` — the cdylib ships beside
    ///      `z42i`/`z42vm`, host-only toolchain, deliberately not `<sdk>/native/`; dev
    ///      cargo target dir where `cargo build -p z42-repl` also drops it).
    fn candidates() -> Vec<PathBuf> {
        let file = lib_filename();
        let mut out = Vec::new();
        if let Some(over) = std::env::var_os("Z42_REPL_NATIVE") {
            let p = PathBuf::from(over);
            // Accept either a direct file path or a directory containing the lib.
            out.push(p.join(&file));
            out.push(p);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                out.push(dir.join(&file));
            }
        }
        out
    }

    /// Platform lib filename: `libz42_repl.so` / `.dylib`, `z42_repl.dll` on Windows.
    fn lib_filename() -> String {
        format!("{}z42_repl{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX)
    }

    pub(super) fn readline(ctx: &VmContext, prompt: &str) -> Result<Value> {
        let lib = match loaded() {
            Some(l) => l,
            None => return super::super::repl::plain_readline(prompt),
        };
        let cbs = ReplCallbacks {
            ctx: ctx as *const VmContext as *mut c_void,
            complete: super::super::repl::complete_trampoline,
            key_edit: super::super::repl_editing::keyedit_trampoline,
            free_str: z42vm_free_str,
        };
        // A prompt with an interior NUL is nonsensical; fall back to empty rather than
        // error (the z42 prompt strings never contain NUL).
        let c_prompt = CString::new(prompt).unwrap_or_default();
        let mut out_kind: i32 = Z42_REPL_ERROR;
        // SAFETY: entries come from the version-locked cdylib; `cbs` (and the `ctx`
        // pointer within) stay valid for the synchronous duration of this call, and
        // the callbacks only re-enter on this thread.
        let raw = unsafe { (lib.readline)(c_prompt.as_ptr(), &cbs, &mut out_kind) };
        match out_kind {
            Z42_REPL_LINE => {
                let s = if raw.is_null() {
                    String::new()
                } else {
                    // SAFETY: on LINE the cdylib returns an owned C string.
                    unsafe { CStr::from_ptr(raw) }.to_string_lossy().into_owned()
                };
                if !raw.is_null() {
                    // SAFETY: hand the cdylib's buffer back to its own allocator once.
                    unsafe { (lib.free)(raw) };
                }
                Ok(Value::Str(s.into()))
            }
            Z42_REPL_EOF => Ok(Value::Null),
            // Ctrl-C abandons the buffer and re-prompts: empty line, not exit
            // (matches the old in-VM `ReadlineError::Interrupted` mapping).
            Z42_REPL_INTERRUPT => Ok(Value::Str(String::new().into())),
            // Editor couldn't initialize (no tty) → plain read, same as the old path
            // where a failed `Editor::with_config` dropped to `plain_readline`.
            Z42_REPL_NO_EDITOR => super::super::repl::plain_readline(prompt),
            _ => {
                let msg = last_error(lib);
                anyhow::bail!("__repl_readline: {msg}");
            }
        }
    }

    /// Read the cdylib's thread-local last-error string (set on `Z42_REPL_ERROR`).
    fn last_error(lib: &LoadedRepl) -> String {
        let ptr = (lib.last_error)();
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: valid until the next entry call on this thread; we copy it out now.
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }

    /// Release a string the trampolines (`complete_trampoline` / `keyedit_trampoline`)
    /// returned to the cdylib. Installed as `ReplCallbacks.free_str`.
    ///
    /// # Safety
    /// `s` must be null or a pointer produced by `CString::into_raw` in one of the
    /// trampolines (which it always is — the cdylib only frees what they handed it).
    pub(super) extern "C" fn z42vm_free_str(s: *mut c_char) {
        if !s.is_null() {
            drop(unsafe { CString::from_raw(s) });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn free_str_frees_owned_and_ignores_null() {
            // A pointer from `CString::into_raw` (as the trampolines produce) is
            // reclaimed without leak/double-free; null is a no-op.
            let owned = CString::new("candidate").unwrap().into_raw();
            z42vm_free_str(owned);
            z42vm_free_str(std::ptr::null_mut());
        }

        #[test]
        fn lib_filename_is_platform_shaped() {
            let f = lib_filename();
            assert!(f.contains("z42_repl"));
            // libz42_repl.dylib / .so, or z42_repl.dll on Windows.
            assert!(f.ends_with(std::env::consts::DLL_SUFFIX));
        }
    }
}
