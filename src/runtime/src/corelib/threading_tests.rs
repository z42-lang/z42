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
fn thread_spawn_stack_closure_rejected() {
    let ctx = no_module_ctx();
    let bad = Value::StackClosure(Box::new(crate::metadata::StackClosureData { env_idx: 0, fn_name: "nope".into() }));
    let err = builtin_thread_spawn(&ctx, &[bad]).unwrap_err();
    assert!(err.to_string().contains("stack-allocated closure"),
        "unexpected error: {err}");
}

#[test]
fn thread_spawn_without_module_errors() {
    // `VmContext::new()` constructs VmCore with `module = None`; spawn must
    // refuse rather than panic later from the worker.
    let ctx = no_module_ctx();
    let action = Value::FuncRef("Anything.DoesNotMatter".into());
    let err = builtin_thread_spawn(&ctx, &[action]).unwrap_err();
    assert!(err.to_string().contains("no shared Module"),
        "unexpected error: {err}");
}

#[test]
fn thread_join_missing_arg_errors() {
    let ctx = no_module_ctx();
    let err = builtin_thread_join(&ctx, &[]).unwrap_err();
    assert!(err.to_string().contains("missing slot id"),
        "unexpected error: {err}");
}

#[test]
fn thread_join_wrong_type_errors() {
    let ctx = no_module_ctx();
    let err = builtin_thread_join(&ctx, &[Value::Str("not an id".into())]).unwrap_err();
    assert!(err.to_string().contains("expected i64"),
        "unexpected error: {err}");
}

#[test]
fn thread_join_unknown_slot_returns_discriminator_2() {
    let ctx = no_module_ctx();
    let result = builtin_thread_join(&ctx, &[Value::I64(9_999)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        other => panic!("expected Value::Array, got {other:?}"),
    };
    let borrowed = arr.borrow();
    assert_eq!(borrowed.len(), 1, "unknown-slot result is a 1-elem array");
    assert!(matches!(borrowed.get_boxed(0), Value::I64(2)),
        "discriminator should be 2 (unknown slot), got {:?}", borrowed.get_boxed(0));
}

#[test]
fn thread_join_negative_slot_errors() {
    // i64 → u64 cast guard: negative slot IDs are nonsensical.
    let ctx = no_module_ctx();
    let err = builtin_thread_join(&ctx, &[Value::I64(-1)]).unwrap_err();
    assert!(err.to_string().contains("expected i64 slot id"),
        "unexpected error: {err}");
}
