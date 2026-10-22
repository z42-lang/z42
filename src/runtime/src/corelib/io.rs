use crate::corelib::abi::Blocking;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::cell::{Cell, RefCell};
use std::os::raw::{c_char, c_void};
use std::sync::RwLock;
use super::convert::value_to_str;

// ── R2 完整版 — TestIO sink dispatch ───────────────────────────────────────
//
// thread-local stack of capture buffers. install_*_sink pushes a fresh Vec;
// take_*_buffer pops the top and returns it as a String. While a sink is
// active, builtin_println / builtin_print / builtin_eprintln / builtin_eprint
// route to the top buffer instead of the process stdio.
//
// Stack semantics let nested TestIO.captureStdout calls work intuitively
// (inner capture sees inner output only; outer sees outer pre + post but
// not inner). See 变更 extend-z42-test-library scenario 5.

thread_local! {
    static STDOUT_SINKS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
    static STDERR_SINKS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };

    // Embedding-API hookup (add-embedding-api H2 ):
    // when a host application wraps an `interp::run_*` call inside its
    // `z42_host_invoke` entry, it sets this flag for the duration of the
    // call. While set, `route_stdout` / `route_stderr` consult the
    // process-global host sinks (`HOST_STDOUT_SINK` / `HOST_STDERR_SINK`)
    // first; they fall back to the test-IO stack otherwise. This keeps
    // host-driven invocation isolated from concurrent test-IO captures
    // running on other threads.
    static HOST_SINK_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

/// Process-global stdout / stderr sink installed by the embedding API.
/// `None` → no host sink configured (the default).
///
/// Spec: docs/internals/src/runtime/embedding.md §8 (stdout / stderr 重定向).
#[derive(Clone, Copy)]
pub struct HostSink {
    pub callback: unsafe extern "C" fn(bytes: *const c_char, length: usize, user_data: *mut c_void),
    pub user_data: *mut c_void,
}

// `user_data` is opaque to the runtime; the host alone owns its lifetime.
// The runtime never dereferences it beyond passing it to the callback.
unsafe impl Send for HostSink {}
unsafe impl Sync for HostSink {}

static HOST_STDOUT_SINK: RwLock<Option<HostSink>> = RwLock::new(None);
static HOST_STDERR_SINK: RwLock<Option<HostSink>> = RwLock::new(None);

/// Install the process-global stdout sink. Returns the previous sink (if
/// any) so callers can restore it on shutdown.
pub fn install_host_stdout_sink(sink: Option<HostSink>) -> Option<HostSink> {
    let mut guard = match HOST_STDOUT_SINK.write() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    std::mem::replace(&mut *guard, sink)
}

/// Install the process-global stderr sink. Returns the previous sink.
pub fn install_host_stderr_sink(sink: Option<HostSink>) -> Option<HostSink> {
    let mut guard = match HOST_STDERR_SINK.write() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    std::mem::replace(&mut *guard, sink)
}

/// Activate host-sink dispatch on the current thread. Returns the prior
/// flag value so callers can restore it (paired enter / leave in
/// `z42_host_invoke`).
pub fn host_sink_set_active(on: bool) -> bool {
    HOST_SINK_ACTIVE.with(|c| {
        let prev = c.get();
        c.set(on);
        prev
    })
}

fn dispatch_host_sink(
    text: &str,
    append_newline: bool,
    slot: &RwLock<Option<HostSink>>,
) -> bool {
    // Copy the sink out and drop the guard before calling it: the callback
    // may re-enter the host API (`shutdown` reinstalls the slot under the
    // write lock).
    let host = match slot.read() {
        Ok(g) => match *g {
            Some(h) => h,
            None => return false,
        },
        Err(_) => return false,
    };
    if append_newline {
        let mut combined = String::with_capacity(text.len() + 1);
        combined.push_str(text);
        combined.push('\n');
        unsafe {
            (host.callback)(
                combined.as_ptr() as *const c_char,
                combined.len(),
                host.user_data,
            );
        }
    } else {
        unsafe {
            (host.callback)(
                text.as_ptr() as *const c_char,
                text.len(),
                host.user_data,
            );
        }
    }
    true
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Route a `Console.Write*` text. A capture sink — the embedding host's callback (when
/// active on this thread) or the innermost TestIO buffer — takes it right here, unparked:
/// a buffer append does not block, and the host callback may re-enter the host API (so it
/// must not run parked). Only a write to the process's own stdout / stderr can block — a
/// pipe whose reader stalls fills up — and that part is returned as a [`Blocking`] write,
/// run parked.
fn route(stream: Stream, text: String, append_newline: bool) -> Blocking<()> {
    let (host, sinks) = match stream {
        Stream::Stdout => (&HOST_STDOUT_SINK, &STDOUT_SINKS),
        Stream::Stderr => (&HOST_STDERR_SINK, &STDERR_SINKS),
    };
    if HOST_SINK_ACTIVE.with(|c| c.get()) && dispatch_host_sink(&text, append_newline, host) {
        return Blocking::ready(());
    }
    let captured = sinks.with(|sinks| match sinks.borrow_mut().last_mut() {
        Some(top) => {
            top.extend_from_slice(text.as_bytes());
            if append_newline { top.push(b'\n'); }
            true
        }
        None => false,
    });
    if captured {
        return Blocking::ready(());
    }
    Blocking::new(move || {
        match (stream, append_newline) {
            (Stream::Stdout, true)  => println!("{text}"),
            (Stream::Stdout, false) => print!("{text}"),
            (Stream::Stderr, true)  => eprintln!("{text}"),
            (Stream::Stderr, false) => eprint!("{text}"),
        }
        Ok(())
    })
}

/// One line from `r` without its line terminator; `""` at end of input (the
/// `Console.ReadLine` shape the stdlib declares — `string`, not `string?`).
fn read_line_from(r: &mut impl std::io::BufRead) -> std::io::Result<String> {
    let mut line = String::new();
    r.read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}

/// `Console` builtins (builtin ABI v2).
///
/// `Write` / `WriteLine` stringify their argument through the user's `ToString` (z42
/// code: `reenters`), then route it ([`route`]): the process-stream write runs parked.
/// `ReadLine` waits for input for as long as the user likes — the whole read is parked.
#[z42_macros::builtins]
pub mod typed {
    use super::{route, Stream};
    use crate::corelib::abi::{Blocking, BuiltinError};
    use crate::metadata::Value;
    use crate::vm_context::VmContext;

    /// dispatch-tostring-in-native-stringify: the same string interpolation / `ToString()`
    /// produce — a user `ToString` is dispatched (boxed structs, record synthesis, enum
    /// member names), the short type name only without one.
    fn stringify(ctx: &VmContext, value: &Value) -> Result<String, BuiltinError> {
        crate::interp::dispatch::stringify_dispatch(ctx, value).map_err(BuiltinError::Legacy)
    }

    #[builtin(name = "__println", reenters)]
    pub fn println(ctx: &VmContext, value: &Value) -> Result<Blocking<()>, BuiltinError> {
        Ok(route(Stream::Stdout, stringify(ctx, value)?, true))
    }

    #[builtin(name = "__print", reenters)]
    pub fn print(ctx: &VmContext, value: &Value) -> Result<Blocking<()>, BuiltinError> {
        Ok(route(Stream::Stdout, stringify(ctx, value)?, false))
    }

    #[builtin(name = "__eprintln", reenters)]
    pub fn eprintln(ctx: &VmContext, value: &Value) -> Result<Blocking<()>, BuiltinError> {
        Ok(route(Stream::Stderr, stringify(ctx, value)?, true))
    }

    #[builtin(name = "__eprint", reenters)]
    pub fn eprint(ctx: &VmContext, value: &Value) -> Result<Blocking<()>, BuiltinError> {
        Ok(route(Stream::Stderr, stringify(ctx, value)?, false))
    }

    /// `Console.ReadLine()` — one line without its terminator; `""` at end of input.
    #[builtin(name = "__readline", blocking)]
    pub fn readline() -> Result<String, BuiltinError> {
        super::read_line_from(&mut std::io::stdin().lock()).map_err(BuiltinError::from)
    }
}

pub fn builtin_test_io_install_stdout_sink(_ctx: &VmContext, _: &[Value]) -> Result<()> {
    push_stdout_sink();
    Ok(())
}

pub fn builtin_test_io_take_stdout_buffer(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let bytes = take_stdout_sink();
    Ok(Value::Str(String::from_utf8_lossy(&bytes).into_owned().into()))
}

/// bench-stats-in-process-capture : Rust-side push of a
/// stdout-capture sink. Mirror of `builtin_test_io_install_stdout_sink`
/// for callers (test-runner) that don't have a z42 VmContext to invoke
/// the builtin through.
///
/// While installed, any subsequent `println` / `print` from z42 code on
/// this thread routes into the buffer instead of process stdout. Stack
/// semantics: pushing while one is already active nests — the inner
/// captures only its own output.
pub fn push_stdout_sink() {
    STDOUT_SINKS.with(|s| s.borrow_mut().push(Vec::new()));
}

/// bench-stats-in-process-capture : Rust-side pop of the
/// most-recent stdout sink. Returns the captured bytes (empty Vec when
/// no sink was installed). Caller may re-emit to process stdout if it
/// wants the user to still see the output in their terminal.
pub fn take_stdout_sink() -> Vec<u8> {
    STDOUT_SINKS.with(|s| s.borrow_mut().pop().unwrap_or_default())
}

pub fn builtin_test_io_install_stderr_sink(_ctx: &VmContext, _: &[Value]) -> Result<()> {
    STDERR_SINKS.with(|s| s.borrow_mut().push(Vec::new()));
    Ok(())
}

pub fn builtin_test_io_take_stderr_buffer(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let bytes = STDERR_SINKS.with(|s| s.borrow_mut().pop().unwrap_or_default());
    Ok(Value::Str(String::from_utf8_lossy(&bytes).into_owned().into()))
}

pub fn builtin_concat(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let a = args.first().map(value_to_str).unwrap_or_default();
    let b = args.get(1).map(value_to_str).unwrap_or_default();
    Ok(Value::Str(format!("{}{}", a, b).into()))
}

pub fn builtin_len(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::Array(rc)) => Ok(Value::I64(rc.borrow().len() as i64)),
        Some(Value::Str(s))    => Ok(Value::I64(s.len() as i64)),
        Some(other)            => bail!("__len: expected array or string, got {:?}", other),
        None                   => bail!("__len: missing argument"),
    }
}

pub fn builtin_contains(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::Str(s)) => {
            let needle = super::convert::arg_str(args, 1, "__contains")?;
            Ok(Value::Bool(s.contains(needle)))
        }
        Some(Value::Array(arr)) => {
            // split-null-sentinel-channels ⑥：此前缺参时会去**搜索 Null**。而
            // 「在数组里找 null」本身是合法操作（`list.Contains(null)`）—— 正因如此
            // 「少传参数」与「显式传 null」必须靠 arity 校验才分得开。
            // 字符串那一支早就经 `arg_str(args, 1, ..)` 校验了，这一支漏了。
            crate::corelib::expect_args("Contains", args, 2)?;
            let item = args[1].clone();
            Ok(Value::Bool(arr.borrow().iter_boxed().any(|v| v == item)))
        }
        _ => bail!("Contains: first argument must be a string or List"),
    }
}

#[cfg(test)]
#[path = "io_tests.rs"]
mod io_tests;
