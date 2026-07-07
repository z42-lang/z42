//! End-to-end tests for the OS signal handler. Spawns the
//! `signal_crash_helper` example binary, raises a fatal signal, and
//! asserts the handler wrote the expected marker + z42 call stack header
//! to stderr (plus optional crash file under `Z42_CRASH_DIR`).
//!
//! All tests are `#[cfg(unix)]`-gated; on Windows the entire file is
//! a no-op (Phase 2.1 will add Vectored Exception Handler tests).

#![cfg(unix)]

use std::path::PathBuf;
use std::process::Command;

/// Locate the helper binary. Search candidates in priority order:
/// 1. `CARGO_TARGET_DIR/<profile>/examples/signal_crash_helper` (when env var set)
/// 2. `<crate-root>/../../artifacts/build/runtime/<profile>/examples/...` (z42 default
///    via .cargo/config.toml `target-dir`)
/// 3. `<crate-root>/target/<profile>/examples/...` (vanilla cargo default)
fn helper_path() -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. honor CARGO_TARGET_DIR
    if let Some(td) = std::env::var_os("CARGO_TARGET_DIR") {
        let td = PathBuf::from(td);
        for sub in ["debug/examples", "release/examples"] {
            candidates.push(td.join(sub).join("signal_crash_helper"));
        }
    }

    // 2. z42 repo default (target-dir = artifacts/build/runtime relative to repo root)
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = crate_root.parent().and_then(|p| p.parent()).unwrap_or(&crate_root);
    let z42_target = repo_root.join("artifacts/build/runtime");
    for sub in ["debug/examples", "release/examples"] {
        candidates.push(z42_target.join(sub).join("signal_crash_helper"));
    }

    // 3. vanilla cargo default — crate-local target/
    let local_target = crate_root.join("target");
    for sub in ["debug/examples", "release/examples"] {
        candidates.push(local_target.join(sub).join("signal_crash_helper"));
    }

    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    panic!(
        "signal_crash_helper binary not found. Searched:\n  {}\nRun: cargo build --example signal_crash_helper",
        candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n  ")
    );
}

/// Spawn helper raising the given signal, return (stderr bytes, exit code).
fn run_helper(sig_name: &str, crash_dir: Option<&std::path::Path>) -> (Vec<u8>, Option<i32>) {
    let mut cmd = Command::new(helper_path());
    cmd.arg(sig_name);
    cmd.env_remove("RUST_BACKTRACE");
    if let Some(dir) = crash_dir {
        cmd.env("Z42_CRASH_DIR", dir);
    } else {
        cmd.env_remove("Z42_CRASH_DIR");
    }
    let output = cmd.output().expect("spawn signal_crash_helper");
    (output.stderr, output.status.code())
}

fn assert_marker(stderr: &[u8], sig_name: &str) {
    let s = String::from_utf8_lossy(stderr);
    assert!(
        s.contains(&format!("[z42vm signal {sig_name}]")),
        "stderr missing signal marker for {sig_name}; got:\n{s}"
    );
    assert!(
        s.contains(&format!("z42vm {}", env!("CARGO_PKG_VERSION"))),
        "stderr missing build banner; got:\n{s}"
    );
    assert!(
        s.contains("=== z42 call stack"),
        "stderr missing call stack header; got:\n{s}"
    );
    assert!(
        s.contains("VmCore"),
        "stderr missing VmCore marker; got:\n{s}"
    );
}

// On macOS, `Command.status.code()` returns None when child terminated by
// signal (the actual signal info is in `.signal()` from UnixExt). The kernel
// receives the re-raised signal and terminates the process accordingly —
// from the test's perspective code() is None and signal() is Some(<sig>).
fn assert_signaled(code: Option<i32>, expected_sig: i32, stderr: &[u8]) {
    // Under POSIX, when a child is killed by a signal, std::process::ExitStatus::code()
    // returns None (the signal info is in .signal() via ExitStatusExt). The marker
    // assertion above already proves it was OUR signal — here we just confirm the
    // process didn't return normally.
    if code.is_some() {
        let s = String::from_utf8_lossy(stderr);
        panic!(
            "expected process to be killed by signal {expected_sig}, got exit code {:?}; stderr:\n{s}",
            code
        );
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[test]
fn sigsegv_captures_marker() {
    let (stderr, code) = run_helper("SIGSEGV", None);
    assert_marker(&stderr, "SIGSEGV");
    assert_signaled(code, libc::SIGSEGV, &stderr);
}

#[test]
fn sigabrt_captures_marker() {
    let (stderr, code) = run_helper("SIGABRT", None);
    assert_marker(&stderr, "SIGABRT");
    assert_signaled(code, libc::SIGABRT, &stderr);
}

#[test]
fn sigfpe_captures_marker() {
    let (stderr, code) = run_helper("SIGFPE", None);
    assert_marker(&stderr, "SIGFPE");
    assert_signaled(code, libc::SIGFPE, &stderr);
}

#[test]
fn sigill_captures_marker() {
    let (stderr, code) = run_helper("SIGILL", None);
    assert_marker(&stderr, "SIGILL");
    assert_signaled(code, libc::SIGILL, &stderr);
}

#[test]
fn sigbus_captures_marker() {
    let (stderr, code) = run_helper("SIGBUS", None);
    assert_marker(&stderr, "SIGBUS");
    assert_signaled(code, libc::SIGBUS, &stderr);
}

#[test]
fn z42_crash_dir_writes_file() {
    let tempdir = std::env::temp_dir().join(format!(
        "z42-signal-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&tempdir).unwrap();

    let (stderr, _code) = run_helper("SIGABRT", Some(&tempdir));
    assert_marker(&stderr, "SIGABRT");

    // Read any z42vm-crash-*.txt files in the temp dir
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&tempdir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().starts_with("z42vm-crash-") {
            let content = std::fs::read_to_string(entry.path()).unwrap();
            found.push(content);
        }
    }
    assert_eq!(
        found.len(),
        1,
        "expected exactly one crash file under {}; found {}",
        tempdir.display(),
        found.len()
    );
    let content = &found[0];
    assert!(content.contains("[z42vm signal SIGABRT]"),
        "crash file missing marker; content:\n{content}");
    assert!(content.contains("=== z42 call stack"),
        "crash file missing call stack header; content:\n{content}");

    let _ = std::fs::remove_dir_all(&tempdir);
}

#[test]
fn z42_crash_dir_unwritable_falls_back_to_stderr() {
    // /no/such/path/ — install logs warn, signal still captures to stderr
    let bogus = std::path::PathBuf::from("/no/such/path/that/should/not/exist");
    let (stderr, _code) = run_helper("SIGABRT", Some(&bogus));
    assert_marker(&stderr, "SIGABRT");
    // No file written — nothing to check; if helper hung or crashed differently
    // the marker assert above would have failed.
}
