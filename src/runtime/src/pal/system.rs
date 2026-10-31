//! System queries and process-environment operations.
//! The OS-neutral API delegates to the compile-time backend in `pal/sys/`.

/// Network hostname of the machine running this VM. Returns `None` when
/// the underlying syscall failed or isn't implemented on this platform
/// (the Windows and wasm backends currently return `None`).
pub fn hostname() -> Option<String> {
    super::sys::system::hostname()
}

/// Human-readable OS version string (e.g. `"Darwin 24.6.0 22.6.0"` on
/// macOS, where the three fields are `sysname` / `release` / `version`
/// from `uname(2)`). Empty string when the syscall fails or isn't
/// implemented.
///
/// Format is OS-dependent — callers wanting structured fields should
/// switch on `os_kind()` (Phase 2) and parse appropriately.
pub fn os_version() -> String {
    super::sys::system::os_version()
}

/// Short OS name: `std::env::consts::OS`, except that every wasm32 target
/// (`wasm32-unknown-unknown` reports `"unknown"`, WASI reports `"wasi"`) is `"wasm"`.
pub fn os_name() -> &'static str {
    super::sys::system::os_name()
}


/// This process's id; `0` without a process environment.
pub fn pid() -> u32 {
    super::sys::system::pid()
}

/// Path of the running executable; `None` when unknown.
pub fn current_exe() -> Option<std::path::PathBuf> {
    super::sys::system::current_exe()
}

/// The working directory; `/` without a process environment.
pub fn current_dir() -> std::io::Result<std::path::PathBuf> {
    super::sys::system::current_dir()
}

/// Change the working directory; a no-op without a process environment.
pub fn set_current_dir(path: &str) -> std::io::Result<()> {
    super::sys::system::set_current_dir(path)
}

/// The temp directory; `/tmp` without a process environment.
pub fn temp_dir() -> std::path::PathBuf {
    super::sys::system::temp_dir()
}

/// Set (`Some`) or remove (`None`) an environment variable; a no-op without a process
/// environment.
pub fn set_env(name: &str, value: Option<&str>) {
    super::sys::system::set_env(name, value)
}

/// What the target OS lets a program do, independent of cargo features.
#[derive(Debug, Clone, Copy)]
pub struct OsCaps {
    pub threads: bool,
    pub fs: bool,
    pub clock: bool,
    pub entropy: bool,
    /// Spawning child processes.
    pub process: bool,
    pub tty: bool,
    /// A writable process environment that child processes see.
    pub env: bool,
    /// Listening sockets (accepting inbound connections).
    pub socket_listen: bool,
    pub hardlink: bool,
}


/// The capabilities of the target this binary was built for.
pub const OS_CAPS: OsCaps = super::sys::system::OS_CAPS;

#[cfg(test)]
#[path = "system_tests.rs"]
mod system_tests;
