//! Helper binary for `tests/signal_handler_e2e.rs`. Installs the z42 signal
//! handler, creates a fake VmContext (so the call-stack walk has something
//! to report), then raises the signal named in argv[1].
//!
//! Usage: `signal_crash_helper {SIGSEGV|SIGABRT|SIGFPE|SIGILL|SIGBUS}`
//!
//! The parent test process spawns this with `Command::new(...).args([sig]).output()`,
//! reads stderr, and checks the captured z42 marker / call stack.

#[cfg(unix)]
fn main() {
    use std::process::ExitCode;

    // Install hooks like a real z42vm boot would. `install` now takes the
    // resolved crash dir instead of reading `Z42_CRASH_DIR` itself
    // (fix-phase1-knobs-bypass-config); this helper has no config-file layer,
    // so the process-wide config — which lazily falls back to env-only — gives
    // exactly the behaviour the parent test's `cmd.env("Z42_CRASH_DIR", ..)`
    // expects.
    z42::signal_handler::install(z42::config::runtime_config().crash_dir.as_deref());

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
