//! Unit tests of the typed process builtins (`process::typed`): results, the .NET-shaped
//! exceptions thrown from Rust, and parking while a call waits on the child.
//!
//! POSIX coreutils assumed (`sh` / `true` / `cat` / `sleep`).

use super::*;
use crate::corelib::abi::{BuiltinError, FieldInit};
use crate::corelib::{exec_builtin, exec_builtin_value};

fn s(v: &str) -> Value { Value::Str(v.into()) }
fn i(n: i64) -> Value { Value::I64(n) }

fn strs(ctx: &VmContext, xs: &[&str]) -> Value {
    ctx.heap().alloc_array(xs.iter().map(|x| s(x)).collect())
}

/// `__process_run_sync` arguments: stdin Null, stdout / stderr Pipe, no timeout.
fn run_args(ctx: &VmContext, program: &str, argv: &[&str]) -> Vec<Value> {
    vec![
        s(program), strs(ctx, argv), strs(ctx, &[]), strs(ctx, &[]), strs(ctx, &[]), Value::Bool(false),
        Value::Null, i(STDIO_NULL), Value::Null, i(STDIO_PIPE), Value::Null, i(STDIO_PIPE), Value::Null,
        i(-1), Value::Bool(true),
    ]
}

/// `__process_start` arguments (no one-shot stdin, no timeout / process-group flag).
fn start_args(ctx: &VmContext, program: &str, argv: &[&str], stdin_mode: i64) -> Vec<Value> {
    vec![
        s(program), strs(ctx, argv), strs(ctx, &[]), strs(ctx, &[]), strs(ctx, &[]), Value::Bool(false),
        Value::Null, i(stdin_mode), i(STDIO_PIPE), Value::Null, i(STDIO_PIPE), Value::Null,
    ]
}

fn fail(ctx: &VmContext, name: &str, args: &[Value]) -> BuiltinError {
    let e = exec_builtin_value(ctx, name, args).expect_err(name);
    match e.downcast::<BuiltinError>() {
        Ok(b) => b,
        Err(e) => panic!("{name}: not a typed error: {e}"),
    }
}

fn at(v: &Value, k: usize) -> Value {
    let Value::Array(a) = v else { panic!("expected an array, got {v:?}") };
    a.borrow().get_boxed(k)
}

fn start(ctx: &VmContext, program: &str, argv: &[&str], stdin_mode: i64) -> i64 {
    match exec_builtin_value(ctx, "__process_start", &start_args(ctx, program, argv, stdin_mode)).unwrap() {
        Value::I64(slot) => slot,
        other => panic!("__process_start returned {other:?}"),
    }
}

#[test]
fn run_sync_returns_exit_code_and_both_streams() {
    let ctx = VmContext::new();
    let r = exec_builtin_value(&ctx, "__process_run_sync", &run_args(&ctx, "sh", &["-c", "printf out; printf err >&2; exit 3"])).unwrap();
    assert_eq!(at(&r, 0), i(3));
    assert_eq!(at(&r, 1), s("out"));
    assert_eq!(at(&r, 2), s("err"));
    let Value::Array(bytes) = at(&r, 3) else { panic!() };
    assert_eq!(bytes.borrow().byte_snapshot().unwrap(), b"out".to_vec(), "stdout bytes are a packed byte[]");
}

#[test]
fn start_failure_is_a_process_start_exception() {
    let ctx = VmContext::new();
    for name in ["__process_run_sync", "__process_start"] {
        let args = if name == "__process_start" {
            start_args(&ctx, "z42-no-such-binary-p18", &[], STDIO_NULL)
        } else {
            run_args(&ctx, "z42-no-such-binary-p18", &[])
        };
        let e = fail(&ctx, name, &args);
        assert_eq!(e.classes(), ["Std.IO.ProcessStartException", "Std.IO.IOException"], "{name}");
        assert!(e.message().starts_with("z42-no-such-binary-p18: "), "{name}: {}", e.message());
        assert!(e.message().contains("(kind: NotFound)"), "{name}: {}", e.message());
    }
}

#[test]
fn run_timeout_is_a_process_timeout_exception_with_its_fields() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "sh", &["-c", "sleep 5"]);
    args[13] = i(150);
    let t0 = std::time::Instant::now();
    let BuiltinError::Detailed(d) = fail(&ctx, "__process_run_sync", &args) else { panic!("not detailed") };
    assert!(t0.elapsed() < std::time::Duration::from_secs(3), "the tree is killed on timeout");
    assert_eq!(d.classes, ["Std.IO.ProcessTimeoutException", "Std.TimeoutException"]);
    assert_eq!(d.message, "process `sh` did not finish within 150ms");
    assert!(matches!(&d.fields[..], [("Program", FieldInit::Str(p)), ("TimeoutMs", FieldInit::I64(150))] if p == "sh"));
}

#[test]
fn a_consumed_or_unknown_handle_is_handle_invalid() {
    let ctx = VmContext::new();
    let slot = start(&ctx, "true", &[], STDIO_NULL);
    let r = exec_builtin_value(&ctx, "__process_wait", &[i(slot)]).unwrap();
    assert_eq!(at(&r, 0), i(0));
    for name in ["__process_wait", "__process_pid", "__process_try_wait", "__process_stdin_close"] {
        let e = fail(&ctx, name, &[i(slot)]);
        assert_eq!(e.classes(), ["Std.IO.ProcessHandleInvalidException", "Std.InvalidOperationException"], "{name}");
    }
    exec_builtin(&ctx, "__process_close", &[i(slot)]).unwrap();   // idempotent
}

#[test]
fn stdin_write_without_a_pipe_is_invalid_operation_and_keeps_the_handle() {
    let ctx = VmContext::new();
    let slot = start(&ctx, "true", &[], STDIO_NULL);
    let bytes = ctx.heap().alloc_bytes(b"x".to_vec());
    assert!(matches!(fail(&ctx, "__process_stdin_write", &[i(slot), bytes]), BuiltinError::InvalidOperation(_)));
    assert!(matches!(exec_builtin_value(&ctx, "__process_pid", &[i(slot)]).unwrap(), Value::I64(p) if p > 0));
    let r = exec_builtin_value(&ctx, "__process_wait", &[i(slot)]).unwrap();
    assert_eq!(at(&r, 0), i(0));
}

#[test]
fn malformed_bundles_are_argument_errors() {
    let ctx = VmContext::new();
    let mut args = run_args(&ctx, "true", &[]);
    args[7] = i(STDIO_FILE);
    assert!(matches!(fail(&ctx, "__process_run_sync", &args), BuiltinError::NotSupported(_)), "Stdio.ToFile on stdin");
    let mut args = run_args(&ctx, "true", &[]);
    args[9] = i(9);
    assert!(matches!(fail(&ctx, "__process_run_sync", &args), BuiltinError::ArgumentOutOfRange(_)));
    let mut args = run_args(&ctx, "true", &[]);
    args[2] = strs(&ctx, &["K"]);
    assert!(matches!(fail(&ctx, "__process_run_sync", &args), BuiltinError::Argument(_)));
    let mut args = run_args(&ctx, "echo", &["x"]);
    args[9] = i(STDIO_FILE);
    args[10] = s("/z42-no-such-dir-p18/out.txt");
    assert!(matches!(fail(&ctx, "__process_run_sync", &args), BuiltinError::DirectoryNotFound(_)));
}

#[test]
fn streaming_reads_until_eof_then_try_wait() {
    let ctx = VmContext::new();
    let slot = start(&ctx, "sh", &["-c", "printf abcdef"], STDIO_NULL);
    let buf = ctx.heap().alloc_bytes(vec![0; 8]);
    let mut got = Vec::new();
    loop {
        let Value::I64(n) = exec_builtin_value(&ctx, "__process_stdout_read", &[i(slot), buf, i(2), i(4)]).unwrap()
            else { panic!() };
        if n == 0 { break; }
        let Value::Array(a) = buf else { panic!() };
        got.extend(a.borrow().byte_range(2, n as usize).unwrap());
    }
    assert_eq!(got, b"abcdef");
    let mut r = Value::Null;
    for _ in 0..500 {
        r = exec_builtin_value(&ctx, "__process_try_wait", &[i(slot)]).unwrap();
        if r != Value::Null { break; }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(at(&r, 0), i(0));
}

fn parked(ctx: &VmContext) -> usize {
    ctx.core.parked_count.load(std::sync::atomic::Ordering::Acquire) as usize
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < std::time::Duration::from_secs(5) {
        if cond() { return true; }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cond()
}

/// A thread waiting on a child (`Wait`, and a whole `Run`) is parked: another thread can
/// run a full collection meanwhile.
#[test]
fn a_collection_completes_while_a_thread_waits_on_a_child() {
    for name in ["__process_wait", "__process_run_sync"] {
        let ctx = VmContext::new();
        let slot = start(&ctx, "sleep", &["1"], STDIO_NULL);
        let core = std::sync::Arc::clone(&ctx.core);
        let waiter = std::thread::spawn(move || {
            let w = VmContext::new_with_core(core);
            let r = if name == "__process_wait" {
                exec_builtin_value(&w, name, &[i(slot)]).unwrap()
            } else {
                exec_builtin_value(&w, name, &run_args(&w, "sleep", &["1"])).unwrap()
            };
            match at(&r, 0) { Value::I64(code) => code, other => panic!("{other:?}") }
        });
        assert!(wait_until(|| parked(&ctx) == 1), "{name}: the waiting thread is parked");
        let t0 = std::time::Instant::now();
        {
            let pause = crate::gc::safepoint::request_gc_pause(&ctx);
            assert!(pause.is_some(), "{name}: this thread becomes the collector");
            ctx.heap().collect_full();
        }
        assert!(t0.elapsed() < std::time::Duration::from_millis(700),
            "{name}: the collection did not wait for the child ({:?})", t0.elapsed());
        assert_eq!(waiter.join().unwrap(), 0);
        assert_eq!(parked(&ctx), 0, "{name}: the park is released");
        exec_builtin(&ctx, "__process_close", &[i(slot)]).unwrap();
    }
}
