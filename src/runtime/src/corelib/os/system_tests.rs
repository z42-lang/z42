//! `Std.OperatingSystem` builtin unit tests.

use super::*;
use crate::object::Value;
use crate::vm_context::VmContext;

#[test]
fn pid_is_positive() {
    let ctx = VmContext::new();
    let pid = system_pid()
        else { panic!("expected I64"); };
    assert!(pid > 0, "pid must be positive, got {pid}");
}

#[test]
fn exe_path_either_path_or_empty() {
    let ctx = VmContext::new();
    let p = system_exe_path()
        else { panic!("expected Str"); };
    // In test harness, exe is the test binary — should not be empty.
    assert!(!p.is_empty(), "test binary path should be non-empty");
}

#[test]
fn cwd_non_empty_in_test_harness() {
    let ctx = VmContext::new();
    let p = system_cwd()
        else { panic!("expected Str"); };
    assert!(!p.is_empty());
}

#[test]
fn set_cwd_then_cwd_reflects() {
    let ctx = VmContext::new();
    let orig = system_cwd()
        else { panic!() };

    let tmp = std::env::temp_dir();
    let tmp_str = tmp.to_string_lossy().into_owned();
    system_set_cwd(&tmp_str).unwrap();

    let after = system_cwd()
        else { panic!() };
    // macOS resolves /tmp → /private/tmp via symlink; accept either.
    assert!(after == tmp_str || after.ends_with(tmp.file_name().unwrap().to_str().unwrap()),
        "after={after}, expected to end with tmp dir name");

    // Restore original cwd so other tests aren't affected.
    let _ = system_set_cwd(&orig);
}

#[test]
fn hostname_non_empty_on_unix() {
    let ctx = VmContext::new();
    let h = system_hostname()
        else { panic!("expected Str"); };
    #[cfg(unix)]
    assert!(!h.is_empty(), "unix hostname should be non-empty in CI");
    #[cfg(not(unix))]
    let _ = h;  // windows / wasm: allowed to be empty
}

#[test]
fn cpu_count_at_least_one() {
    let ctx = VmContext::new();
    let n = system_cpu_count()
        else { panic!("expected I64"); };
    assert!(n >= 1, "cpu count {n} must be >= 1");
}

#[test]
fn os_version_returns_string() {
    let ctx = VmContext::new();
    let v = system_os_version()
        else { panic!("expected Str"); };
    #[cfg(unix)]
    assert!(!v.is_empty(), "unix uname should produce non-empty string");
    #[cfg(not(unix))]
    let _ = v;
}
