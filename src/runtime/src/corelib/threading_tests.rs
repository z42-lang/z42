//! Unit tests for `corelib/threading.rs` — validates the surface of
//! `__thread_spawn` / `__thread_join` without spawning real OS threads on
//! complex modules. End-to-end spawn+join is covered by
//! `runtime/tests/cross_thread_smoke.rs::spawn_via_builtin_then_join` (which
//! constructs a minimal real Function) and by the z42 stdlib tests in
//! `src/libraries/z42.threading/tests/`.
//!
//! Cargo unit-test scope here:
//! - Argument validation (wrong type / missing) → `Err`
//! - Spawn without `VmContext::with_module` → `Err`
//! - Join unknown / already-joined slot → discriminator 2 (no panic)
//! - Join arg validation → `Err`

use super::*;
use crate::metadata::Value;

/// Build a VmContext with no Module installed — `__thread_spawn` should
/// bail before touching the thread API.
fn no_module_ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

#[test]
fn thread_spawn_missing_arg_errors() {
    let ctx = no_module_ctx();
    let err = builtin_thread_spawn(&ctx, &[]).unwrap_err();
    assert!(err.to_string().contains("missing action"),
        "unexpected error: {err}");
}

#[test]
fn thread_spawn_non_callable_arg_errors() {
    let ctx = no_module_ctx();
    let err = builtin_thread_spawn(&ctx, &[Value::I64(7)]).unwrap_err();
    assert!(err.to_string().contains("expected callable"),
        "unexpected error: {err}");
}

#[test]
fn thread_spawn_without_module_errors() {
    // `VmContext::new()` constructs VmCore with `module = None`; spawn must
    // refuse rather than panic later from the worker.
    let ctx = no_module_ctx();
    let action = Value::FuncRef(crate::metadata::tokens::FnId(0));
    let err = builtin_thread_spawn(&ctx, &[action]).unwrap_err();
    assert!(err.to_string().contains("no shared Module"),
        "unexpected error: {err}");
}

fn join(ctx: &VmContext, args: &[Value]) -> anyhow::Result<Value> {
    crate::corelib::exec_builtin_value(ctx, "__thread_join", args)
}

fn typed_err(e: anyhow::Error) -> crate::corelib::abi::BuiltinError {
    e.downcast::<crate::corelib::abi::BuiltinError>().expect("a typed builtin error")
}

#[test]
fn thread_join_missing_arg_is_an_internal_error() {
    // Arity is fixed by the `#[builtin]` signature: a mismatch means the `[Native]`
    // declaration and the Rust builtin disagree — a VM bug, fatal when run.
    let ctx = no_module_ctx();
    let err = typed_err(join(&ctx, &[]).unwrap_err());
    assert!(matches!(&err, crate::corelib::abi::BuiltinError::Internal(m) if m.contains("takes 1 argument")),
        "unexpected error: {err}");
}

#[test]
fn thread_join_wrong_type_is_an_invalid_cast() {
    let ctx = no_module_ctx();
    let err = typed_err(join(&ctx, &[Value::Str("not an id".into())]).unwrap_err());
    assert!(matches!(&err, crate::corelib::abi::BuiltinError::InvalidCast(m) if m.contains("`slot` expects an integer")),
        "unexpected error: {err}");
}

fn discriminator(v: Value) -> Value {
    let Value::Array(rc) = v else { panic!("expected Value::Array, got {v:?}") };
    let b = rc.borrow();
    assert_eq!(b.len(), 1, "unknown-slot result is a 1-elem array");
    b.get_boxed(0)
}

#[test]
fn thread_join_unknown_slot_returns_discriminator_2() {
    let ctx = no_module_ctx();
    assert!(matches!(discriminator(join(&ctx, &[Value::I64(9_999)]).unwrap()), Value::I64(2)));
}

#[test]
fn thread_join_negative_slot_is_an_unknown_slot() {
    // Slot ids are allocated from 1 up; a negative one names no thread.
    let ctx = no_module_ctx();
    assert!(matches!(discriminator(join(&ctx, &[Value::I64(-1)]).unwrap()), Value::I64(2)));
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
