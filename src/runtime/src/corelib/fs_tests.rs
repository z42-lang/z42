use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

/// Extract a `Value::Array` of `Value::Str` into `Vec<String>`.
/// (Caller must bind the owning VmContext to a named local so the returned
/// array's GcRef outlives this call — see fix-crypto-tests-ctx-lifetime.)
fn extract_strings(v: Value) -> Vec<String> {
    match v {
        Value::Array(rc) => rc
            .borrow()
            .iter_boxed()
            .map(|e| match e {
                Value::Str(s) => s.to_string(),
                other => panic!("expected Value::Str, got {other:?}"),
            })
            .collect(),
        other => panic!("expected Value::Array, got {other:?}"),
    }
}

// add-z42-launcher (2026-06-02): __env_args returns the program arguments
// passed after `--` on the z42vm command line (stored on VmCore via
// set_program_args), NOT the VM's own raw process argv.

#[test]
fn env_args_empty_by_default() {
    let c = ctx();
    let r = builtin_env_args(&c, &[]).unwrap();
    assert!(extract_strings(r).is_empty(),
        "no `--` args set → GetCommandLineArgs() must be empty");
}

#[test]
fn env_args_returns_set_program_args_in_order() {
    let c = ctx();
    c.set_program_args(vec!["alpha".into(), "beta".into(), "gamma".into()]);
    let r = builtin_env_args(&c, &[]).unwrap();
    assert_eq!(extract_strings(r), vec!["alpha", "beta", "gamma"]);
}

#[test]
fn env_args_reflects_last_set() {
    let c = ctx();
    c.set_program_args(vec!["x".into()]);
    c.set_program_args(vec!["y".into(), "z".into()]);
    let r = builtin_env_args(&c, &[]).unwrap();
    assert_eq!(extract_strings(r), vec!["y", "z"]);
}
