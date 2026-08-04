//! `Std.IO.Process` builtins — cross-platform command execution.
//!
//! All Windows / macOS / Linux differences (PATH lookup, PATHEXT,
//! CreateProcess vs fork-exec, signals vs TerminateProcess) are
//! delegated to `std::process::Command`. This module is a thin marshal
//! layer between z42 `Value`s and Rust's process API.
//!
//! ## Architecture
//!
//! z42 `Process` is a pure facade: builder methods accumulate fields,
//! `Run()` / `Spawn()` make a single `[Native("__process_*")]` call
//! that decodes the field bundle into `std::process::Command`. Live
//! children launched by `Spawn` are kept in [`VmContext.processes`]
//! keyed by a monotonic slot id; the z42 `ProcessHandle` holds only
//! that id.
//!
//! ## Return shape (`__process_run`)
//!
//! Heterogeneous `Value::Array` with leading discriminator so the z42
//! facade can route ok / start-failure / timeout uniformly:
//!
//! ```text
//! [I64(0), I64(exit_code), Str(stdout), Str(stderr),
//!  Array(stdout_bytes_as_i64), Array(stderr_bytes_as_i64)]   // ok
//! [I64(1), Str(program), Str(os_error_message)]              // start failure
//! [I64(2), Str(program), I64(timeout_ms)]                    // timeout (Phase 4)
//! ```
//!
//! `__process_spawn` returns `Value::I64(slot_id)`; subsequent
//! `__process_handle_*` builtins take `slot_id` as arg 0.
//!
//! See [`docs/spec/changes/add-std-process/design.md`].

use super::convert::{arg_str, arg_i64, arg_bool};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use std::io::Write;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

const STDIO_NULL:    i64 = 0;
const STDIO_INHERIT: i64 = 1;
const STDIO_PIPE:    i64 = 2;
const STDIO_FILE:    i64 = 3;

const KIND_OK:        i64 = 0;
const KIND_START_ERR: i64 = 1;
const KIND_TIMEOUT:   i64 = 2;

/// One live spawned child plus optionally-captured stdio handles.
///
/// `child` is `Option` so methods that move the child out (`wait`,
/// `kill` followed by reap) can leave the slot in a consumed state
/// without removing it from the map — the next operation observes
/// `None` and reports `ProcessHandleInvalidException`.
pub struct ProcessSlot {
    pub child:         Option<Child>,
    pub stdin_writer:  Option<ChildStdin>,
    pub stdout_reader: Option<ChildStdout>,
    pub stderr_reader: Option<ChildStderr>,
    /// Total wall-clock timeout for `__process_run` paths; `None` for
    /// `__process_spawn` (timeouts apply only to the synchronous Run).
    pub timeout:       Option<std::time::Duration>,
}

// ── arg-parsing helpers ──────────────────────────────────────────────────
//
// `arg_str` / `arg_i64` / `arg_bool` come from convert.rs (refactor-corelib-
// typed-extractors, 2026-05-17). File-local helpers below cover the shapes
// not in the shared layer: Vec<String> (env_keys / argv) and Option<String>
// (cwd null-or-set).

/// Accept `Value::Str` or `Value::Null` — null means "not set, fall through".
fn optional_str(args: &[Value], idx: usize, ctx: &str) -> Result<Option<String>> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(Some(s.to_string())),
        Some(Value::Null)   => Ok(None),
        Some(other) => bail!("{}: arg {} expected string or null, got {:?}", ctx, idx, other),
        None        => bail!("{}: missing arg {}", ctx, idx),
    }
}

fn require_str_array(args: &[Value], idx: usize, ctx: &str) -> Result<Vec<String>> {
    match args.get(idx) {
        Some(Value::Array(rc)) => {
            let borrowed = rc.borrow();
            let mut out = Vec::with_capacity(borrowed.len());
            for (i, v) in borrowed.iter_boxed().enumerate() {
                match v {
                    Value::Str(s) => out.push(s.to_string()),
                    other => bail!("{}: arg {} element {} expected string, got {:?}", ctx, idx, i, other),
                }
            }
            Ok(out)
        }
        Some(other) => bail!("{}: arg {} expected string array, got {:?}", ctx, idx, other),
        None        => bail!("{}: missing arg {}", ctx, idx),
    }
}

/// Accept `Value::Array<u8-as-i64>` or `Value::Null`.
fn optional_byte_array(args: &[Value], idx: usize, ctx: &str) -> Result<Option<Vec<u8>>> {
    match args.get(idx) {
        Some(Value::Null) => Ok(None),
        Some(Value::Array(rc)) => {
            let borrowed = rc.borrow();
            let mut out = Vec::with_capacity(borrowed.len());
            for (i, v) in borrowed.iter_boxed().enumerate() {
                match v {
                    Value::I64(n) if (0..=255).contains(&n) => out.push(n as u8),
                    other => bail!(
                        "{}: arg {} byte {} not a u8 in 0..=255: {:?}",
                        ctx, idx, i, other),
                }
            }
            Ok(Some(out))
        }
        Some(other) => bail!("{}: arg {} expected byte array or null, got {:?}", ctx, idx, other),
        None        => bail!("{}: missing arg {}", ctx, idx),
    }
}

// ── stdio configuration ─────────────────────────────────────────────────

fn stdio_for_input(mode: i64, path: Option<&str>) -> Result<Stdio> {
    match mode {
        STDIO_NULL    => Ok(Stdio::null()),
        STDIO_INHERIT => Ok(Stdio::inherit()),
        STDIO_PIPE    => Ok(Stdio::piped()),
        STDIO_FILE    => {
            let p = path.ok_or_else(|| anyhow!("stdin Stdio.ToFile missing path"))?;
            Ok(Stdio::from(std::fs::File::open(p)?))
        }
        _ => bail!("invalid stdin mode {}", mode),
    }
}

fn stdio_for_output(mode: i64, path: Option<&str>) -> Result<Stdio> {
    match mode {
        STDIO_NULL    => Ok(Stdio::null()),
        STDIO_INHERIT => Ok(Stdio::inherit()),
        STDIO_PIPE    => Ok(Stdio::piped()),
        STDIO_FILE    => {
            let p = path.ok_or_else(|| anyhow!("stdout/stderr Stdio.ToFile missing path"))?;
            Ok(Stdio::from(std::fs::OpenOptions::new()
                .write(true).create(true).truncate(true).open(p)?))
        }
        _ => bail!("invalid stdout/stderr mode {}", mode),
    }
}

// ── result encoding ──────────────────────────────────────────────────────

fn bytes_to_value_array(ctx: &VmContext, bytes: Vec<u8>) -> Value {
    let elems: Vec<Value> = bytes.into_iter().map(|b| Value::I64(b as i64)).collect();
    ctx.heap().alloc_array(elems)
}

fn ok_result(ctx: &VmContext, status: std::process::ExitStatus,
             stdout_bytes: Vec<u8>, stderr_bytes: Vec<u8>) -> Value {
    // Unix: killed by signal → code() is None; encode as 128 + signal
    // (matches sh convention). Windows: code() is always Some.
    let exit_code: i64 = match status.code() {
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
    };
    let stdout_str = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes).into_owned();
    let stdout_arr = bytes_to_value_array(ctx, stdout_bytes);
    let stderr_arr = bytes_to_value_array(ctx, stderr_bytes);
    ctx.heap().alloc_array(vec![
        Value::I64(KIND_OK),
        Value::I64(exit_code),
        Value::Str(stdout_str.into()),
        Value::Str(stderr_str.into()),
        stdout_arr,
        stderr_arr,
    ])
}

fn start_err_result(ctx: &VmContext, program: &str, err: &std::io::Error) -> Value {
    ctx.heap().alloc_array(vec![
        Value::I64(KIND_START_ERR),
        Value::Str(program.to_string().into()),
        Value::Str(format!("{} (kind: {:?})", err, err.kind()).into()),
    ])
}

fn timeout_result(ctx: &VmContext, program: &str, timeout_ms: i64) -> Value {
    ctx.heap().alloc_array(vec![
        Value::I64(KIND_TIMEOUT),
        Value::Str(program.to_string().into()),
        Value::I64(timeout_ms),
    ])
}

// ── __process_run ────────────────────────────────────────────────────────

/// Synchronous run: spawn, optionally feed stdin bytes, wait_with_output,
/// return result tuple. Timeout (`timeout_ms >= 0`) lands in Phase 4 —
/// for now arg 13 is parsed but only -1 (no timeout) is honoured; any
/// other value yields a Phase 4 placeholder error.
pub fn builtin_process_run(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_run";

    let program     = arg_str(args, 0, NAME)?;
    let argv        = require_str_array(args, 1, NAME)?;
    let env_keys    = require_str_array(args, 2, NAME)?;
    let env_vals    = require_str_array(args, 3, NAME)?;
    let env_remove  = require_str_array(args, 4, NAME)?;
    let env_clear   = arg_bool(args, 5, NAME)?;
    let cwd         = optional_str(args, 6, NAME)?;
    let stdin_mode  = arg_i64(args, 7, NAME)?;
    let stdin_bytes = optional_byte_array(args, 8, NAME)?;
    let stdout_mode = arg_i64(args, 9, NAME)?;
    let stdout_path = optional_str(args, 10, NAME)?;
    let stderr_mode = arg_i64(args, 11, NAME)?;
    let stderr_path = optional_str(args, 12, NAME)?;
    let timeout_ms  = arg_i64(args, 13, NAME)?;

    if env_keys.len() != env_vals.len() {
        bail!("{}: env_keys / env_vals length mismatch ({} vs {})",
              NAME, env_keys.len(), env_vals.len());
    }

    let mut cmd = Command::new(&program);
    cmd.args(&argv);

    if env_clear { cmd.env_clear(); }
    for (k, v) in env_keys.iter().zip(env_vals.iter()) {
        cmd.env(k, v);
    }
    for k in &env_remove {
        cmd.env_remove(k);
    }
    if let Some(d) = &cwd {
        cmd.current_dir(d);
    }

    cmd.stdin (stdio_for_input (stdin_mode,  None)?);
    cmd.stdout(stdio_for_output(stdout_mode, stdout_path.as_deref())?);
    cmd.stderr(stdio_for_output(stderr_mode, stderr_path.as_deref())?);

    // Put the child in its own process group (pgid == child pid) so a run-timeout
    // can kill the WHOLE tree, not just the direct child. Without this, `sh -c
    // "sleep 5"` that forks a `sleep` grandchild leaves the orphaned grandchild
    // holding the inherited stdout/stderr pipe write-ends open — the reader-thread
    // join below then blocks until the grandchild exits naturally (the timeout is
    // detected correctly but the call returns ~5s late). See wait_with_optional_timeout.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = match cmd.spawn() {
        Ok(c)  => c,
        Err(e) => return Ok(start_err_result(ctx, &program, &e)),
    };

    // Optional one-shot stdin payload (Pipe mode + .StdinBytes / .StdinString).
    if let Some(bytes) = stdin_bytes {
        if let Some(mut sin) = child.stdin.take() {
            sin.write_all(&bytes)?;
            drop(sin); // EOF for the child
        }
    }

    // Concurrent reader threads drain piped stdout / stderr so a chatty
    // child can't deadlock on a full pipe while we poll wait().
    let stdout_h = child.stdout.take().map(|r| std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = std::io::BufReader::new(r).read_to_end(&mut buf);
        buf
    }));
    let stderr_h = child.stderr.take().map(|r| std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = std::io::BufReader::new(r).read_to_end(&mut buf);
        buf
    }));

    let (status, timed_out) = wait_with_optional_timeout(&mut child, timeout_ms)?;

    let out = stdout_h.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    let err = stderr_h.map(|h| h.join().unwrap_or_default()).unwrap_or_default();

    if timed_out {
        return Ok(timeout_result(ctx, &program, timeout_ms));
    }
    Ok(ok_result(ctx, status, out, err))
}

/// Poll-loop wait — `timeout_ms < 0` means "no timeout, block forever".
/// On timeout the child is `kill`'d and the post-kill `wait` is reaped
/// to release zombie state; the returned `bool` reports whether we hit
/// the timeout (caller maps to `ProcessTimeoutException`).
///
/// 20ms poll interval is a compromise between latency and CPU: a
/// 5-second timeout polls 250 times; a 50ms timeout still sees the
/// child within one tick. No condvar / signal: stdlib-only path keeps
/// the implementation portable.
fn wait_with_optional_timeout(
    child: &mut Child, timeout_ms: i64,
) -> Result<(std::process::ExitStatus, bool)> {
    if timeout_ms < 0 {
        return Ok((child.wait()?, false));
    }
    let timeout = std::time::Duration::from_millis(timeout_ms as u64);
    let start   = std::time::Instant::now();
    loop {
        if let Some(s) = child.try_wait()? { return Ok((s, false)); }
        if start.elapsed() >= timeout {
            kill_process_tree(child);
            let status = child.wait()?;
            return Ok((status, true));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// SIGKILL the child AND its whole process group (grandchildren included). The
/// child was spawned with `process_group(0)` (pgid == child pid), so `kill(-pid)`
/// reaps a forked-grandchild tree (e.g. `sh -c "sleep 5"`). Without the group kill,
/// killing only the direct child orphans grandchildren that keep the inherited
/// stdout/stderr pipe write-ends open, blocking the caller's reader-thread joins
/// until they exit naturally. `child.kill()` is kept as the portable fallback.
fn kill_process_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // Safe: kill(2) on a negative pid targets the process group; a stale/no-such
        // group just returns ESRCH, which we ignore (the child.kill() below covers it).
        let pid = child.id() as i32;
        unsafe { libc::kill(-pid, libc::SIGKILL); }
    }
    let _ = child.kill();
}

// ── __process_spawn ──────────────────────────────────────────────────────

/// Spawn the configured Command and stash the live child + captured
/// stdio handles in `VmContext.processes`. Returns
/// `[I64(0), I64(slot_id)]` on success or `[I64(1), program, err]` on
/// start failure. Stdin one-shot bytes are *not* honoured here — the
/// z42 facade enforces "Pipe stdin in Spawn means stream via handle";
/// any leftover `stdin_bytes` value passed is silently ignored.
pub fn builtin_process_spawn(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_spawn";

    let program     = arg_str(args, 0, NAME)?;
    let argv        = require_str_array(args, 1, NAME)?;
    let env_keys    = require_str_array(args, 2, NAME)?;
    let env_vals    = require_str_array(args, 3, NAME)?;
    let env_remove  = require_str_array(args, 4, NAME)?;
    let env_clear   = arg_bool(args, 5, NAME)?;
    let cwd         = optional_str(args, 6, NAME)?;
    let stdin_mode  = arg_i64(args, 7, NAME)?;
    let _stdin_bytes_unused = optional_byte_array(args, 8, NAME)?; // see doc above
    let stdout_mode = arg_i64(args, 9, NAME)?;
    let stdout_path = optional_str(args, 10, NAME)?;
    let stderr_mode = arg_i64(args, 11, NAME)?;
    let stderr_path = optional_str(args, 12, NAME)?;

    if env_keys.len() != env_vals.len() {
        bail!("{}: env_keys / env_vals length mismatch", NAME);
    }

    let mut cmd = Command::new(&program);
    cmd.args(&argv);
    if env_clear { cmd.env_clear(); }
    for (k, v) in env_keys.iter().zip(env_vals.iter()) {
        cmd.env(k, v);
    }
    for k in &env_remove {
        cmd.env_remove(k);
    }
    if let Some(d) = &cwd {
        cmd.current_dir(d);
    }
    cmd.stdin (stdio_for_input (stdin_mode,  None)?);
    cmd.stdout(stdio_for_output(stdout_mode, stdout_path.as_deref())?);
    cmd.stderr(stdio_for_output(stderr_mode, stderr_path.as_deref())?);

    let mut child = match cmd.spawn() {
        Ok(c)  => c,
        Err(e) => return Ok(start_err_result(ctx, &program, &e)),
    };

    let stdin_writer  = child.stdin.take();
    let stdout_reader = child.stdout.take();
    let stderr_reader = child.stderr.take();

    let slot = ProcessSlot {
        child:         Some(child),
        stdin_writer,
        stdout_reader,
        stderr_reader,
        timeout:       None,
    };
    let slot_id = ctx.alloc_process_slot(slot);

    Ok(ctx.heap().alloc_array(vec![
        Value::I64(KIND_OK),
        Value::I64(slot_id as i64),
    ]))
}

// ── handle operations ────────────────────────────────────────────────────

/// `[I64(3), I64(slot_id)]` — slot doesn't exist or child already
/// consumed. z42 facade translates to `ProcessHandleInvalidException`.
const KIND_HANDLE_INVALID: i64 = 3;

fn handle_invalid_result(ctx: &VmContext, slot_id: i64) -> Value {
    ctx.heap().alloc_array(vec![
        Value::I64(KIND_HANDLE_INVALID),
        Value::I64(slot_id),
    ])
}

fn require_slot_id(args: &[Value], idx: usize, ctx: &str) -> Result<u64> {
    let n = arg_i64(args, idx, ctx)?;
    if n < 0 {
        bail!("{}: slot id must be non-negative, got {}", ctx, n);
    }
    Ok(n as u64)
}

/// Drain the (currently held) stdout / stderr readers into byte vecs.
/// Helper for `wait` / `try_wait` post-exit drain.
fn drain_readers(
    mut stdout: Option<ChildStdout>,
    mut stderr: Option<ChildStderr>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut err = Vec::new();
    if let Some(r) = stdout.as_mut() { r.read_to_end(&mut out)?; }
    if let Some(r) = stderr.as_mut() { r.read_to_end(&mut err)?; }
    Ok((out, err))
}

/// `__process_handle_wait(slot_id)` — blocking wait + drain stdio.
/// Returns the same ok-result shape as `__process_run`, or
/// handle-invalid discriminator. Always consumes the slot.
pub fn builtin_process_handle_wait(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_wait";
    let slot_id = require_slot_id(args, 0, NAME)?;

    let Some(mut slot) = ctx.take_process_slot(slot_id) else {
        return Ok(handle_invalid_result(ctx, slot_id as i64));
    };
    let Some(mut child) = slot.child.take() else {
        return Ok(handle_invalid_result(ctx, slot_id as i64));
    };

    // Drop the writer before reading — otherwise a child waiting on its
    // own stdin will deadlock us.
    drop(slot.stdin_writer.take());

    let stdout = slot.stdout_reader.take();
    let stderr = slot.stderr_reader.take();

    // Read concurrently on background threads to avoid pipe-full
    // deadlocks while we block in wait(). One-or-zero pipes can stay
    // synchronous (read after wait).
    let (status, out, err) = match (stdout, stderr) {
        (Some(o), Some(e)) => {
            let h_o = std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = Vec::new();
                let _ = std::io::BufReader::new(o).read_to_end(&mut buf);
                buf
            });
            let h_e = std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = Vec::new();
                let _ = std::io::BufReader::new(e).read_to_end(&mut buf);
                buf
            });
            let status = child.wait()?;
            let out = h_o.join().unwrap_or_default();
            let err = h_e.join().unwrap_or_default();
            (status, out, err)
        }
        (o, e) => {
            let status = child.wait()?;
            let (out, err) = drain_readers(o, e)?;
            (status, out, err)
        }
    };
    Ok(ok_result(ctx, status, out, err))
}

/// `__process_handle_try_wait(slot_id)` — non-blocking. Returns:
/// - `Value::Null` if still running
/// - ok-result tuple if exited (and consumes the slot)
/// - handle-invalid tuple if slot missing
pub fn builtin_process_handle_try_wait(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_try_wait";
    let slot_id = require_slot_id(args, 0, NAME)?;

    // First peek: see if child has exited. Hold the slot if not.
    let status_opt = ctx.with_process_slot(slot_id, |slot| -> Result<Option<std::process::ExitStatus>> {
        let Some(child) = slot.child.as_mut() else {
            return Ok(None); // already consumed; caller will see invalid below
        };
        Ok(child.try_wait()?)
    });

    let status = match status_opt {
        None              => return Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Err(e))      => return Err(e),
        Some(Ok(None))    => return Ok(Value::Null),    // still running
        Some(Ok(Some(s))) => s,
    };

    // Exited — take the slot to drain stdio.
    let Some(mut slot) = ctx.take_process_slot(slot_id) else {
        return Ok(handle_invalid_result(ctx, slot_id as i64));
    };
    drop(slot.stdin_writer.take());
    let (out, err) = drain_readers(slot.stdout_reader.take(), slot.stderr_reader.take())?;
    Ok(ok_result(ctx, status, out, err))
}

/// `__process_handle_kill(slot_id, force)` — send SIGKILL / TerminateProcess.
/// `force` is currently a no-op in both branches: `std::process::Child::kill`
/// is unconditionally SIGKILL on Unix and TerminateProcess on Windows; a
/// polite-SIGTERM variant needs `libc` and is a follow-up. Returns
/// `Value::Null` on success or handle-invalid tuple.
pub fn builtin_process_handle_kill(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_kill";
    let slot_id  = require_slot_id(args, 0, NAME)?;
    let _force   = arg_bool(args, 1, NAME)?;

    let killed = ctx.with_process_slot(slot_id, |slot| -> Result<bool> {
        let Some(child) = slot.child.as_mut() else { return Ok(false) };
        // Ignore InvalidInput (already-exited): kill is idempotent enough.
        match child.kill() {
            Ok(())                                                       => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput        => Ok(true),
            Err(e)                                                       => Err(e.into()),
        }
    });
    match killed {
        None             => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Err(e))     => Err(e),
        Some(Ok(false))  => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Ok(true))   => Ok(Value::Null),
    }
}

/// `__process_handle_write_stdin(slot_id, bytes)` — write bytes to the
/// child's piped stdin. No-op if the slot was spawned without
/// `Stdio.Pipe` stdin (writer is `None`).
pub fn builtin_process_handle_write_stdin(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_write_stdin";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let bytes   = optional_byte_array(args, 1, NAME)?
        .ok_or_else(|| anyhow!("{}: bytes arg must not be null", NAME))?;

    let r = ctx.with_process_slot(slot_id, |slot| -> Result<bool> {
        let Some(w) = slot.stdin_writer.as_mut() else { return Ok(false) };
        w.write_all(&bytes)?;
        Ok(true)
    });
    match r {
        None              => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Err(e))      => Err(e),
        Some(Ok(false))   => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Ok(true))    => Ok(Value::Null),
    }
}

/// `__process_handle_close_stdin(slot_id)` — close the writer so the
/// child sees EOF. Idempotent (no-op if writer already closed).
pub fn builtin_process_handle_close_stdin(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_close_stdin";
    let slot_id = require_slot_id(args, 0, NAME)?;

    let r = ctx.with_process_slot(slot_id, |slot| {
        drop(slot.stdin_writer.take()); // dropping the writer closes the fd
    });
    match r {
        None    => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(()) => Ok(Value::Null),
    }
}

/// `__process_handle_pid(slot_id)` — return the OS pid (u32 widened to
/// i64) or handle-invalid tuple. After consumption the slot has no
/// child, so this returns invalid.
pub fn builtin_process_handle_pid(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_pid";
    let slot_id = require_slot_id(args, 0, NAME)?;

    let pid = ctx.with_process_slot(slot_id, |slot| {
        slot.child.as_ref().map(|c| c.id() as i64)
    });
    match pid {
        None | Some(None) => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Some(p))     => Ok(Value::I64(p)),
    }
}

/// `__process_handle_drop(slot_id)` — explicit dispose. If child is
/// still alive, kill + wait to reap; then drop slot. Idempotent: a
/// missing slot is silent success (since GC may have already triggered).
pub fn builtin_process_handle_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_drop";
    let slot_id = require_slot_id(args, 0, NAME)?;

    if let Some(mut slot) = ctx.take_process_slot(slot_id) {
        if let Some(mut child) = slot.child.take() {
            // Best-effort reap.
            let _ = child.kill();
            let _ = child.wait();
        }
        drop(slot);
    }
    Ok(Value::Null)
}

// ── add-process-stream-stdio (2026-05-24) — streaming reads from child pipes ──
//
// Mirrors `__file_read`'s buffer-fill shape (slot, buf, offset, count -> int;
// 0 = EOF) so callers can plug a z42-side `ProcessOutputStream` straight into
// the `Std.IO.Stream` ecosystem alongside `FileStream` / `MemoryStream`.
//
// A single `Read` call reads up-to-`count` bytes (whatever the OS pipe
// surfaces in one go) — we deliberately do NOT loop to fill the buffer;
// that's the `Read` contract everywhere in the stream API. Blocking
// behaviour: the underlying `ChildStdout::read` blocks until at least
// one byte is available OR the child closes the pipe (EOF).
//
// Reader-already-consumed-by-Wait case: `slot.stdout_reader` becomes
// `None` after Wait/TryWait drained it. We treat None as EOF (`Ok(0)`)
// rather than handle-invalid — the z42 facade's `CanRead()` defends
// against the typical misuse; if a caller still drives Read post-Wait
// they'll just see EOF, matching .NET behaviour on a closed stream.

use std::io::Read;

/// Common impl for stdout / stderr — picks the reader off the slot,
/// reads up to `count` bytes into a scratch Vec<u8>, then copies into
/// the user's z42 `byte[]`.
fn process_handle_read_impl(
    ctx: &VmContext,
    args: &[Value],
    name: &'static str,
    is_stderr: bool,
) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, name)?;
    let buf_value = args.get(1).cloned()
        .ok_or_else(|| anyhow!("{}: missing arg 1 (buf)", name))?;
    let buf_arr = match &buf_value {
        Value::Array(rc) => rc.clone(),
        other => bail!("{}: arg 1 expected byte array, got {:?}", name, other),
    };
    let offset  = arg_i64(args, 2, name)? as usize;
    let count   = arg_i64(args, 3, name)? as usize;

    let buf_len = buf_arr.borrow().len();
    if offset + count > buf_len {
        bail!("{}: offset {} + count {} exceeds buf length {}", name, offset, count, buf_len);
    }
    if count == 0 { return Ok(Value::I64(0)); }

    // Borrow the reader from the slot, do the blocking read, then drop
    // the slot borrow before touching the heap (Read blocks; we don't
    // want to hold the slot lock across an unbounded wait).
    let mut tmp = vec![0u8; count];
    let read_result = ctx.with_process_slot(slot_id, |slot| -> Result<Option<usize>> {
        // Returning None signals "reader is None" → EOF.
        if is_stderr {
            let Some(r) = slot.stderr_reader.as_mut() else { return Ok(None) };
            Ok(Some(r.read(&mut tmp)?))
        } else {
            let Some(r) = slot.stdout_reader.as_mut() else { return Ok(None) };
            Ok(Some(r.read(&mut tmp)?))
        }
    });
    let n = match read_result {
        // Slot missing: facade translates Value::Null →
        // ProcessHandleInvalidException. Distinct from EOF (Value::I64(0)).
        None              => return Ok(Value::Null),
        Some(Err(e))      => return Err(e),
        Some(Ok(None))    => 0,       // pipe never piped → EOF
        Some(Ok(Some(n))) => n,
    };

    let mut borrowed = buf_arr.borrow_mut();
    for i in 0..n {
        borrowed.set_boxed(offset + i, Value::I64(tmp[i] as i64));
    }
    Ok(Value::I64(n as i64))
}

/// `__process_handle_read_stdout(slot, buf, off, count) -> int`
pub fn builtin_process_handle_read_stdout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    process_handle_read_impl(ctx, args, "__process_handle_read_stdout", false)
}

/// `__process_handle_read_stderr(slot, buf, off, count) -> int`
pub fn builtin_process_handle_read_stderr(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    process_handle_read_impl(ctx, args, "__process_handle_read_stderr", true)
}

// ── __process_which ──────────────────────────────────────────────────────
//
// `Process.Which(name)` — locate an executable on `$PATH`, matching the
// semantics of POSIX `command -v` and Windows `where`. Returns the full
// resolved path or `Value::Null` when not found.
//
// Resolution rules:
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

pub fn builtin_process_which(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_which";
    let name = arg_str(args, 0, NAME)?;
    Ok(match resolve_executable(&name) {
        Some(path) => Value::Str(path.into()),
        None       => Value::Null,
    })
}

fn resolve_executable(name: &str) -> Option<String> {
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
