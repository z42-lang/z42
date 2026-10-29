//! Unit tests for `pal::system`. Behaviour-focused (don't assert specific
//! hostnames since the CI sandbox has no stable name), but verify that
//! the syscalls succeed where they should.

use super::*;

#[test]
fn hostname_returns_some_on_unix() {
    if cfg!(unix) {
        // Every unix host has a hostname; `Some("")` is also acceptable
        // (a misconfigured host can have a blank one) but `None` would
        // indicate the syscall itself failed.
        let h = hostname();
        assert!(h.is_some(), "hostname() must succeed on unix");
    } else {
        // Windows / WASM stub returns None — confirm the contract.
        assert!(hostname().is_none());
    }
}

#[test]
fn os_version_is_non_empty_on_unix() {
    if cfg!(unix) {
        let v = os_version();
        assert!(!v.is_empty(), "os_version() must be non-empty on unix; got {v:?}");
        // sysname is space-separated from release; expect at least one space.
        assert!(v.contains(' '),
            "os_version() format includes sysname / release / version; got {v:?}");
    } else if cfg!(target_arch = "wasm32") {
        assert_eq!(os_version(), "wasm");
    } else {
        // Windows-without-impl stub returns empty.
        assert_eq!(os_version(), "");
    }
}

#[test]
fn hostname_and_os_version_are_callable_repeatedly() {
    // Idempotent / no panics on repeated invocation (sanity check that
    // there's no `OnceLock` swallowing the second call or similar).
    for _ in 0..3 {
        let _ = hostname();
        let _ = os_version();
    }
}

#[test]
fn os_name_is_non_empty() {
    assert!(!super::os_name().is_empty());
}
