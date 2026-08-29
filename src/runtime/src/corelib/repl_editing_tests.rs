//! Unit tests for `keyedit_trampoline` — the `extern "C"` shim the REPL editor
//! cdylib calls back through on each controlled key (extract-repl-native-cdylib).
//!
//! The action→`Cmd` translation now lives in the cdylib (`editing.rs` tests cover
//! it). Here we pin the VM-side trampoline's **defensive contract**: null args and
//! a failing key-editor callback yield a null pointer (→ the key's default), never
//! a crash — same "throw becomes a no-op" semantics as the completer trampoline.

use super::*;
use crate::vm_context::VmContext;
use std::ffi::CString;

/// Serialize tests that mutate the process-global `REGISTERED_KEY_EDITOR`.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn keyedit_trampoline_null_ctx_returns_null() {
    let key = CString::new("enter").unwrap();
    let line = CString::new("if x {").unwrap();
    let r = keyedit_trampoline(std::ptr::null_mut(), key.as_ptr(), line.as_ptr(), 6);
    assert!(r.is_null());
}

#[test]
fn keyedit_trampoline_null_key_or_line_returns_null() {
    let ctx = VmContext::new();
    let ctx_ptr = &*ctx as *const VmContext as *mut c_void;
    let line = CString::new("if x {").unwrap();
    assert!(keyedit_trampoline(ctx_ptr, std::ptr::null(), line.as_ptr(), 6).is_null());
    let key = CString::new("enter").unwrap();
    assert!(keyedit_trampoline(ctx_ptr, key.as_ptr(), std::ptr::null(), 0).is_null());
}

#[test]
fn keyedit_trampoline_no_editor_registered_returns_null() {
    let _g = SERIAL.lock().unwrap();
    let ctx = VmContext::new();
    builtin_repl_set_key_editor(&ctx, &[Value::Str("".into())]).unwrap();
    let ctx_ptr = &*ctx as *const VmContext as *mut c_void;
    let key = CString::new("enter").unwrap();
    let line = CString::new("if x {").unwrap();
    let r = keyedit_trampoline(ctx_ptr, key.as_ptr(), line.as_ptr(), 6);
    assert!(r.is_null());
}

#[test]
fn keyedit_trampoline_callback_error_swallowed_to_null() {
    let _g = SERIAL.lock().unwrap();
    let ctx = VmContext::new();
    // Registered editor + fresh ctx (module == None) → `key_edit_via_callback`
    // errors; the trampoline must swallow it to null (perform the key's default).
    builtin_repl_set_key_editor(&ctx, &[Value::Str("Repl.NoSuchEditor".into())]).unwrap();
    let ctx_ptr = &*ctx as *const VmContext as *mut c_void;
    let key = CString::new("enter").unwrap();
    let line = CString::new("if x {").unwrap();
    let park = crate::gc::NativeParkGuard::enter(&ctx);
    let r = keyedit_trampoline(ctx_ptr, key.as_ptr(), line.as_ptr(), 6);
    drop(park);
    assert!(r.is_null(), "module=None → callback errors → null, never crash");
    builtin_repl_set_key_editor(&ctx, &[Value::Str("".into())]).unwrap();
}
