//! Fatal-signal OS primitives — registration, signal-name table, disposition
//! reset, and the async-signal-safe write helpers a handler may use. POSIX
//! signals only: elsewhere (Windows, wasm) registration reports `false`, the
//! writers and the reset are no-ops, so the crash reporter compiles unchanged.
//!
//! OS primitives are provided by `pal/sys/`; the VM crash reporter remains in
//! `signal_handler.rs`. The generic formatting helpers never allocate.
//!
//! # Async-signal safety
//!
//! `register_fatal_handlers` runs at install time (normal context). The
//! `sigsafe::*` writers + `signal_name` + `reset_default_and_reraise` are the
//! ONLY items here callable from async-signal context: they touch nothing but
//! raw `libc::write` / atomics / `libc::raise` (no allocation, no locks, no
//! stdio).

/// The standard-error file descriptor the `sigsafe` writers take.
pub const STDERR_FD: i32 = 2;

/// Register the same `handler` for the 5 covered fatal POSIX signals
/// (SIGSEGV / SIGABRT / SIGFPE / SIGILL / SIGBUS). Registration failures are
/// logged and skipped (best-effort). The caller's `handler` must invoke only
/// async-signal-safe primitives (the `sigsafe::*` helpers below qualify).
/// Returns `false` where the target has no POSIX signals.
pub fn register_fatal_handlers(handler: extern "C" fn(i32)) -> bool {
    super::sys::signal::register_fatal_handlers(handler)
}

/// Hand `file`'s descriptor over to the caller for the rest of the process
/// (the OS reclaims it at exit), for the `sigsafe` writers. `None` where the
/// writers have no descriptor to write to.
pub fn leak_fd(file: std::fs::File) -> Option<i32> {
    super::sys::signal::leak_fd(file)
}

/// Reset `sig`'s disposition to the kernel default and re-raise it, so the
/// default action (coredump if `ulimit -c` permits) takes over after a handler
/// has finished its best-effort report. Async-signal-safe.
pub fn reset_default_and_reraise(sig: i32) {
    super::sys::signal::reset_default_and_reraise(sig)
}

/// Map signal number to a constant string. Returns `b"UNKNOWN"` for any signal
/// outside the covered fatal set. Async-signal-safe (const match, no alloc).
pub fn signal_name(sig: i32) -> &'static [u8] {
    super::sys::signal::signal_name(sig)
}

/// Async-signal-safe write primitives. **Only these (plus [`signal_name`] /
/// [`reset_default_and_reraise`]) may be invoked from within a signal handler.**
/// Uses raw `libc::write(2)` syscalls + stack buffers (no allocation, no Mutex,
/// no stdio lock).
pub mod sigsafe {
    /// Write a byte slice to a file descriptor. Handles partial writes; on
    /// any negative return (including EINTR) gives up silently — process is
    /// already crashing, partial output is better than infinite loop.
    pub fn write_str(fd: i32, bytes: &[u8]) {
        super::super::sys::signal::write_str(fd, bytes)
    }

    /// Write a base-10 representation of `v` to `fd`. No allocation.
    pub fn write_dec_u32(fd: i32, v: u32) {
        if v == 0 { write_str(fd, b"0"); return; }
        let mut buf = [0u8; 10];  // u32::MAX is 10 digits
        let mut i = 10usize;
        let mut n = v;
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        write_str(fd, &buf[i..]);
    }

    /// Write a `0x`-prefixed hex representation of `v` to `fd`. No allocation.
    ///
    /// Held in reserve for stack-pointer / instruction-pointer attribution
    /// (signal `si_addr`) — not yet wired into the crash reporter.
    #[allow(dead_code)]
    pub fn write_hex_u64(fd: i32, v: u64) {
        if v == 0 { write_str(fd, b"0x0"); return; }
        let mut buf = [0u8; 16];  // u64 = 16 hex nibbles max
        let mut i = 16usize;
        let mut n = v;
        let hex = b"0123456789abcdef";
        while n > 0 && i > 0 {
            i -= 1;
            buf[i] = hex[(n & 0xf) as usize];
            n >>= 4;
        }
        write_str(fd, b"0x");
        write_str(fd, &buf[i..]);
    }
}

#[cfg(test)]
#[path = "signal_tests.rs"]
mod signal_tests;
