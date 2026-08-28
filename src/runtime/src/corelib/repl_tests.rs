//! Unit tests for `complete_trampoline` — the `extern "C"` shim the REPL editor
//! cdylib calls back through for Tab completion (extract-repl-native-cdylib).
//!
//! The re-entrancy core (`complete_via_callback` running a real z42 completer) is
//! exercised by the `__repl_complete_probe` path + its z42 golden test; here we pin
//! the trampoline's **defensive contract**: null args and a failing callback must
//! yield a null pointer (→ the editor performs the key's default), never a crash —
//! preserving the "a throw in the completer becomes a silent no-op" semantics.

use super::*;
use crate::vm_context::VmContext;
use std::ffi::CString;

/// Serialize tests that mutate the process-global `REGISTERED_COMPLETER`.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn complete_trampoline_null_ctx_returns_null() {
    let line = CString::new("Con").unwrap();
    // Null ctx is rejected by the leading guard before any global / deref.
    let r = complete_trampoline(std::ptr::null_mut(), line.as_ptr(), 3);
    assert!(r.is_null());
}

#[test]
fn complete_trampoline_null_line_returns_null() {
    let ctx = VmContext::new();
    let ctx_ptr = &*ctx as *const VmContext as *mut c_void;
    let r = complete_trampoline(ctx_ptr, std::ptr::null(), 0);
    assert!(r.is_null());
}

#[test]
fn complete_trampoline_no_completer_registered_returns_null() {
    let _g = SERIAL.lock().unwrap();
    let ctx = VmContext::new();
    // Ensure unset, then a non-null call still returns null (no completer FQN).
    builtin_repl_set_completer(&ctx, &[Value::Str("".into())]).unwrap();
    let ctx_ptr = &*ctx as *const VmContext as *mut c_void;
    let line = CString::new("Con").unwrap();
    let r = complete_trampoline(ctx_ptr, line.as_ptr(), 3);
    assert!(r.is_null());
}

#[test]
fn complete_trampoline_callback_error_swallowed_to_null() {
    let _g = SERIAL.lock().unwrap();
    let ctx = VmContext::new();
    // Register a completer so we pass the "no FQN" gate and actually re-enter; a
    // fresh ctx has `core.module == None`, so `complete_via_callback` errors — the
    // trampoline must swallow that to null, not propagate/panic.
    builtin_repl_set_completer(&ctx, &[Value::Str("Repl.NoSuchCompleter".into())]).unwrap();
    let ctx_ptr = &*ctx as *const VmContext as *mut c_void;
    let line = CString::new("Con").unwrap();
    // Mirror `builtin_repl_readline`: park around the (would-be blocking) read; the
    // trampoline unparks/reparks internally, so `parked_count` stays balanced.
    let park = crate::gc::NativeParkGuard::enter(&ctx);
    let r = complete_trampoline(ctx_ptr, line.as_ptr(), 3);
    drop(park);
    assert!(r.is_null(), "module=None → callback errors → null, never crash");
    builtin_repl_set_completer(&ctx, &[Value::Str("".into())]).unwrap();
}
