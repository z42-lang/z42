//! The pre-ABI-v2 process builtins — **seed compatibility only**.
//!
//! `__process_run` / `__process_spawn` / `__process_handle_*` return discriminated
//! arrays the z42 facade turns into exceptions. The stdlib in this tree calls the typed
//! builtins in `process.rs` (`__process_run_sync`, `__process_start`, `__process_wait` …),
//! which throw from Rust; these stay registered under their old names and shapes because
//! the published seed's stdlib still calls them (a builtin name keeps its shape). Delete
//! this file once the seed no longer declares them.
//!
//! ## Return shape (`__process_run` / `__process_handle_wait` / `try_wait`)
//!
//! ```text
//! [I64(0), I64(exit_code), Str(stdout), Str(stderr),
//!  Array(stdout_bytes_as_i64), Array(stderr_bytes_as_i64)]   // ok
//! [I64(1), Str(program), Str(os_error_message)]              // start failure
//! [I64(2), Str(program), I64(timeout_ms)]                    // timeout
//! [I64(3), I64(slot_id)]                                     // handle invalid
//! ```
//!
//! Other failures (a malformed bundle, an I/O error) are the typed exceptions of the new
//! builtins, carried through the old `anyhow` channel.

// STAGE2-DEBT(legacy-process-builtins): 种子的 z42.core 不再声明 __process_run / __process_spawn / __process_handle_* 后删掉本文件、其表项与 process_legacy_tests.rs。

use super::convert::{arg_bool, arg_i64, arg_str};
use super::process::{
    drain_readers, exit_code_of, run_to_end, wait_and_drain, CommandSpec, Pipe, ProcessSlot, RunFail, RunOutput,
};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use std::io::{Read, Write};

const KIND_OK:             i64 = 0;
const KIND_START_ERR:      i64 = 1;
const KIND_TIMEOUT:        i64 = 2;
const KIND_HANDLE_INVALID: i64 = 3;

// ── arg-parsing helpers ──────────────────────────────────────────────────

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
            if let Some(b) = borrowed.byte_snapshot() { return Ok(Some(b)); }
            let mut out = Vec::with_capacity(borrowed.len());
            for (i, v) in borrowed.iter_boxed().enumerate() {
                match v {
                    Value::I64(n) if (0..=255).contains(&n) => out.push(n as u8),
                    other => bail!("{}: arg {} byte {} not a u8 in 0..=255: {:?}", ctx, idx, i, other),
                }
            }
            Ok(Some(out))
        }
        Some(other) => bail!("{}: arg {} expected byte array or null, got {:?}", ctx, idx, other),
        None        => bail!("{}: missing arg {}", ctx, idx),
    }
}

/// Args 0..=12 of the run / spawn bundle (arg 8, the one-shot stdin payload, is read by the caller).
fn spec_from(args: &[Value], name: &str) -> Result<CommandSpec> {
    Ok(CommandSpec {
        program:     arg_str(args, 0, name)?.to_string(),
        argv:        require_str_array(args, 1, name)?,
        env_keys:    require_str_array(args, 2, name)?,
        env_vals:    require_str_array(args, 3, name)?,
        env_remove:  require_str_array(args, 4, name)?,
        env_clear:   arg_bool(args, 5, name)?,
        cwd:         optional_str(args, 6, name)?,
        stdin_mode:  arg_i64(args, 7, name)?,
        stdout_mode: arg_i64(args, 9, name)?,
        stdout_path: optional_str(args, 10, name)?,
        stderr_mode: arg_i64(args, 11, name)?,
        stderr_path: optional_str(args, 12, name)?,
    })
}

// ── result encoding ──────────────────────────────────────────────────────

fn bytes_to_value_array(ctx: &VmContext, bytes: Vec<u8>) -> Value {
    let elems: Vec<Value> = bytes.into_iter().map(|b| Value::I64(b as i64)).collect();
    ctx.heap().alloc_array(elems)
}

fn ok_result(ctx: &VmContext, out: RunOutput) -> Value {
    let stdout_str = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr_str = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout_arr = bytes_to_value_array(ctx, out.stdout);
    let stderr_arr = bytes_to_value_array(ctx, out.stderr);
    ctx.heap().alloc_array(vec![
        Value::I64(KIND_OK),
        Value::I64(exit_code_of(out.status)),
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

fn handle_invalid_result(ctx: &VmContext, slot_id: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_HANDLE_INVALID), Value::I64(slot_id)])
}

fn typed(e: crate::corelib::abi::BuiltinError) -> anyhow::Error { anyhow::Error::new(e) }

// ── __process_run / __process_spawn ──────────────────────────────────────

/// Synchronous run (the whole spawn + feed + wait parked). Arg 14 (own process group) is
/// read defensively — absent → true — so 14-argument callers keep working.
pub fn builtin_process_run(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_run";
    let spec        = spec_from(args, NAME)?;
    let stdin_bytes = optional_byte_array(args, 8, NAME)?;
    let timeout_ms  = arg_i64(args, 13, NAME)?;
    let own_process_group = match args.get(14) {
        Some(Value::Bool(b)) => *b,
        _ => true,
    };
    let r = {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        run_to_end(&spec, stdin_bytes, timeout_ms, own_process_group)
    };
    match r {
        Ok(out) => Ok(ok_result(ctx, out)),
        Err(RunFail::Start(e)) => Ok(start_err_result(ctx, &spec.program, &e)),
        Err(RunFail::TimedOut) => Ok(timeout_result(ctx, &spec.program, timeout_ms)),
        Err(RunFail::Other(e)) => Err(typed(e)),
    }
}

/// `[I64(0), I64(slot_id)]` on success or `[I64(1), program, err]` on start failure.
/// Arg 8 (one-shot stdin bytes) is ignored — Spawn streams stdin via the handle.
pub fn builtin_process_spawn(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_spawn";
    let spec = spec_from(args, NAME)?;
    let _stdin_bytes_unused = optional_byte_array(args, 8, NAME)?;
    let spawned = {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        spec.command().map(|mut c| c.spawn())
    };
    let child = match spawned.map_err(typed)? {
        Ok(c)  => c,
        Err(e) => return Ok(start_err_result(ctx, &spec.program, &e)),
    };
    let slot_id = ctx.alloc_process_slot(ProcessSlot::spawned(spec.program, child));
    Ok(ctx.heap().alloc_array(vec![Value::I64(KIND_OK), Value::I64(slot_id as i64)]))
}

// ── handle operations ────────────────────────────────────────────────────

fn require_slot_id(args: &[Value], idx: usize, ctx: &str) -> Result<u64> {
    let n = arg_i64(args, idx, ctx)?;
    if n < 0 {
        bail!("{}: slot id must be non-negative, got {}", ctx, n);
    }
    Ok(n as u64)
}

/// Blocking wait + drain; always consumes the slot.
pub fn builtin_process_handle_wait(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, "__process_handle_wait")?;
    let Some(mut slot) = ctx.take_process_slot(slot_id) else {
        return Ok(handle_invalid_result(ctx, slot_id as i64));
    };
    let Some(child) = slot.child.take() else {
        return Ok(handle_invalid_result(ctx, slot_id as i64));
    };
    drop(slot.stdin_writer.take());
    let (o, e) = (slot.stdout_reader.take(), slot.stderr_reader.take());
    let out = {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        wait_and_drain(child, o, e)
    };
    Ok(ok_result(ctx, out.map_err(typed)?))
}

/// Non-blocking: null while running, the ok tuple once exited (consumes the slot).
pub fn builtin_process_handle_try_wait(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, "__process_handle_try_wait")?;
    let status_opt = ctx.with_process_slot(slot_id, |slot| -> Result<Option<std::process::ExitStatus>> {
        let Some(child) = slot.child.as_mut() else { return Ok(None) };
        Ok(child.try_wait()?)
    });
    let status = match status_opt {
        None              => return Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Err(e))      => return Err(e),
        Some(Ok(None))    => return Ok(Value::Null),
        Some(Ok(Some(s))) => s,
    };
    let Some(mut slot) = ctx.take_process_slot(slot_id) else {
        return Ok(handle_invalid_result(ctx, slot_id as i64));
    };
    drop(slot.stdin_writer.take());
    let (stdout, stderr) = {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        drain_readers(slot.stdout_reader.take(), slot.stderr_reader.take())?
    };
    Ok(ok_result(ctx, RunOutput { status, stdout, stderr }))
}

/// SIGKILL / TerminateProcess (`force` is a no-op). Null on success or the invalid tuple.
pub fn builtin_process_handle_kill(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_kill";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let _force  = arg_bool(args, 1, NAME)?;
    let killed = ctx.with_process_slot(slot_id, |slot| -> Result<bool> {
        let Some(child) = slot.child.as_mut() else { return Ok(false) };
        match child.kill() {
            Ok(())                                                => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Ok(true),
            Err(e)                                                => Err(e.into()),
        }
    });
    match killed {
        None | Some(Ok(false)) => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Err(e))           => Err(e),
        Some(Ok(true))         => Ok(Value::Null),
    }
}

/// Write bytes to the child's piped stdin (parked, outside the table lock).
pub fn builtin_process_handle_write_stdin(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__process_handle_write_stdin";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let bytes   = optional_byte_array(args, 1, NAME)?
        .ok_or_else(|| anyhow!("{}: bytes arg must not be null", NAME))?;
    let writer = match ctx.with_process_slot(slot_id, |slot| slot.stdin_writer.clone()) {
        Some(Some(w)) => w,
        None | Some(None) => return Ok(handle_invalid_result(ctx, slot_id as i64)),
    };
    {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        writer.lock().write_all(&bytes)?;
    }
    Ok(Value::Null)
}

/// Close the writer so the child sees EOF. Idempotent.
pub fn builtin_process_handle_close_stdin(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, "__process_handle_close_stdin")?;
    match ctx.with_process_slot(slot_id, |slot| drop(slot.stdin_writer.take())) {
        None     => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(()) => Ok(Value::Null),
    }
}

/// The OS pid, or the invalid tuple.
pub fn builtin_process_handle_pid(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, "__process_handle_pid")?;
    match ctx.with_process_slot(slot_id, |slot| slot.child.as_ref().map(|c| c.id() as i64)) {
        None | Some(None) => Ok(handle_invalid_result(ctx, slot_id as i64)),
        Some(Some(p))     => Ok(Value::I64(p)),
    }
}

/// Explicit dispose: kill + reap a live child, drop the slot. Idempotent.
pub fn builtin_process_handle_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, "__process_handle_drop")?;
    if let Some(mut slot) = ctx.take_process_slot(slot_id) {
        if let Some(mut child) = slot.child.take() {
            let _park = crate::gc::NativeParkGuard::enter(ctx);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    Ok(Value::Null)
}

// ── streaming reads from child pipes ─────────────────────────────────────
//
// `(slot, buf, offset, count) -> int` (0 = EOF), or null when the slot is missing. A
// single read returns whatever the pipe surfaces (no fill loop); a reader already drained
// by a wait reads as EOF.

fn process_handle_read_impl<R: Read>(
    ctx: &VmContext,
    args: &[Value],
    name: &'static str,
    pick: fn(&ProcessSlot) -> Option<Pipe<R>>,
) -> Result<Value> {
    let slot_id = require_slot_id(args, 0, name)?;
    let buf_arr = match args.get(1) {
        Some(Value::Array(rc)) => *rc,
        Some(other) => bail!("{}: arg 1 expected byte array, got {:?}", name, other),
        None => bail!("{}: missing arg 1 (buf)", name),
    };
    let offset = arg_i64(args, 2, name)? as usize;
    let count  = arg_i64(args, 3, name)? as usize;
    let buf_len = buf_arr.borrow().len();
    if offset + count > buf_len {
        bail!("{}: offset {} + count {} exceeds buf length {}", name, offset, count, buf_len);
    }
    if count == 0 { return Ok(Value::I64(0)); }

    let mut tmp = vec![0u8; count];
    let n = match ctx.with_process_slot(slot_id, |slot| pick(slot)) {
        None          => return Ok(Value::Null),
        Some(None)    => 0,
        Some(Some(r)) => {
            let _park = crate::gc::NativeParkGuard::enter(ctx);
            r.lock().read(&mut tmp)?
        }
    };
    super::fs::write_byte_range(&buf_arr.borrow(), offset, &tmp[..n]);
    Ok(Value::I64(n as i64))
}

/// `__process_handle_read_stdout(slot, buf, off, count) -> int`
pub fn builtin_process_handle_read_stdout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    process_handle_read_impl(ctx, args, "__process_handle_read_stdout", |s| s.stdout_reader.clone())
}

/// `__process_handle_read_stderr(slot, buf, off, count) -> int`
pub fn builtin_process_handle_read_stderr(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    process_handle_read_impl(ctx, args, "__process_handle_read_stderr", |s| s.stderr_reader.clone())
}

#[cfg(test)]
#[path = "process_legacy_tests.rs"]
mod process_legacy_tests;
