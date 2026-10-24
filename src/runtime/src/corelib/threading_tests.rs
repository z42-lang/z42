//! Unit tests for `corelib/threading.rs` — validates the surface of
//! `__thread_spawn` / `__thread_wait` without spawning real OS threads on
//! complex modules. End-to-end spawn+join is covered by
//! `runtime/tests/cross_thread_smoke.rs::spawn_via_builtin_then_join` (which
//! constructs a minimal real Function) and by the z42 stdlib tests in
//! `src/libraries/z42.threading/tests/`.
//!
//! Cargo unit-test scope here:
//! - Argument validation (wrong type / missing) → `Err`
//! - Spawn without `VmContext::with_module` → `Err`

use super::*;
use crate::metadata::Value;

/// Build a VmContext with no Module installed — `__thread_spawn` should
/// bail before touching the thread API.
fn no_module_ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

fn spawn(c: &VmContext, args: &[Value]) -> crate::corelib::abi::BuiltinError {
    // Drive through the table so the tests also cover the typed registration.
    let e = crate::corelib::exec_builtin(c, "__thread_spawn", args).unwrap_err();
    e.downcast::<crate::corelib::abi::BuiltinError>().expect("a typed builtin error")
}

#[test]
fn thread_spawn_missing_arg_errors() {
    let ctx = no_module_ctx();
    let err = spawn(&ctx, &[]);
    // The arity check lives in the macro-generated wrapper.
    assert!(matches!(err, crate::corelib::abi::BuiltinError::Internal(ref m) if m.contains("argument")),
        "unexpected error: {err:?}");
}

#[test]
fn thread_spawn_non_callable_arg_errors() {
    let ctx = no_module_ctx();
    let err = spawn(&ctx, &[Value::I64(7)]);
    assert!(matches!(err, crate::corelib::abi::BuiltinError::Argument(ref m) if m.contains("expected callable")),
        "unexpected error: {err:?}");
}

#[test]
fn thread_spawn_without_module_errors() {
    // `VmContext::new()` constructs VmCore with `module = None`; spawn must
    // refuse rather than panic later from the worker.
    let ctx = no_module_ctx();
    let action = Value::FuncRef(crate::metadata::tokens::FnId(0));
    let err = spawn(&ctx, &[action]);
    assert!(matches!(err, crate::corelib::abi::BuiltinError::Internal(ref m) if m.contains("no shared Module")),
        "unexpected error: {err:?}");
}

fn typed_err(e: anyhow::Error) -> crate::corelib::abi::BuiltinError {
    e.downcast::<crate::corelib::abi::BuiltinError>().expect("a typed builtin error")
}

// `__thread_wait` (the throwing join the stdlib calls): an unknown / already-joined slot
// throws `Std.Threading.ThreadException` instead of returning a discriminator.
#[test]
fn thread_wait_unknown_slot_throws_thread_exception() {
    let ctx = no_module_ctx();
    let e = typed_err(crate::corelib::exec_builtin(&ctx, "__thread_wait", &[Value::I64(999_999)]).unwrap_err());
    match e {
        crate::corelib::abi::BuiltinError::Exception { class, message } => {
            assert_eq!(class, "Std.Threading.ThreadException");
            assert_eq!(message, "thread already joined");
        }
        other => panic!("{other:?}"),
    }
}
