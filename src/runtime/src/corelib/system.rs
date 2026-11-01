//! `Std.OperatingSystem` builtins — process + machine info.
//!
//! Cross-platform impls now flow through `crate::pal::system` (review.md
//! Part 1 P2 Phase 1, add-pal-system-phase1). This file is
//! just the builtin-dispatch layer that wraps PAL calls into VM `Value`s.
//!
//!  add-platform-os-stdlib (original landing).

use super::convert::arg_str;
use crate::object::Value;
use crate::pal;
use crate::vm_context::VmContext;
use anyhow::Result;
use crate::corelib::abi::BuiltinError;

pub fn system_pid() -> i64 {
    pal::system::pid() as i64
}

pub fn system_exe_path() -> String {
    pal::system::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn system_cwd() -> String {
    pal::system::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn system_set_cwd(path: &str) -> Result<(), BuiltinError> {
    pal::system::set_current_dir(path).map_err(|e| BuiltinError::Io(e.to_string()))
}

pub fn system_hostname() -> String {
    pal::system::hostname().unwrap_or_default()
}

pub fn system_cpu_count() -> i64 {
    std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1)
}

pub fn system_os_version() -> String {
    pal::system::os_version()
}

// add-pal-system-phase1 : the unix / wasm / windows-stub
// branches that used to live inline here now sit behind `crate::pal::system::*`.
// Future PAL concerns (fs / signal / thread / mem) follow the same pattern —
// see `docs/internals/src/runtime/pal.md` for the migration plan.

#[cfg(test)]
#[path = "system_tests.rs"]
mod system_tests;

/// 系统信息迁 builtin ABI v2(P1-8 批次 12):查询全 PURE;SetCwd 的 IO 错误 →
/// IOException;wasm 桩值原样保留。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__system_pid", pure)]
    pub fn pid() -> i64 { system_pid() }

    #[builtin(name = "__system_exe_path", pure)]
    pub fn exe_path() -> String { system_exe_path() }

    #[builtin(name = "__system_cwd", pure)]
    pub fn cwd() -> String { system_cwd() }

    #[builtin(name = "__system_set_cwd")]
    pub fn set_cwd(path: &str) -> Result<(), BuiltinError> { system_set_cwd(path) }

    #[builtin(name = "__system_hostname", pure)]
    pub fn hostname() -> String { system_hostname() }

    #[builtin(name = "__system_cpu_count", pure)]
    pub fn cpu_count() -> i64 { system_cpu_count() }

    #[builtin(name = "__system_os_version", pure)]
    pub fn os_version() -> String { system_os_version() }
}
