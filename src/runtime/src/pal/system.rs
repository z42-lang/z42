//! System-level platform queries — hostname, OS version, etc.
//!
//! review.md Part 1 P2 Phase 1 (add-pal-system-phase1): first
//! concern migrated to the new `pal/` layer. Public functions return
//! OS-neutral types (`Option<String>` / `String`); the unix / non-unix
//! split lives entirely inside this file, so consumers (currently
//! `corelib/system.rs`) carry zero `#[cfg(...)]` blocks.

/// Network hostname of the machine running this VM. Returns `None` when
/// the underlying syscall failed or isn't implemented on this platform
/// (Windows / WASM today — windows impl via `GetComputerNameW` lands in
/// a follow-up spec once a Windows CI runner exists).
pub fn hostname() -> Option<String> {
    hostname_impl()
}

/// Human-readable OS version string (e.g. `"Darwin 24.6.0 22.6.0"` on
/// macOS, where the three fields are `sysname` / `release` / `version`
/// from `uname(2)`). Empty string when the syscall fails or isn't
/// implemented.
///
/// Format is OS-dependent — callers wanting structured fields should
/// switch on `os_kind()` (Phase 2) and parse appropriately.
pub fn os_version() -> String {
    os_version_impl()
}

/// Short OS name: `std::env::consts::OS`, except that every wasm32 target
/// (`wasm32-unknown-unknown` reports `"unknown"`, WASI reports `"wasi"`) is `"wasm"`.
pub fn os_name() -> &'static str {
    if cfg!(target_arch = "wasm32") { "wasm" } else { std::env::consts::OS }
}

// ── unix impls ───────────────────────────────────────────────────────────────

#[cfg(unix)]
fn hostname_impl() -> Option<String> {
    use std::ffi::CStr;
    let mut buf = vec![0u8; 256];
    let rc = unsafe {
        libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len())
    };
    if rc != 0 {
        return None;
    }
    // gethostname produces a NUL-terminated C string; find the first NUL.
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const libc::c_char) };
    Some(cstr.to_string_lossy().into_owned())
}

#[cfg(unix)]
fn os_version_impl() -> String {
    let mut utsname: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut utsname) } != 0 {
        return String::new();
    }
    fn cstr(arr: &[libc::c_char]) -> String {
        let p = arr.as_ptr();
        let cstr = unsafe { std::ffi::CStr::from_ptr(p) };
        cstr.to_string_lossy().into_owned()
    }
    format!(
        "{} {} {}",
        cstr(&utsname.sysname),
        cstr(&utsname.release),
        cstr(&utsname.version),
    )
}

// ── wasm32 impls ─────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
fn hostname_impl() -> Option<String> {
    // WASM sandbox has no network identity. Graceful degrade.
    None
}

#[cfg(target_arch = "wasm32")]
fn os_version_impl() -> String {
    String::from("wasm")
}

// ── other non-unix (windows today) ───────────────────────────────────────────

#[cfg(all(not(unix), not(target_arch = "wasm32")))]
fn hostname_impl() -> Option<String> {
    // Windows: real impl via `GetComputerNameW` lands once a Windows
    // CI runner is in the loop; graceful degrade for now.
    None
}

#[cfg(all(not(unix), not(target_arch = "wasm32")))]
fn os_version_impl() -> String {
    String::new()
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod system_tests;
