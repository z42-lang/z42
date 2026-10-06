//! Native stack budget for z42 frames — stack overflow is a **fatal** error.
//!
//! Both engines recurse on the native stack (one or more Rust / machine frames
//! per z42 call). Without a check, deep recursion runs into the guard page and
//! the process dies with no message. Every z42 frame entry now compares the
//! stack pointer with a per-thread limit — `exec_function_body` in the
//! interpreter, the prologue of every JIT function — and below it the VM:
//!
//! 1. records the z42 call stack and sets the VM's [`is_fatal`] flag (one per
//!    `VmCore`: every thread of that VM, no other VM in the process);
//! 2. stops: the interpreter returns an internal error, a JIT function returns
//!    the "thrown" code;
//! 3. while the flag is set, no z42 handler runs (`find_handler` and the JIT
//!    catch dispatch decline) and no error is turned into a z42 exception, so
//!    the failure unwinds to the outermost entry;
//! 4. that entry reports it — `z42vm` prints [`take_report`] and exits with
//!    [`EXIT_CODE`], the embedding API returns an error status.
//!
//! Not catchable by design (runtime-audit decision, 2026-10-06): some paths
//! between two checks run unbounded native recursion, re-entry points used to
//! flatten exceptions, and JIT helpers are `extern "C"` — a catchable overflow
//! could not be made reliable, so it is not offered at all.

use std::cell::Cell;
use std::sync::atomic::Ordering;

/// Process exit code of `z42vm` after a fatal VM error (1 = uncaught z42
/// exception / runtime error, 2 = configuration error).
pub const EXIT_CODE: i32 = 3;

/// First words of every fatal report; the embedding API classifies by it.
pub const FATAL_PREFIX: &str = "fatal error: ";

/// Headroom kept below the limit for the native work between two checks
/// (builtins, a lazy JIT compile, formatting the report). An eighth of the
/// stack, clamped to 256 KiB – 1 MiB.
fn margin(size: usize) -> usize {
    (size / 8).clamp(256 * 1024, 1024 * 1024)
}

thread_local! {
    /// Lowest stack address a z42 frame may start at. `0` = not computed yet;
    /// `1` = no answer from the platform (check disabled: `sp < 1` never holds).
    static LIMIT: Cell<usize> = const { Cell::new(0) };
}

/// The report of the latest fatal error, for the outermost entry to print
/// (`z42vm` main / `z42_host_run_app` / `z42_host_invoke`).
static REPORT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The calling thread's limit.
#[inline]
pub fn limit() -> usize {
    LIMIT.with(|l| {
        let v = l.get();
        if v != 0 { return v; }
        let v = match crate::pal::stack::current_thread_stack() {
            Some((low, high)) => low + margin(high - low),
            None => 1,
        };
        l.set(v);
        v
    })
}

/// Approximate stack pointer of the caller.
#[inline(always)]
fn current_sp() -> usize {
    let probe = 0u8;
    std::hint::black_box(&probe) as *const u8 as usize
}

/// Interpreter frame entry: `Err` once the stack is used up.
#[inline(always)]
pub fn check(ctx: &crate::vm_context::VmContext) -> anyhow::Result<()> {
    if current_sp() < limit() {
        return Err(overflow(ctx));
    }
    Ok(())
}

/// Record a stack overflow on this thread: set the fatal flag, keep the z42
/// call stack for the report. Returns the error the interpreter propagates.
#[cold]
#[inline(never)]
pub fn overflow(ctx: &crate::vm_context::VmContext) -> anyhow::Error {
    if !ctx.core.fatal.swap(true, Ordering::SeqCst) {
        let frames = ctx.snapshot_call_stack();
        let report = format!(
            "{FATAL_PREFIX}stack overflow ({} z42 frames)\n{}",
            frames.len(),
            format_trace(&frames),
        );
        *REPORT.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
    }
    anyhow::anyhow!("stack overflow")
}

/// Same order as an exception's stack trace (innermost frame first). A very
/// deep stack keeps its two ends: the innermost frames, then the outermost
/// ones down to the entry point.
fn format_trace(frames: &[crate::exception::FrameSnapshot]) -> String {
    const OUTER: usize = 10;
    const INNER: usize = 30;
    if frames.len() <= OUTER + INNER {
        return crate::exception::format_stack_trace(frames);
    }
    format!(
        "{}\n  ... {} frames omitted ...\n{}",
        crate::exception::format_stack_trace(&frames[frames.len() - INNER..]),
        frames.len() - OUTER - INNER,
        crate::exception::format_stack_trace(&frames[..OUTER]),
    )
}

/// Whether a fatal VM error is unwinding in this VM. Checked by every place
/// that would otherwise run a z42 handler or turn an error into a z42 exception.
#[inline(always)]
pub fn is_fatal(ctx: &crate::vm_context::VmContext) -> bool {
    ctx.core.fatal.load(Ordering::Relaxed)
}

/// The report of the fatal error, if one happened (taken once).
pub fn take_report() -> Option<String> {
    REPORT.lock().unwrap_or_else(|e| e.into_inner()).take()
}


#[cfg(test)]
#[path = "stack_guard_tests.rs"]
mod stack_guard_tests;
