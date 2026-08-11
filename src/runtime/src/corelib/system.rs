//! `Std.OperatingSystem` builtins — process + machine info.
//!
//! Cross-platform impls now flow through `crate::pal::system` (review.md
//! Part 1 P2 Phase 1, add-pal-system-phase1, 2026-06-03). This file is
//! just the builtin-dispatch layer that wraps PAL calls into VM `Value`s.
//!
//! 2026-05-14 add-platform-os-stdlib (original landing).

use super::convert::arg_str;
use crate::metadata::Value;
use crate::pal;
use crate::vm_context::VmContext;
use anyhow::Result;

// fix-wasm-corpus-capability-gate: std::process::id / std::env::current_exe /
// current_dir / set_current_dir all panic on wasm32 ("not supported"). Degrade
// to browser-safe values so infrastructure paths don't abort the VM module.
pub fn builtin_system_pid(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    let pid = std::process::id() as i64;
    #[cfg(target_arch = "wasm32")]
    let pid = 0i64;
    Ok(Value::I64(pid))
}

pub fn builtin_system_exe_path(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    let exe = match std::env::current_exe() {
        Ok(p)  => p.to_string_lossy().into_owned(),
        Err(_) => String::new(),
    };
    #[cfg(target_arch = "wasm32")]
    let exe = String::new();
    Ok(Value::Str(exe.into()))
}

pub fn builtin_system_cwd(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    #[cfg(not(target_arch = "wasm32"))]
    let cwd = match std::env::current_dir() {
        Ok(p)  => p.to_string_lossy().into_owned(),
        Err(_) => String::new(),
    };
    #[cfg(target_arch = "wasm32")]
    let cwd = "/".to_string();
    Ok(Value::Str(cwd.into()))
}

pub fn builtin_system_set_cwd(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let path = arg_str(args, 0, "__system_set_cwd")?;
    #[cfg(not(target_arch = "wasm32"))]
    std::env::set_current_dir(path)?;
    #[cfg(target_arch = "wasm32")]
    let _ = path;
    Ok(Value::Null)
}

pub fn builtin_system_hostname(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    Ok(Value::Str(pal::system::hostname().unwrap_or_default().into()))
}

pub fn builtin_system_cpu_count(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let n = std::thread::available_parallelism()
        .map(|n| n.get() as i64)
        .unwrap_or(1);
    Ok(Value::I64(n))
}

pub fn builtin_system_os_version(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    Ok(Value::Str(pal::system::os_version().into()))
}

// add-pal-system-phase1 (2026-06-03): the unix / wasm / windows-stub
// branches that used to live inline here now sit behind `crate::pal::system::*`.
// Future PAL concerns (fs / signal / thread / mem) follow the same pattern —
// see `docs/design/runtime/pal.md` for the migration plan.

#[cfg(test)]
#[path = "system_tests.rs"]
mod system_tests;
