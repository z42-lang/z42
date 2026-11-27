//! OS signal handler — z42 call stack capture on hard crash.
//!
//! Phase 2 of the panic-hook story (Phase 1 = Rust panic hook in main.rs).
//! Spec: `变更 add-os-signal-handler/` .
//!
//! Covers POSIX fatal signals only:
//! `SIGSEGV` / `SIGABRT` / `SIGFPE` / `SIGILL` / `SIGBUS`. Windows VEH is
//! Phase 2.1 future work.
//!
//! # Async-signal safety
//!
//! The signal handler is invoked in async-signal context. The Rust standard
//! library + most crates are NOT async-signal-safe. This module is structured
//! around a [`sigsafe`] submodule whose only operations are direct
//! `libc::write(2)` syscalls + stack-buffer integer formatting. The handler
//! body must touch ONLY `sigsafe` and atomic loads — no `format!`, no
//! `eprintln!`, no `Mutex::lock` (only `try_lock`), no allocation.
//!
//! # Stack walk strategy
//!
//! The handler walks the process-wide Core registry, then each Core's registered
//! contexts. Both walks use `try_lock` and write a placeholder on contention.
//!
//! A context's frame stack is owner-thread only (`exec/call/frame_stack.rs`),
//! so the handler prints full frames only for the contexts **the crashing
//! thread owns** — it runs on that thread, so reading them is an owner read.
//! For every other context it prints just the frame count, read from the
//! stack's published-depth atomic: another thread may be pushing or popping
//! right now, and dereferencing its frames could fault again and lose the
//! whole report.
//!
//! # Termination
//!
//! Handler ends by resetting the signal disposition to `SIG_DFL` and
//! re-raising — this preserves kernel coredump behaviour (`ulimit -c`).
//!
//! Where the target has no POSIX signals `pal::signal` registers nothing and
//! [`install`] only records that it ran.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, Weak};

// PAL Phase 3 (add-pal-signal): OS signal primitives live in `pal::signal`;
// the call sites below (`sigsafe::*`, `signal_name(sig)`) resolve to them via
// these imports, so the z42 crash-report logic in this file stays unchanged.
use crate::exec::stack::CrashFrames;
use crate::pal::signal::{self, sigsafe, signal_name, STDERR_FD};
use crate::vm::VmCore;

/// Weak entries let Core teardown proceed independently of diagnostics.
static VM_CORES: Mutex<Vec<Weak<VmCore>>> = Mutex::new(Vec::new());

/// Register a Core and discard expired entries.
pub(crate) fn register_crash_core(core: &Arc<VmCore>) {
    if let Ok(mut cores) = VM_CORES.lock() {
        cores.retain(|entry| entry.strong_count() > 0);
        cores.push(Arc::downgrade(core));
    }
}

/// Build a snapshot for ordinary diagnostics outside the signal handler.
pub(crate) fn live_cores_snapshot() -> Vec<Arc<VmCore>> {
    VM_CORES
        .lock()
        .ok()
        .map(|cores| cores.iter().filter_map(Weak::upgrade).collect())
        .unwrap_or_default()
}

/// File descriptor for the optional `Z42_CRASH_DIR` persistence file.
/// `-1` = no file (env var not set or open failed). Opened once at
/// [`install`] time; the OS reclaims it at process exit.
static SIGNAL_CRASH_FD: AtomicI32 = AtomicI32::new(-1);

/// Idempotency guard — second install call is a no-op.
static INSTALLED: AtomicI32 = AtomicI32::new(0);

/// Install signal handlers for the 5 covered POSIX signals.
/// Safe to call multiple times — only first call has effect.
pub(crate) fn install(crash_dir: impl FnOnce() -> Option<&'static std::path::Path>) {
    if INSTALLED.swap(1, Ordering::SeqCst) != 0 {
        return;
    }

    // Open crash_report fd from Z42_CRASH_DIR (if set + writable)
    if let Some(dir) = crash_dir() {
        open_crash_report_fd(dir);
    }

    // Register the z42 crash `handler` for the 5 fatal signals (OS primitive).
    if signal::register_fatal_handlers(handler) {
        tracing::debug!("OS signal handlers installed for SIGSEGV/SIGABRT/SIGFPE/SIGILL/SIGBUS");
    }
}

fn open_crash_report_fd(dir_path: &std::path::Path) {
    // Open before registering handlers so the signal path only writes to the fd.
    let ts_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir_path.join(format!("z42vm-crash-{ts_ns}.txt"));

    if let Err(e) = std::fs::create_dir_all(dir_path) {
        tracing::warn!("Z42_CRASH_DIR {} create failed: {e}; OS signal reports go to stderr only", dir_path.display());
        return;
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => {
            if let Some(fd) = signal::leak_fd(file) {
                SIGNAL_CRASH_FD.store(fd, Ordering::SeqCst);
                tracing::debug!("signal crash report fd opened: {}", path.display());
            }
        }
        Err(e) => {
            tracing::warn!("Z42_CRASH_DIR {} not writable: {e}; OS signal reports go to stderr only", path.display());
        }
    }
}

/// Async-signal context handler. STRICT RULES:
///  - no allocation
///  - no Mutex::lock (try_lock only)
///  - no eprintln!/format! (use sigsafe::*)
///  - no thread_local! reads (may allocate)
extern "C" fn handler(sig: i32) {
    // 1. Write header to stderr
    sigsafe::write_str(STDERR_FD, b"\n[z42vm signal ");
    sigsafe::write_str(STDERR_FD, signal_name(sig));
    sigsafe::write_str(STDERR_FD, b"]\n");

    // 2. Build banner
    write_banner(STDERR_FD);

    // 3. Z42 call stacks (best-effort, try_lock; other threads: frame count only)
    write_call_stacks(STDERR_FD);

    // 4. Mirror to crash_report fd if configured
    let fd = SIGNAL_CRASH_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        sigsafe::write_str(fd, b"\n[z42vm signal ");
        sigsafe::write_str(fd, signal_name(sig));
        sigsafe::write_str(fd, b"]\n");
        write_banner(fd);
        write_call_stacks(fd);
        sigsafe::write_str(STDERR_FD, b"[signal hook] crash report appended to Z42_CRASH_DIR fd\n");
    }

    // 5. Reset disposition + reraise — kernel default takes over (coredump if ulimit -c permits)
    signal::reset_default_and_reraise(sig);
}

fn write_banner(fd: i32) {
    sigsafe::write_str(fd, b"z42vm ");
    sigsafe::write_str(fd, env!("CARGO_PKG_VERSION").as_bytes());
    sigsafe::write_str(fd, b" (");
    sigsafe::write_str(fd, if cfg!(debug_assertions) { b"debug" } else { b"release" });
    sigsafe::write_str(fd, b", ");
    sigsafe::write_str(fd, std::env::consts::OS.as_bytes());
    sigsafe::write_str(fd, b"/");
    sigsafe::write_str(fd, std::env::consts::ARCH.as_bytes());
    sigsafe::write_str(fd, b")\n");
}

/// Walk live Cores and contexts without blocking. A contended registry gets
/// a placeholder. Only the crashing thread's frames are printed.
fn write_call_stacks(fd: i32) {
    let cores_guard = match VM_CORES.try_lock() {
        Ok(g) => g,
        Err(_) => {
            sigsafe::write_str(fd, b"=== z42 call stack: unavailable (VM_CORES lock contended) ===\n");
            return;
        }
    };

    if cores_guard.is_empty() {
        sigsafe::write_str(fd, b"=== z42 call stack: no VmCore registered ===\n");
        return;
    }

    let mut live_count = 0u32;
    for weak in cores_guard.iter() {
        if weak.strong_count() > 0 { live_count += 1; }
    }

    sigsafe::write_str(fd, b"=== z42 call stack (");
    sigsafe::write_dec_u32(fd, live_count);
    sigsafe::write_str(fd, b" VmCore(s) live) ===\n");

    for (core_idx, weak) in cores_guard.iter().enumerate() {
        let Some(core) = weak.upgrade() else { continue };
        sigsafe::write_str(fd, b"-- VmCore #");
        sigsafe::write_dec_u32(fd, core_idx as u32);
        sigsafe::write_str(fd, b" --\n");

        let count = core.threads().try_visit_contexts(|ctx_idx, ctx| {
            // SAFETY: the visitor only formats the view and writes it to the fd.
            unsafe {
                ctx.exec().inspect_crash_frames(|view| match view {
                    CrashFrames::CurrentThread(frames) => {
                        write_thread_header(fd, ctx_idx, frames.len(), true);
                        for (i, frame) in frames.iter().enumerate() {
                            sigsafe::write_str(fd, b"    #");
                            sigsafe::write_dec_u32(fd, i as u32);
                            sigsafe::write_str(fd, b"  ");
                            write_frame(fd, frame);
                        }
                    }
                    CrashFrames::PublishedDepth(depth) => {
                        write_thread_header(fd, ctx_idx, depth, false);
                    }
                });
            }
        });
        match count {
            None => sigsafe::write_str(fd, b"  <vm_contexts lock contended>\n"),
            Some(0) => sigsafe::write_str(fd, b"  <no VmContext>\n"),
            Some(_) => {}
        }
    }
    sigsafe::write_str(fd, b"===\n");
}

/// One context's header line:
///
/// - `  thread #<i> (<n> frame(s))` — empty stack, or the crashing thread's
///   own stack (its frames follow);
/// - `  thread #<i> (<n> frame(s), other thread: frames not read)` — any other
///   thread's stack, whose frames the handler must not dereference.
fn write_thread_header(fd: i32, ctx_idx: usize, depth: usize, crashing: bool) {
    sigsafe::write_str(fd, b"  thread #");
    sigsafe::write_dec_u32(fd, ctx_idx as u32);
    sigsafe::write_str(fd, b" (");
    sigsafe::write_dec_u32(fd, depth.min(u32::MAX as usize) as u32);
    sigsafe::write_str(fd, b" frame(s)");
    if depth > 0 && !crashing {
        sigsafe::write_str(fd, b", other thread: frames not read");
    }
    sigsafe::write_str(fd, b")\n");
}

/// `<name> at <file>:<line>:<col>\n` for one frame — the same text
/// `VmFrame::snapshot` would produce, computed from `func` + `pc` without
/// allocating: the precomputed `frame_meta` strings are written in place, and
/// a function without them has its name written piece by piece; `line_col`
/// is a binary search over the line table.
fn write_frame(fd: i32, frame: &crate::exec::frames::VmFrame) {
    let func = frame.func();
    match &func.frame_meta {
        Some((name, file)) => {
            sigsafe::write_str(fd, name.as_bytes());
            sigsafe::write_str(fd, b" at ");
            sigsafe::write_str(fd, file.as_bytes());
        }
        None => {
            crate::zpkg::debug::for_each_frame_name_piece(func, |piece| {
                sigsafe::write_str(fd, piece.as_bytes());
            });
            sigsafe::write_str(fd, b" at ");
            sigsafe::write_str(fd, crate::zpkg::debug::frame_file(func).as_bytes());
        }
    }
    let (line, column) = frame.line_col();
    sigsafe::write_str(fd, b":");
    sigsafe::write_dec_u32(fd, line);
    sigsafe::write_str(fd, b":");
    sigsafe::write_dec_u32(fd, column);
    sigsafe::write_str(fd, b"\n");
}

// signal_name + the `sigsafe` write primitives moved to `pal::signal`
// (PAL Phase 3); see the imports at the top of this file.

#[cfg(test)]
#[path = "crash_report_tests.rs"]
mod crash_report_tests;
