//! OS signal handler — z42 call stack capture on hard crash.
//!
//! Phase 2 of the panic-hook story (Phase 1 = Rust panic hook in main.rs).
//! Spec: `docs/spec/changes/add-os-signal-handler/` (2026-05-25).
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
//! Walking the z42 call stack requires reading
//! [`vm_context::VM_CORES`] → each `Arc<VmCore>` → each
//! `VmContext.call_stack`. All three are behind `Mutex`. Handler uses
//! `try_lock` everywhere — on contention, writes a placeholder and
//! continues. This trades capture completeness for deadlock safety.
//!
//! # Termination
//!
//! Handler ends by resetting the signal disposition to `SIG_DFL` and
//! re-raising — this preserves kernel coredump behaviour (`ulimit -c`).

#![cfg(unix)]

use std::sync::atomic::{AtomicI32, Ordering};

// PAL Phase 3 (add-pal-signal): OS signal primitives live in `pal::signal`;
// the call sites below (`sigsafe::*`, `signal_name(sig)`) resolve to them via
// these imports, so the z42 crash-report logic in this file stays unchanged.
use crate::pal::signal::{self, sigsafe, signal_name};

/// File descriptor for the optional `Z42_CRASH_DIR` persistence file.
/// `-1` = no file (env var not set or open failed). Opened once at
/// [`install`] time; the OS reclaims it at process exit.
static SIGNAL_CRASH_FD: AtomicI32 = AtomicI32::new(-1);

/// Idempotency guard — second install call is a no-op.
static INSTALLED: AtomicI32 = AtomicI32::new(0);

/// Install signal handlers for the 5 covered POSIX signals.
/// Safe to call multiple times — only first call has effect.
pub fn install() {
    if INSTALLED.swap(1, Ordering::SeqCst) != 0 {
        return;
    }

    // Open crash_report fd from Z42_CRASH_DIR (if set + writable)
    open_crash_report_fd();

    // Register the z42 crash `handler` for the 5 fatal signals (OS primitive).
    signal::register_fatal_handlers(handler);

    tracing::debug!("OS signal handlers installed for SIGSEGV/SIGABRT/SIGFPE/SIGILL/SIGBUS");
}

fn open_crash_report_fd() {
    use std::os::fd::IntoRawFd;

    // adopt-inline-env-knobs (2026-09-05): the `crash-dir` knob off the layered
    // config, not a raw env read — `--set crash-dir=` / `[runtime].crash-dir` were
    // silently ignored here. (This runs at `install()` time, pre-opening the fd; the
    // signal handler itself only writes to the already-open fd, so nothing about
    // async-signal-safety changes.)
    let Some(dir_path) = crate::config::runtime_config().crash_dir.clone() else { return };
    let ts_ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir_path.join(format!("z42vm-crash-{ts_ns}.txt"));

    if let Err(e) = std::fs::create_dir_all(&dir_path) {
        tracing::warn!("Z42_CRASH_DIR {} create failed: {e}; OS signal reports go to stderr only", dir_path.display());
        return;
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => {
            let fd = file.into_raw_fd();  // leak; OS reclaims at exit
            SIGNAL_CRASH_FD.store(fd, Ordering::SeqCst);
            tracing::debug!("signal crash report fd opened: {}", path.display());
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
    sigsafe::write_str(libc::STDERR_FILENO, b"\n[z42vm signal ");
    sigsafe::write_str(libc::STDERR_FILENO, signal_name(sig));
    sigsafe::write_str(libc::STDERR_FILENO, b"]\n");

    // 2. Build banner
    write_banner(libc::STDERR_FILENO);

    // 3. Z42 call stacks (best-effort, try_lock)
    write_call_stacks(libc::STDERR_FILENO);

    // 4. Mirror to crash_report fd if configured
    let fd = SIGNAL_CRASH_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        sigsafe::write_str(fd, b"\n[z42vm signal ");
        sigsafe::write_str(fd, signal_name(sig));
        sigsafe::write_str(fd, b"]\n");
        write_banner(fd);
        write_call_stacks(fd);
        sigsafe::write_str(libc::STDERR_FILENO, b"[signal hook] crash report appended to Z42_CRASH_DIR fd\n");
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

/// Walk VM_CORES → vm_contexts → call_stack via try_lock everywhere.
/// On any lock contention, writes a placeholder for that scope and continues.
///
/// **Note**: VM_CORES uses `std::sync::Mutex` (const fn for static init);
/// inner `core.vm_contexts` and `ctx.call_stack` use `parking_lot::Mutex`
/// whose `try_lock` returns `Option<MutexGuard>` (not `Result`). Each
/// `match` arm is shaped to its mutex type.
fn write_call_stacks(fd: i32) {
    use crate::vm_context::VM_CORES;

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

        // parking_lot::Mutex::try_lock returns Option<MutexGuard>
        let contexts = match core.vm_contexts.try_lock() {
            Some(g) => g,
            None => {
                sigsafe::write_str(fd, b"  <vm_contexts lock contended>\n");
                continue;
            }
        };

        if contexts.is_empty() {
            sigsafe::write_str(fd, b"  <no VmContext>\n");
            continue;
        }

        for (ctx_idx, ctx_ptr) in contexts.iter().enumerate() {
            // SAFETY: VmContextPtr.0 raw ptr stays valid for the entire
            // lifetime of the VmContext (VmContext: !Unpin; deregistered in
            // Drop). See SAFETY block at vm_context.rs::VmContextPtr.
            let ctx_ref = unsafe { &*ctx_ptr.0 };

            sigsafe::write_str(fd, b"  thread #");
            sigsafe::write_dec_u32(fd, ctx_idx as u32);

            // ctx.call_stack: Arc<parking_lot::Mutex<Vec<VmFrame>>>
            let frames = match ctx_ref.call_stack.try_lock() {
                Some(g) => g,
                None => {
                    sigsafe::write_str(fd, b": <call_stack lock contended>\n");
                    continue;
                }
            };

            sigsafe::write_str(fd, b" (");
            sigsafe::write_dec_u32(fd, frames.len() as u32);
            sigsafe::write_str(fd, b" frame(s))\n");

            for (i, frame) in frames.iter().enumerate() {
                sigsafe::write_str(fd, b"    #");
                sigsafe::write_dec_u32(fd, i as u32);
                sigsafe::write_str(fd, b"  ");
                sigsafe::write_str(fd, frame.func_name.as_bytes());
                sigsafe::write_str(fd, b" at ");
                sigsafe::write_str(fd, frame.file.as_bytes());
                sigsafe::write_str(fd, b":");
                sigsafe::write_dec_u32(fd, frame.line.get());
                sigsafe::write_str(fd, b":");
                sigsafe::write_dec_u32(fd, frame.column.get());
                sigsafe::write_str(fd, b"\n");
            }
        }
    }
    sigsafe::write_str(fd, b"===\n");
}

// signal_name + the `sigsafe` write primitives moved to `pal::signal`
// (PAL Phase 3); see the imports at the top of this file.

#[cfg(test)]
#[path = "signal_handler_tests.rs"]
mod signal_handler_tests;
