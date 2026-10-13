//! `Std.IO.Process` builtins — cross-platform command execution (builtin ABI v2).
//!
//! All Windows / macOS / Linux differences (PATH lookup, PATHEXT, CreateProcess vs
//! fork-exec, signals vs TerminateProcess) are delegated to `std::process::Command`.
//!
//! ## Architecture
//!
//! z42 `Process` is a pure facade: builder methods accumulate fields, `Run()` / `Spawn()`
//! make a single `[Native]` call that decodes the field bundle ([`CommandSpec`]) into a
//! `Command`. Live children launched by `Spawn` are kept in `VmCore.processes` keyed by a
//! monotonic slot id; the z42 `ProcessHandle` holds only that id.
//!
//! - **Failures throw from Rust** (the `typed` module) — no `[KIND, …]` arrays. The
//!   mapping is .NET-shaped; `Process.Start`'s `Win32Exception` does not exist in z42, so
//!   `ProcessStartException` takes its place under `IOException`:
//!
//!   | failure | exception (fallback when `z42.io` is not loaded) |
//!   |---|---|
//!   | the child could not be started (missing executable / cwd, permission, fork / exec) | `Std.IO.ProcessStartException : IOException` |
//!   | `Run()` exceeded `Timeout(ms)` (the child was killed and reaped) | `Std.IO.ProcessTimeoutException : TimeoutException`, `Program` / `TimeoutMs` set |
//!   | a handle already waited / disposed, or an unknown slot | `Std.IO.ProcessHandleInvalidException : InvalidOperationException` |
//!   | writing stdin that is not `Stdio.Pipe()` (or was closed) | `InvalidOperationException` (.NET: `StandardIn has not been redirected.`) |
//!   | `Stdio.ToFile` on stdin | `NotSupportedException` |
//!   | a `Stdio.ToFile` target cannot be created | the file-system mapping (`fs::fs_error`) |
//!   | an I/O error on a pipe (a dead child's stdin: `EPIPE`) | `IOException` |
//!   | a malformed field bundle (mode out of range, env keys / values of different length) | `ArgumentOutOfRangeException` / `ArgumentException` |
//!
//! - **Every call that waits on the child runs parked** — spawn, the one-shot stdin write,
//!   wait, drain, streaming reads and writes, the reap in `Dispose`. Pipes have their own
//!   locks ([`Pipe`]): a call clones the pipe out under the table lock and does the I/O
//!   after dropping it.
//!
//! The older builtins (`__process_run` / `__process_spawn` / `__process_handle_*`, which
//! return discriminated arrays) live in `process_legacy.rs` — the published seed's stdlib
//! still calls them. They share the helpers here.

use crate::corelib::abi::{BuiltinError, CallResult, FieldInit, IntoRet, PlainRet, RetKind};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};

pub(crate) const STDIO_NULL:    i64 = 0;
pub(crate) const STDIO_INHERIT: i64 = 1;
pub(crate) const STDIO_PIPE:    i64 = 2;
pub(crate) const STDIO_FILE:    i64 = 3;

/// One live spawned child plus optionally-captured stdio handles.
///
/// `child` is `Option` so methods that move the child out can leave the slot in a
/// consumed state; the next operation observes `None` and reports an invalid handle.
///
/// **fix-park-blocking-natives (2026-09-14)**: each pipe has its own lock. A streaming
/// read / stdin write blocks for as long as the child likes, and it must do so parked and
/// *outside* the process-table lock — any thread waiting on that shared lock is unparked,
/// so one blocked read would stall every collection. Callers clone the `Arc` under the
/// table lock, drop it, then lock the pipe parked.
pub struct ProcessSlot {
    /// The program the slot was spawned from (exception messages / `Program` fields).
    pub program:       String,
    pub child:         Option<Child>,
    pub stdin_writer:  Option<Pipe<ChildStdin>>,
    pub stdout_reader: Option<Pipe<ChildStdout>>,
    pub stderr_reader: Option<Pipe<ChildStderr>>,
    /// Total wall-clock timeout for run paths; `None` for spawned children (timeouts
    /// apply only to the synchronous Run).
    pub timeout:       Option<std::time::Duration>,
}

pub type Pipe<T> = std::sync::Arc<parking_lot::Mutex<T>>;

pub(crate) fn pipe<T>(end: Option<T>) -> Option<Pipe<T>> {
    end.map(|e| std::sync::Arc::new(parking_lot::Mutex::new(e)))
}

impl ProcessSlot {
    /// A freshly spawned child, its pipes moved into their own locks.
    pub(crate) fn spawned(program: String, mut child: Child) -> Self {
        ProcessSlot {
            program,
            stdin_writer:  pipe(child.stdin.take()),
            stdout_reader: pipe(child.stdout.take()),
            stderr_reader: pipe(child.stderr.take()),
            child:         Some(child),
            timeout:       None,
        }
    }
}

// ── the command ──────────────────────────────────────────────────────────

/// The field bundle a z42 `Process` builder passes (`Process.Run` / `Spawn`).
pub(crate) struct CommandSpec {
    pub program:     String,
    pub argv:        Vec<String>,
    pub env_keys:    Vec<String>,
    pub env_vals:    Vec<String>,
    pub env_remove:  Vec<String>,
    pub env_clear:   bool,
    pub cwd:         Option<String>,
    pub stdin_mode:  i64,
    pub stdout_mode: i64,
    pub stdout_path: Option<String>,
    pub stderr_mode: i64,
    pub stderr_path: Option<String>,
}

impl CommandSpec {
    /// The `Command`. Opens `Stdio.ToFile` targets — file I/O, so callers run it parked.
    pub(crate) fn command(&self) -> Result<Command, BuiltinError> {
        if self.env_keys.len() != self.env_vals.len() {
            return Err(BuiltinError::Argument(format!(
                "env keys / values length mismatch ({} vs {})", self.env_keys.len(), self.env_vals.len())));
        }
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.argv);
        if self.env_clear { cmd.env_clear(); }
        for (k, v) in self.env_keys.iter().zip(self.env_vals.iter()) {
            cmd.env(k, v);
        }
        for k in &self.env_remove {
            cmd.env_remove(k);
        }
        if let Some(d) = &self.cwd {
            cmd.current_dir(d);
        }
        cmd.stdin(stdio_for_input(self.stdin_mode)?);
        cmd.stdout(stdio_for_output(self.stdout_mode, self.stdout_path.as_deref(), "stdout")?);
        cmd.stderr(stdio_for_output(self.stderr_mode, self.stderr_path.as_deref(), "stderr")?);
        Ok(cmd)
    }
}

fn mode_out_of_range(which: &str, mode: i64) -> BuiltinError {
    BuiltinError::ArgumentOutOfRange(format!(
        "{which} mode {mode} is out of range (0 = Null, 1 = Inherit, 2 = Pipe, 3 = ToFile)"))
}

fn stdio_for_input(mode: i64) -> Result<Stdio, BuiltinError> {
    match mode {
        STDIO_NULL    => Ok(Stdio::null()),
        STDIO_INHERIT => Ok(Stdio::inherit()),
        STDIO_PIPE    => Ok(Stdio::piped()),
        STDIO_FILE    => Err(BuiltinError::NotSupported(
            "Stdio.ToFile is not supported for stdin; read the file and pass it with StdinBytes".into())),
        _ => Err(mode_out_of_range("stdin", mode)),
    }
}

fn stdio_for_output(mode: i64, path: Option<&str>, which: &str) -> Result<Stdio, BuiltinError> {
    match mode {
        STDIO_NULL    => Ok(Stdio::null()),
        STDIO_INHERIT => Ok(Stdio::inherit()),
        STDIO_PIPE    => Ok(Stdio::piped()),
        STDIO_FILE    => {
            let p = path.ok_or_else(|| BuiltinError::ArgumentNull(format!(
                "Value cannot be null. (Parameter '{which}Path')")))?;
            std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(p)
                .map(Stdio::from)
                .map_err(|e| super::fs::fs_error(e, p, super::fs::PathRole::Create))
        }
        _ => Err(mode_out_of_range(which, mode)),
    }
}

// ── outcomes ─────────────────────────────────────────────────────────────

/// Unix: killed by signal → `code()` is `None`; encode as 128 + signal (the sh convention).
pub(crate) fn exit_code_of(status: ExitStatus) -> i64 {
    match status.code() {
        Some(c) => c as i64,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                status.signal().map(|s| 128 + (s as i64)).unwrap_or(-1)
            }
            #[cfg(not(unix))]
            { -1 }
        }
    }
}

/// An exited child's status and captured output — plain data, built into the result
/// array `[exitCode, stdout, stderr, stdoutBytes, stderrBytes]` only after unparking.
pub struct RunOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl IntoRet for RunOutput {
    const KIND: RetKind = RetKind::Any;
    fn into_ret(self, ctx: &VmContext) -> CallResult {
        let out = Value::Str(String::from_utf8_lossy(&self.stdout).into_owned().into());
        let err = Value::Str(String::from_utf8_lossy(&self.stderr).into_owned().into());
        let out_bytes = ctx.heap().alloc_bytes(self.stdout);
        let err_bytes = ctx.heap().alloc_bytes(self.stderr);
        let code = Value::I64(exit_code_of(self.status));
        Ok(Some(ctx.heap().alloc_array(vec![code, out, err, out_bytes, err_bytes])))
    }
}
impl PlainRet for RunOutput {}

/// `TryWait`: `None` = still running (→ null).
pub struct MaybeExited(pub Option<RunOutput>);

impl IntoRet for MaybeExited {
    const KIND: RetKind = RetKind::Any;
    fn into_ret(self, ctx: &VmContext) -> CallResult {
        match self.0 {
            None => Ok(Some(Value::Null)),
            Some(out) => out.into_ret(ctx),
        }
    }
}
impl PlainRet for MaybeExited {}

/// Why a synchronous run produced no output.
pub(crate) enum RunFail {
    /// `Command::spawn` failed — the child never ran.
    Start(std::io::Error),
    /// The timeout fired; the child was killed and reaped.
    TimedOut,
    Other(BuiltinError),
}

impl From<BuiltinError> for RunFail {
    fn from(e: BuiltinError) -> Self { RunFail::Other(e) }
}

/// The start-failure text: `<program>: <OS error> (kind: <ErrorKind>)`.
pub(crate) fn start_message(program: &str, e: &std::io::Error) -> String {
    format!("{program}: {e} (kind: {:?})", e.kind())
}

pub(crate) fn start_error(program: &str, e: &std::io::Error) -> BuiltinError {
    BuiltinError::detailed(
        &["Std.IO.ProcessStartException", "Std.IO.IOException"], start_message(program, e), vec![])
}

pub(crate) fn timeout_error(program: &str, timeout_ms: i64) -> BuiltinError {
    BuiltinError::detailed(
        &["Std.IO.ProcessTimeoutException", "Std.TimeoutException"],
        format!("process `{program}` did not finish within {timeout_ms}ms"),
        vec![("Program", FieldInit::Str(program.to_string())), ("TimeoutMs", FieldInit::I64(timeout_ms))])
}

/// The handle has no child any more (already waited / disposed), or the slot is unknown.
pub(crate) fn handle_invalid(program: Option<&str>, slot: i64) -> BuiltinError {
    let (message, fields) = match program {
        Some(p) => (format!("process handle for `{p}` is no longer valid (already waited, killed, or disposed)"),
                    vec![("Program", FieldInit::Str(p.to_string()))]),
        None => (format!("process handle #{slot} is no longer valid (already waited, killed, or disposed)"), vec![]),
    };
    BuiltinError::detailed(
        &["Std.IO.ProcessHandleInvalidException", "Std.InvalidOperationException"], message, fields)
}

// ── running ──────────────────────────────────────────────────────────────

/// Synchronous run: spawn, feed the optional one-shot stdin payload, wait (with the
/// optional timeout), collect piped output. Blocks for as long as the child runs — the
/// caller is parked.
pub(crate) fn run_to_end(
    spec: &CommandSpec, stdin_bytes: Option<Vec<u8>>, timeout_ms: i64, own_process_group: bool,
) -> Result<RunOutput, RunFail> {
    let mut cmd = spec.command()?;
    // Put the child in its own process group (pgid == child pid) so a run-timeout
    // can kill the WHOLE tree, not just the direct child. Without this, `sh -c
    // "sleep 5"` that forks a `sleep` grandchild leaves the orphaned grandchild
    // holding the inherited stdout/stderr pipe write-ends open — the reader-thread
    // join below then blocks until the grandchild exits naturally.
    //
    // Opt-out (own_process_group == false): keep the child in the caller's process
    // group. Required for an interactive passthrough child that must control the tty
    // — a fresh process group is a *background* group for the controlling terminal, so
    // the child can't grab the tty (line editor fails) and its tty reads stop on SIGTTIN.
    // There is never a run-timeout on an interactive Inherit-stdin child, so the
    // tree-kill rationale above does not apply.
    #[cfg(unix)]
    if own_process_group {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(RunFail::Start)?;

    // Optional one-shot stdin payload (Pipe mode + .StdinBytes / .StdinString).
    if let Some(bytes) = stdin_bytes {
        if let Some(mut sin) = child.stdin.take() {
            sin.write_all(&bytes).map_err(|e| RunFail::Other(e.into()))?;
            drop(sin); // EOF for the child
        }
    }

    // Concurrent reader threads drain piped stdout / stderr so a chatty
    // child can't deadlock on a full pipe while we poll wait().
    let stdout_h = child.stdout.take().map(|r| std::thread::spawn(move || read_all(r)));
    let stderr_h = child.stderr.take().map(|r| std::thread::spawn(move || read_all(r)));

    let (status, timed_out) = wait_with_optional_timeout(&mut child, timeout_ms, own_process_group)
        .map_err(|e| RunFail::Other(e.into()))?;
    let stdout = stdout_h.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    let stderr = stderr_h.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    if timed_out {
        return Err(RunFail::TimedOut);
    }
    Ok(RunOutput { status, stdout, stderr })
}

fn read_all(mut r: impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = r.read_to_end(&mut buf);
    buf
}

/// Poll-loop wait — `timeout_ms < 0` means "no timeout, block forever".
/// On timeout the child is `kill`'d and the post-kill `wait` is reaped
/// to release zombie state; the returned `bool` reports whether we hit
/// the timeout.
///
/// 20ms poll interval is a compromise between latency and CPU: a
/// 5-second timeout polls 250 times; a 50ms timeout still sees the
/// child within one tick. No condvar / signal: stdlib-only path keeps
/// the implementation portable.
pub(crate) fn wait_with_optional_timeout(
    child: &mut Child, timeout_ms: i64, own_process_group: bool,
) -> std::io::Result<(ExitStatus, bool)> {
    if timeout_ms < 0 {
        return Ok((child.wait()?, false));
    }
    let timeout = std::time::Duration::from_millis(timeout_ms as u64);
    let start   = std::time::Instant::now();
    loop {
        if let Some(s) = child.try_wait()? { return Ok((s, false)); }
        if start.elapsed() >= timeout {
            kill_process_tree(child, own_process_group);
            let status = child.wait()?;
            return Ok((status, true));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// SIGKILL the child, plus (when it owns its process group) the whole group so a
/// forked-grandchild tree (e.g. `sh -c "sleep 5"`) is reaped too. `own_process_group`
/// is the same flag that gated `process_group(0)` at spawn: only when the child was
/// put in its OWN group (pgid == child pid) is `kill(-pid)` safe — otherwise a
/// negative-pid kill would target the *caller's* group.
fn kill_process_tree(child: &mut Child, own_process_group: bool) {
    #[cfg(unix)]
    if own_process_group {
        // Safe: kill(2) on a negative pid targets the process group; a stale/no-such
        // group just returns ESRCH, which we ignore (the child.kill() below covers it).
        let pid = child.id() as i32;
        unsafe { libc::kill(-pid, libc::SIGKILL); }
    }
    #[cfg(not(unix))]
    let _ = own_process_group;
    let _ = child.kill();
}

/// Wait for a spawned child and drain its pipes. Both pipes piped → read them on
/// background threads while waiting (a chatty child cannot deadlock on a full pipe);
/// otherwise read after the wait. Blocks until every writer closes the pipes (an exited
/// child's grandchildren may still hold them), so callers run it parked.
pub(crate) fn wait_and_drain(
    mut child: Child, stdout: Option<Pipe<ChildStdout>>, stderr: Option<Pipe<ChildStderr>>,
) -> Result<RunOutput, BuiltinError> {
    match (stdout, stderr) {
        (Some(o), Some(e)) => {
            let h_o = std::thread::spawn(move || read_all(&mut *o.lock()));
            let h_e = std::thread::spawn(move || read_all(&mut *e.lock()));
            let status = child.wait()?;
            Ok(RunOutput { status, stdout: h_o.join().unwrap_or_default(), stderr: h_e.join().unwrap_or_default() })
        }
        (o, e) => {
            let status = child.wait()?;
            let (stdout, stderr) = drain_readers(o, e)?;
            Ok(RunOutput { status, stdout, stderr })
        }
    }
}

/// Drain the stdout / stderr readers into byte vecs (after the child exited).
pub(crate) fn drain_readers(
    stdout: Option<Pipe<ChildStdout>>, stderr: Option<Pipe<ChildStderr>>,
) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::new();
    let mut err = Vec::new();
    if let Some(r) = stdout { r.lock().read_to_end(&mut out)?; }
    if let Some(r) = stderr { r.lock().read_to_end(&mut err)?; }
    Ok((out, err))
}

/// Read up to `count` bytes of a child's stdout / stderr into `buffer[offset..]`: the pipe
/// is cloned out under the table lock, read parked, copied into the `byte[]` unparked.
/// A pipe that was never piped (or was drained by a wait) reads as EOF.
fn read_pipe<R: Read + Send + 'static>(
    ctx: &VmContext, slot: i64, buffer: Value, offset: i64, count: i64,
    pick: fn(&ProcessSlot) -> Option<Pipe<R>>,
) -> Result<crate::corelib::abi::Then<i64>, BuiltinError> {
    use crate::corelib::abi::Blocking;
    let arr = super::fs::byte_array(&buffer, "buffer")?;
    let (off, n) = super::fs::check_buffer_range(offset, count, arr.borrow().len())?;
    let reader = ctx.with_process_slot(slot as u64, |s| pick(s)).ok_or_else(|| handle_invalid(None, slot))?;
    let read = match reader {
        Some(r) if n > 0 => Blocking::new(move || {
            let mut tmp = vec![0u8; n];
            let got = r.lock().read(&mut tmp)?;
            tmp.truncate(got);
            Ok(tmp)
        }),
        _ => Blocking::ready(Vec::new()),
    };
    Ok(read.then(move |_ctx: &VmContext, bytes: Vec<u8>| {
        super::fs::write_byte_range(&arr.borrow(), off, &bytes);
        Ok(bytes.len() as i64)
    }))
}

/// The process builtins (builtin ABI v2): failures throw (module docs), waits run parked.
#[z42_macros::builtins]
pub mod typed {
    use super::{handle_invalid, start_error, timeout_error, CommandSpec, MaybeExited, ProcessSlot, RunFail, RunOutput};
    use crate::corelib::abi::{Blocking, BuiltinError, Then};
    use crate::metadata::Value;
    use crate::vm_context::VmContext;
    use std::io::Write;

    /// `Process.Run()`: spawn, feed, wait, collect — the whole call parked. Returns
    /// `[exitCode, stdout, stderr, stdoutBytes, stderrBytes]`.
    #[allow(clippy::too_many_arguments)]
    #[builtin(name = "__process_run_sync", blocking)]
    pub fn process_run_sync(
        program: String, args: Vec<String>, env_keys: Vec<String>, env_vals: Vec<String>,
        env_remove: Vec<String>, env_clear: bool, cwd: Option<String>, stdin_mode: i64,
        stdin_bytes: Option<Vec<u8>>, stdout_mode: i64, stdout_path: Option<String>,
        stderr_mode: i64, stderr_path: Option<String>, timeout_ms: i64, own_process_group: bool,
    ) -> Result<RunOutput, BuiltinError> {
        let spec = CommandSpec {
            program, argv: args, env_keys, env_vals, env_remove, env_clear, cwd,
            stdin_mode, stdout_mode, stdout_path, stderr_mode, stderr_path,
        };
        super::run_to_end(&spec, stdin_bytes, timeout_ms, own_process_group).map_err(|f| match f {
            RunFail::Start(e) => start_error(&spec.program, &e),
            RunFail::TimedOut => timeout_error(&spec.program, timeout_ms),
            RunFail::Other(e) => e,
        })
    }

    /// `Process.Spawn()` → slot id. The spawn (it may open `ToFile` targets) runs parked;
    /// the slot is registered after unparking.
    #[allow(clippy::too_many_arguments)]
    #[builtin(name = "__process_start")]
    pub fn process_start(
        program: String, args: Vec<String>, env_keys: Vec<String>, env_vals: Vec<String>,
        env_remove: Vec<String>, env_clear: bool, cwd: Option<String>, stdin_mode: i64,
        stdout_mode: i64, stdout_path: Option<String>, stderr_mode: i64, stderr_path: Option<String>,
    ) -> Then<i64> {
        let spec = CommandSpec {
            program, argv: args, env_keys, env_vals, env_remove, env_clear, cwd,
            stdin_mode, stdout_mode, stdout_path, stderr_mode, stderr_path,
        };
        Blocking::new(move || {
            let child = spec.command()?.spawn().map_err(|e| start_error(&spec.program, &e))?;
            Ok(ProcessSlot::spawned(spec.program, child))
        }).then(|ctx: &VmContext, slot| Ok(ctx.alloc_process_slot(slot) as i64))
    }

    /// `ProcessHandle.Wait()` — consumes the slot; the wait + drain run parked.
    #[builtin(name = "__process_wait")]
    pub fn process_wait(ctx: &VmContext, slot: i64) -> Result<Blocking<RunOutput>, BuiltinError> {
        let mut s = ctx.take_process_slot(slot as u64).ok_or_else(|| handle_invalid(None, slot))?;
        let child = s.child.take().ok_or_else(|| handle_invalid(Some(&s.program), slot))?;
        // Close our stdin end first — a child reading its stdin to EOF would deadlock us.
        drop(s.stdin_writer.take());
        let (out, err) = (s.stdout_reader.take(), s.stderr_reader.take());
        Ok(Blocking::new(move || super::wait_and_drain(child, out, err)))
    }

    /// `TryWait()` — null while the child runs; once it exited, consumes the slot.
    #[builtin(name = "__process_try_wait")]
    pub fn process_try_wait(ctx: &VmContext, slot: i64) -> Result<Blocking<MaybeExited>, BuiltinError> {
        let polled = ctx.with_process_slot(slot as u64, |s| match s.child.as_mut() {
            None => Err(handle_invalid(Some(&s.program), slot)),
            Some(c) => c.try_wait().map_err(BuiltinError::from),
        }).ok_or_else(|| handle_invalid(None, slot))??;
        let Some(status) = polled else { return Ok(Blocking::ready(MaybeExited(None))) };
        let mut s = ctx.take_process_slot(slot as u64).ok_or_else(|| handle_invalid(None, slot))?;
        drop(s.stdin_writer.take());
        let (out, err) = (s.stdout_reader.take(), s.stderr_reader.take());
        Ok(Blocking::new(move || {
            let (stdout, stderr) = super::drain_readers(out, err)?;
            Ok(MaybeExited(Some(RunOutput { status, stdout, stderr })))
        }))
    }

    /// `Kill()` / `KillForce()` — both SIGKILL / TerminateProcess today (`force` is reserved).
    #[builtin(name = "__process_kill")]
    pub fn process_kill(ctx: &VmContext, slot: i64, force: bool) -> Result<(), BuiltinError> {
        let _ = force;
        ctx.with_process_slot(slot as u64, |s| match s.child.as_mut() {
            None => Err(handle_invalid(Some(&s.program), slot)),
            Some(c) => match c.kill() {
                // Already exited (`InvalidInput`): kill is idempotent enough.
                Err(e) if e.kind() != std::io::ErrorKind::InvalidInput => Err(e.into()),
                _ => Ok(()),
            },
        }).unwrap_or_else(|| Err(handle_invalid(None, slot)))
    }

    /// Write to the child's stdin (parked: a full pipe blocks). Stdin not piped, or
    /// already closed → `InvalidOperationException` (the handle stays valid).
    #[builtin(name = "__process_stdin_write")]
    pub fn process_stdin_write(ctx: &VmContext, slot: i64, bytes: Vec<u8>) -> Result<Blocking<()>, BuiltinError> {
        let writer = ctx.with_process_slot(slot as u64, |s| s.stdin_writer.clone())
            .ok_or_else(|| handle_invalid(None, slot))?
            .ok_or_else(|| BuiltinError::InvalidOperation(
                "StandardIn has not been redirected (or was already closed).".into()))?;
        Ok(Blocking::new(move || writer.lock().write_all(&bytes).map_err(BuiltinError::from)))
    }

    /// Close our end of the child's stdin (the child sees EOF). Idempotent.
    #[builtin(name = "__process_stdin_close")]
    pub fn process_stdin_close(ctx: &VmContext, slot: i64) -> Result<(), BuiltinError> {
        let writer = ctx.with_process_slot(slot as u64, |s| s.stdin_writer.take())
            .ok_or_else(|| handle_invalid(None, slot))?;
        // The descriptor closes outside the table lock, once no in-flight write holds a clone.
        drop(writer);
        Ok(())
    }

    #[builtin(name = "__process_pid")]
    pub fn process_pid(ctx: &VmContext, slot: i64) -> Result<i64, BuiltinError> {
        ctx.with_process_slot(slot as u64, |s| match &s.child {
            Some(c) => Ok(c.id() as i64),
            None => Err(handle_invalid(Some(&s.program), slot)),
        }).unwrap_or_else(|| Err(handle_invalid(None, slot)))
    }

    /// `Dispose()` — idempotent; a live child is killed and reaped (parked).
    #[builtin(name = "__process_close")]
    pub fn process_close(ctx: &VmContext, slot: i64) -> Blocking<()> {
        match ctx.take_process_slot(slot as u64).and_then(|mut s| s.child.take()) {
            None => Blocking::ready(()),
            Some(mut child) => Blocking::new(move || {
                let _ = child.kill();
                let _ = child.wait();
                Ok(())
            }),
        }
    }

    /// `ReadStdout(buffer, offset, count) -> int` — 0 = EOF.
    #[builtin(name = "__process_stdout_read")]
    pub fn process_stdout_read(ctx: &VmContext, slot: i64, buffer: Value, offset: i64, count: i64) -> Result<Then<i64>, BuiltinError> {
        super::read_pipe(ctx, slot, buffer, offset, count, |s| s.stdout_reader.clone())
    }

    #[builtin(name = "__process_stderr_read")]
    pub fn process_stderr_read(ctx: &VmContext, slot: i64, buffer: Value, offset: i64, count: i64) -> Result<Then<i64>, BuiltinError> {
        super::read_pipe(ctx, slot, buffer, offset, count, |s| s.stderr_reader.clone())
    }

    /// `Process.Which(name)` — the resolved path, or null (rules at `resolve_executable`).
    /// Stat-only (one `stat` per `PATH` entry): unparked, like `File.Exists`.
    #[builtin(name = "__process_which")]
    pub fn process_which(name: &str) -> Option<String> {
        super::resolve_executable(name)
    }
}

// ── executable lookup (`Process.Which`) ──────────────────────────────────
//
// Locate an executable on `$PATH`, matching the semantics of POSIX `command -v` and
// Windows `where`:
//   - Names containing a path separator (Unix `/`, Windows `/` or `\`) are
//     treated as direct paths: stat the file and return if it exists and
//     is executable, otherwise null. PATH lookup is skipped — matches
//     POSIX `command -v ./foo` behaviour.
//   - Otherwise iterate `$PATH` entries (`;`-separated on Windows, `:`
//     elsewhere). Empty entries are treated as the current directory
//     per POSIX. First hit wins.
//   - On Unix, "executable" means `metadata.permissions().mode() & 0o111
//     != 0` (any of u/g/o exec bit set), matching `access(X_OK)`'s
//     practical behaviour for normal files.
//   - On Windows, when the queried name lacks an extension, also try
//     each `PATHEXT` suffix (`.COM;.EXE;.BAT;.CMD;...`); when it already
//     has an extension, match it literally.

pub(crate) fn resolve_executable(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }

    // Direct-path branch: contains separator → no PATH walk.
    if contains_path_separator(name) {
        return check_candidate(std::path::Path::new(name));
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        // POSIX: empty entry is current directory.
        let base = if dir.as_os_str().is_empty() {
            std::path::PathBuf::from(".")
        } else {
            dir
        };
        for cand in candidates_in_dir(&base, name) {
            if let Some(hit) = check_candidate(&cand) {
                return Some(hit);
            }
        }
    }
    None
}

#[cfg(windows)]
fn contains_path_separator(s: &str) -> bool {
    s.contains('/') || s.contains('\\')
}
#[cfg(not(windows))]
fn contains_path_separator(s: &str) -> bool {
    s.contains('/')
}

#[cfg(windows)]
fn candidates_in_dir(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    // If the queried name already has an extension, accept as-is. Otherwise
    // try each PATHEXT entry (default to common set when env unset).
    let has_ext = std::path::Path::new(name).extension().is_some();
    if has_ext {
        return vec![dir.join(name)];
    }
    let pathext = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| dir.join(format!("{}{}", name, ext)))
        .collect()
}
#[cfg(not(windows))]
fn candidates_in_dir(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    vec![dir.join(name)]
}

#[cfg(unix)]
fn check_candidate(path: &std::path::Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() { return None; }
    if meta.permissions().mode() & 0o111 == 0 { return None; }
    Some(path.to_string_lossy().into_owned())
}
#[cfg(not(unix))]
fn check_candidate(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() { return None; }
    Some(path.to_string_lossy().into_owned())
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod process_tests;
