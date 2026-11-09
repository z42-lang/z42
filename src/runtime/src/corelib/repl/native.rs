//! Lazy dlopen loader + callback wiring for the host-only REPL editor cdylib
//! (`libz42_repl`, crate `z42-repl`). (extract-repl-native-cdylib)
//!
//! # Why a separate loader from `interop/ext/mod.rs`
//!
//! The standard-library loader resolves one-way compression calls from SDK
//! native paths. The REPL editor opens on its first `__repl_readline` and calls
//! back into the VM for completion and editing. It uses the shared colocated
//! library resolver to find the editor beside the interactive apphost payload
//! (or beside `z42vm` in a development build). wasm and mobile use the plain fallback.
//!
//! # Boundary invariant
//!
//! No z42 type crosses the C boundary. The VM re-entrancy (`complete_trampoline`,
//! `keyedit_trampoline`) marshals to/from C strings on the VM side; the opaque
//! `*mut VmContext` is passed through `ReplCallbacks.ctx` verbatim and cast back
//! only inside the trampolines. Owned C strings the trampolines return are freed by
//! the cdylib via `ReplCallbacks.free_str` (= [`z42vm_free_str`]).

use crate::vm::context::VmContext;
use anyhow::Result;

/// Read one edited (possibly multi-line) statement, driving the dlopen'd editor
/// cdylib. Falls back to a plain stdin read when the lib is absent, the terminal
/// can't host an editor (`Z42_REPL_NO_EDITOR`), or `native-interop` is compiled out.
/// Return convention matches the old in-VM path: `Str` line / `Null` EOF / `Str("")`
/// interrupt; a genuine editor error bails.
/// `None` is EOF. Runs inside the caller's `NativeParkGuard`, so it must not allocate on
/// the GC heap — the caller turns the line into a `Value` after the park ends.
pub fn readline(ctx: &VmContext, prompt: &str) -> Result<Option<String>> {
    #[cfg(feature = "native-interop")]
    {
        native::readline(ctx, prompt)
    }
    #[cfg(not(feature = "native-interop"))]
    {
        let _ = ctx;
        super::plain_readline(prompt)
    }
}

#[cfg(feature = "native-interop")]
pub(super) mod native {
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
    type SetKeywordsFn = unsafe extern "C" fn(*const c_char);

    /// Resolved cdylib entries + the `Library` handle kept alive for the process so
    /// the fn ptrs stay valid.
    struct LoadedRepl {
        readline: ReadlineFn,
        free: FreeFn,
        last_error: LastErrorFn,
        /// **可选**：语法着色的关键字注入口。老版本 cdylib 没有这个符号——
        /// 拿不到就 `None`，REPL 照常工作、只是输入行不着色。硬绑定会让
        /// 新 VM 配旧 cdylib 时整个 REPL 退化成 plain-stdin，代价不成比例。
        set_keywords: Option<SetKeywordsFn>,
        _lib: crate::pal::dynlib::Library,
    }

    /// Process-global, resolved on the first keywords or readline request. `Some` = loaded; `None` = no lib
    /// found / load failed → plain-stdin fallback (memoized so we probe the FS once).
    static LOADED: OnceLock<Option<LoadedRepl>> = OnceLock::new();

    fn loaded(ctx: &VmContext) -> Result<Option<&'static LoadedRepl>> {
        if let Some(value) = LOADED.get() { return Ok(value.as_ref()); }
        let paths = super::super::paths::source(ctx)?;
        Ok(loaded_from(&LOADED, paths))
    }

    fn loaded_from(cache: &OnceLock<Option<LoadedRepl>>, paths: impl FnOnce() -> Vec<PathBuf>) -> Option<&LoadedRepl> {
        cache.get_or_init(|| load(paths())).as_ref()
    }

    /// 把 z42 侧的关键字表（`\n` 分隔）交给编辑器 cdylib 做语法着色。
    /// 没加载到 cdylib、或 cdylib 太老没有这个符号时静默 no-op。
    pub(in crate::corelib) fn set_keywords(ctx: &VmContext, joined: &str) -> Result<()> {
        let Some(l) = loaded(ctx)? else { return Ok(()) };
        let Some(f) = l.set_keywords else { return Ok(()) };
        let Ok(c) = std::ffi::CString::new(joined) else { return Ok(()) };
        // SAFETY: `f` 来自本进程持有的 Library（`_lib` 保活）；`c` 在调用期间存活，
        // 且 cdylib 侧只读取、不保留该指针（它把内容拷进自己的关键字集）。
        unsafe { f(c.as_ptr()) };
        Ok(())
    }

    /// Probe the repl-specific candidate paths (env override → sibling of the running
    /// binary) and dlopen the first that resolves the entry set. Never aborts — a
    /// missing lib just means the plain fallback.
    fn load(paths: Vec<PathBuf>) -> Option<LoadedRepl> {
        for path in paths {
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
        let lib = crate::pal::dynlib::Library::open(path)?;
        // Copy the fn ptrs out (Copy); keep `lib` alive in the struct so they resolve.
        let readline: ReadlineFn = lib.symbol("z42_repl_readline")?;
        let free: FreeFn = lib.symbol("z42_repl_free")?;
        let last_error: LastErrorFn = lib.symbol("z42_repl_last_error")?;
        // 可选符号：缺失不是错误（见 LoadedRepl::set_keywords 的注释）。
        let set_keywords: Option<SetKeywordsFn> = lib.symbol("z42_repl_set_keywords").ok();
        if set_keywords.is_none() {
            tracing::debug!("repl: cdylib has no z42_repl_set_keywords; input line will not be colored");
        }
        Ok(LoadedRepl { readline, free, last_error, set_keywords, _lib: lib })
    }

    pub(super) fn readline(ctx: &VmContext, prompt: &str) -> Result<Option<String>> {
        let lib = match loaded(ctx)? {
            Some(l) => l,
            None => return super::super::plain_readline(prompt),
        };
        let cbs = ReplCallbacks {
            ctx: ctx as *const VmContext as *mut c_void,
            complete: super::super::complete_trampoline,
            key_edit: super::super::editing::keyedit_trampoline,
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
                Ok(Some(s))
            }
            Z42_REPL_EOF => Ok(None),
            // Ctrl-C abandons the buffer and re-prompts: empty line, not exit
            // (matches the old in-VM `ReadlineError::Interrupted` mapping).
            Z42_REPL_INTERRUPT => Ok(Some(String::new())),
            // Editor couldn't initialize (no tty) → plain read, same as the old path
            // where a failed `Editor::with_config` dropped to `plain_readline`.
            Z42_REPL_NO_EDITOR => super::super::plain_readline(prompt),
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
        include!("native_cache_tests.rs");
    }
}
