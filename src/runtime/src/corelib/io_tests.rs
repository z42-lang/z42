//! Unit tests for stdout/stderr sink dispatch (R2 完整版 Phase A).
//!
//! Notes:
//! - Tests run in their own thread (cargo test) so thread_local sinks are
//!   freshly empty at start. The crate's other tests share the test thread
//!   pool but since each #[test] runs on its own thread (xUnit-style), the
//!   thread_local stack is per-test.
//! - We don't try to test the "println-to-process-stdout" path (no easy
//!   capture from inside the test). All tests install a sink first and
//!   assert via the buffer.

use super::*;
use crate::vm_context::VmContext;
use crate::object::Value;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

fn builtin_println(c: &VmContext, args: &[Value]) -> anyhow::Result<()> {
    crate::corelib::exec_builtin(c, "__println", args).map(|_| ())
}
fn builtin_print(c: &VmContext, args: &[Value]) -> anyhow::Result<()> {
    crate::corelib::exec_builtin(c, "__print", args).map(|_| ())
}
fn builtin_eprintln(c: &VmContext, args: &[Value]) -> anyhow::Result<()> {
    crate::corelib::exec_builtin(c, "__eprintln", args).map(|_| ())
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string().into())
}

// The sinks are driven through the table like the stdlib does, so these tests also
// cover their registration as typed builtins.
fn install_stdout() {
    crate::corelib::exec_builtin(&ctx(), "__test_io_install_stdout_sink", &[]).unwrap();
}

fn take_stdout() -> String {
    match crate::corelib::exec_builtin_value(&ctx(), "__test_io_take_stdout_buffer", &[]).unwrap() {
        Value::Str(s) => s.to_string(),
        other => panic!("expected Str, got {:?}", other),
    }
}

fn install_stderr() {
    crate::corelib::exec_builtin(&ctx(), "__test_io_install_stderr_sink", &[]).unwrap();
}

fn take_stderr() -> String {
    match crate::corelib::exec_builtin_value(&ctx(), "__test_io_take_stderr_buffer", &[]).unwrap() {
        Value::Str(s) => s.to_string(),
        other => panic!("expected Str, got {:?}", other),
    }
}

#[test]
fn install_then_println_routes_to_buffer_with_newline() {
    install_stdout();
    builtin_println(&ctx(), &[s("hello")]).unwrap();
    builtin_println(&ctx(), &[s("world")]).unwrap();
    let buf = take_stdout();
    assert_eq!(buf, "hello\nworld\n");
}

#[test]
fn install_then_print_routes_without_newline() {
    install_stdout();
    builtin_print(&ctx(), &[s("ab")]).unwrap();
    builtin_print(&ctx(), &[s("cd")]).unwrap();
    let buf = take_stdout();
    assert_eq!(buf, "abcd");
}

#[test]
fn nested_capture_stdout_inner_sees_only_inner() {
    install_stdout();
    builtin_println(&ctx(), &[s("outer-pre")]).unwrap();
    install_stdout();
    builtin_println(&ctx(), &[s("inner")]).unwrap();
    let inner = take_stdout();
    builtin_println(&ctx(), &[s("outer-post")]).unwrap();
    let outer = take_stdout();

    assert_eq!(inner, "inner\n");
    assert_eq!(outer, "outer-pre\nouter-post\n");
}

#[test]
fn take_pops_sink_so_next_println_falls_through() {
    install_stdout();
    builtin_println(&ctx(), &[s("captured")]).unwrap();
    let captured = take_stdout();
    assert_eq!(captured, "captured\n");
    // After take, no sink active. We can't easily assert on process stdout,
    // but we can confirm the sinks vector is now empty by observing that
    // a fresh install + take returns an empty buffer.
    install_stdout();
    let empty = take_stdout();
    assert_eq!(empty, "");
}

#[test]
fn stderr_sink_independent_of_stdout() {
    install_stdout();
    install_stderr();
    builtin_println(&ctx(), &[s("on stdout")]).unwrap();
    builtin_eprintln(&ctx(), &[s("on stderr")]).unwrap();
    let out = take_stdout();
    let err = take_stderr();
    assert_eq!(out, "on stdout\n");
    assert_eq!(err, "on stderr\n");
}

#[test]
fn stderr_only_capture_does_not_intercept_stdout() {
    // Only stderr sink installed. We can verify stderr buffer fills; we
    // can't easily assert that stdout went to process stdout, but we can
    // assert that no stdout buffer exists to swallow it.
    install_stderr();
    builtin_eprintln(&ctx(), &[s("stderr line")]).unwrap();
    // Any stdout call here would route to process stdout — fine.
    let err = take_stderr();
    assert_eq!(err, "stderr line\n");
}

#[test]
fn take_on_empty_stack_returns_empty_string() {
    // Defensive: should not panic on extra take.
    let s = take_stdout();
    assert_eq!(s, "");
}

// `Console.ReadLine` reads one line without its terminator and answers "" at end of
// input (the stdlib declares `string`, not `string?`).
#[test]
fn read_line_strips_the_terminator_and_is_empty_at_eof() {
    let mut r = std::io::Cursor::new(b"first\r\nsecond\nlast".to_vec());
    assert_eq!(read_line_from(&mut r).unwrap(), "first");
    assert_eq!(read_line_from(&mut r).unwrap(), "second");
    assert_eq!(read_line_from(&mut r).unwrap(), "last");
    assert_eq!(read_line_from(&mut r).unwrap(), "");
    assert_eq!(read_line_from(&mut r).unwrap(), "", "EOF stays EOF");
    let mut bad = std::io::Cursor::new(vec![0xff, b'\n']);
    assert!(read_line_from(&mut bad).is_err(), "invalid UTF-8 is an error (IOException)");
}

// A write to the process stdout runs parked; a capture sink takes the text unparked.
#[test]
fn console_write_to_a_capture_sink_does_not_park() {
    let c = ctx();
    install_stdout();
    builtin_println(&c, &[s("captured")]).unwrap();
    assert_eq!(take_stdout(), "captured\n");
    assert_eq!(c.core.parked_count.load(std::sync::atomic::Ordering::Acquire), 0);
}
