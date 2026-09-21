//! Helper binary for `tests/signal_handler_e2e.rs`. Installs the z42 signal
//! handler, creates a fake VmContext (so the call-stack walk has something
//! to report), then raises the signal named in argv[1].
//!
//! Usage: `signal_crash_helper {SIGSEGV|SIGABRT|SIGFPE|SIGILL|SIGBUS}`
//!
//! The parent test process spawns this with `Command::new(...).args([sig]).output()`,
//! reads stderr, and checks the captured z42 marker / call stack.

/// Detach this process from the macOS crash reporter before dying.
///
/// # Why
///
/// On macOS a process killed by a fatal signal is turned into a *corpse* and
/// handed to `ReportCrash` via the Mach exception ports. `ReportCrash` then
/// releases the corpse. When that pipeline is saturated or stuck — a developer
/// machine that has accumulated crash reports, or a sandbox where the reporter
/// can't run — the corpse is never released and the process sits in state `UE`
/// (uninterruptible + exiting) **forever**; `kill -9` does not touch it.
///
/// This helper dies by design on every run, so it walks straight into that. The
/// parent's `Command::output()` waits for the pipe to close, which never happens,
/// and the whole `cargo test` wedges — historically for 1h51m before someone
/// killed it. See `.github/workflows/ci.yml` (why `test runtime` is not part of
/// `test all`) and `docs/spec/archive/2026-07-07-redesign-xtask-test/design.md`.
///
/// Zeroing `EXC_MASK_CRASH` / `EXC_MASK_CORPSE_NOTIFY` cuts exactly one edge:
/// "kernel → crash reporter". Everything the tests assert is untouched — the
/// BSD signal handler still runs and writes the marker + call stack, and the
/// process is still *killed by the signal* (verified: exit status 134 = 128+SIGABRT,
/// so `assert_signaled`'s `.code() == None` still holds). The process simply
/// stops leaving a corpse behind.
///
/// No-op off macOS; other platforms have no such reporter in this path.
#[cfg(all(unix, target_os = "macos"))]
fn detach_from_crash_reporter() {
    // Declared here rather than pulling in a `mach` crate — two symbols, and this
    // is a test-only example binary. Values read out of <mach/exception_types.h>
    // and <mach/thread_status.h> on the macOS SDK (printed from a C probe, not
    // guessed — THREAD_STATE_NONE is 5, not 0).
    const EXC_MASK_CRASH: u32 = 1 << 10; // EXC_CRASH         == 1024
    const EXC_MASK_CORPSE_NOTIFY: u32 = 1 << 13; // EXC_CORPSE_NOTIFY == 8192
    const EXCEPTION_DEFAULT: i32 = 1;
    const THREAD_STATE_NONE: i32 = 5;
    const MACH_PORT_NULL: libc::mach_port_t = 0;

    unsafe extern "C" {
        fn task_set_exception_ports(
            task: libc::mach_port_t,
            exception_mask: u32,
            new_port: libc::mach_port_t,
            behavior: i32,
            new_flavor: i32,
        ) -> libc::kern_return_t;
        // `mach_task_self()` is a macro over this global in <mach/mach_init.h>;
        // `libc::mach_task_self()` is deprecated in favour of the `mach2` crate,
        // which we don't want to depend on for one symbol.
        static mach_task_self_: libc::mach_port_t;
    }

    // SAFETY: plain Mach trap on our own task port, installing a null exception
    // port. Failure is not fatal — worst case we are back to the old behaviour,
    // so the return value is deliberately ignored.
    unsafe {
        let _ = task_set_exception_ports(
            mach_task_self_,
            EXC_MASK_CRASH | EXC_MASK_CORPSE_NOTIFY,
            MACH_PORT_NULL,
            EXCEPTION_DEFAULT,
            THREAD_STATE_NONE,
        );
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detach_from_crash_reporter() {}

#[cfg(unix)]
fn main() {
    use std::process::ExitCode;

    // Before anything else: make sure dying here can't wedge the test run.
    detach_from_crash_reporter();

    // Install hooks like a real z42vm boot would.
    z42::signal_handler::install();

    // Create a VmContext so the stack-walk has something to report. We do
    // NOT push any frames — empty call_stack still proves the walk worked.
    let _ctx = z42::vm_context::VmContext::new();

    let sig_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| {
            eprintln!("usage: signal_crash_helper {{SIGSEGV|SIGABRT|SIGFPE|SIGILL|SIGBUS}}");
            std::process::exit(2);
        });

    let sig = match sig_name.as_str() {
        "SIGSEGV" => libc::SIGSEGV,
        "SIGABRT" => libc::SIGABRT,
        "SIGFPE"  => libc::SIGFPE,
        "SIGILL"  => libc::SIGILL,
        "SIGBUS"  => libc::SIGBUS,
        other     => {
            eprintln!("unknown signal: {other}");
            std::process::exit(2);
        }
    };

    // raise(2) is async-signal-safe by definition. We re-raise from main()
    // (not a signal context) so this is just a clean way to trigger the
    // handler installed above.
    unsafe { libc::raise(sig); }

    // If raise() returns and we still reach here, the handler must have
    // re-raised with SIG_DFL — but the kernel default for fatal signals
    // is process termination, so this line should be unreachable. Print a
    // marker for the test to detect any unexpected return.
    eprintln!("signal_crash_helper: handler returned without termination!");
    let _ = ExitCode::from(99);
}

#[cfg(not(unix))]
fn main() {
    eprintln!("signal_crash_helper is unix-only");
    std::process::exit(2);
}
