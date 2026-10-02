//! `Std.Platform` builtins — OS / architecture / family identity.
//!
//! All values pass through from `std::env::consts::{OS, ARCH, FAMILY}` which
//! are compile-time constants from rustc's target triple — except OS on wasm32,
//! where rustc reports `""` and we report `"wasm"` (see [`HOST_OS`]). The Kind value
//! mapping below must stay in lockstep with
//! `src/libraries/z42.io/src/Platform.z42` constants `OSKind::*` /
//! `ArchKind::*` (the z42 stdlib spec lists the canonical values).
//!
//! 2026-05-14 add-platform-os-stdlib.

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;

/// 宿主 OS 名。其余平台就是 `std::env::consts::OS`；**wasm32 例外**：
/// `wasm32-unknown-unknown` 的 `consts::OS` 是空串，不是 `"wasm"`。旧写法照搬它，
/// 于是 wasm 上 `Platform.OS()` 为 `""`、`OSKindValue()` 落进 `_ => 0`、`IsWasm()` 恒假、
/// `[Skip(platform: "wasm")]` 永不生效 —— 契约（`OSKind.Wasm = 6`）从来没在 wasm 上兑现过
/// （fix-wasm-std-time 发现：nightly 全红期间没人跑到这里）。
const HOST_OS: &str = if cfg!(target_arch = "wasm32") { "wasm" } else { std::env::consts::OS };

pub fn builtin_platform_os(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    Ok(Value::Str(HOST_OS.to_string().into()))
}

pub fn builtin_platform_arch(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    Ok(Value::Str(std::env::consts::ARCH.to_string().into()))
}

pub fn builtin_platform_family(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    Ok(Value::Str(std::env::consts::FAMILY.to_string().into()))
}

/// Keep in sync with `Std.OSKind` in
/// `src/libraries/z42.core/src/Platform.z42`.
pub fn builtin_platform_os_kind(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let kind: i64 = match HOST_OS {
        "linux"   => 1,
        "macos"   => 2,
        "windows" => 3,
        "android" => 4,
        "ios"     => 5,
        "wasm"    => 6,
        "freebsd" => 7,
        _         => 0,
    };
    Ok(Value::I64(kind))
}

/// Keep in sync with `Std.ArchKind` in
/// `src/libraries/z42.core/src/Platform.z42`. Names use the .NET-style
/// short forms (X64 / Arm64 / Wasm / X86); the integer values are what
/// matters at the ABI boundary.
pub fn builtin_platform_arch_kind(_ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let kind: i64 = match std::env::consts::ARCH {
        "x86_64"  => 1,
        "aarch64" => 2,
        "wasm32"  => 3,
        "x86"     => 4,
        _         => 0,
    };
    Ok(Value::I64(kind))
}

/// `__platform_caps() -> string[]` — runtime capability set of THIS binary.
///
/// add-exec-profile-matrix (2026-07-31): the exec-profile bench/test harness
/// probes the target VM binary with `Std.Platform.Capabilities()` and tags each
/// result with the caps the binary actually reports — never a static guess. The
/// values are the ground truth of "what was compiled in":
///   - cargo-feature caps (`jit` / `native-interop` / `bundled-compression`) via cfg.
///   - `threads`: real OS-thread support (`corelib::threading`) is compiled in
///     wherever `std::thread` exists — i.e. everywhere except wasm. It is *not*
///     a cargo feature, so it is gated on `target_arch` directly (mirrors the
///     wasm `interp-only` preset which also has no threads).
///   - OS-facility caps (`fs` / `clock` / `entropy` / `process` / `tty` / `env` /
///     `socket-listen` / `hardlink`): what the TARGET's process environment
///     provides, gated on `target_arch` / `target_os`. Three tiers:
///       · wasm32 — none of them (no real fs, no SystemTime, no OS entropy …);
///       · ios / android — a real native process inside an APP SANDBOX: fs, clock and
///         entropy yes; no fork/exec, no TTY, no mutable env / desktop OS identity,
///         no server sockets (bind/listen is denied, CI has no network); Android's
///         sandbox additionally denies POSIX hard links;
///       · desktop — all of them.
///     These back `// requires-caps:` / `[Skip(feature:)]` in the test corpus: the
///     on-device bundle runner skips a case whose declared caps are not all here.
///     The vocabulary (and which caps are live) is docs/internals/src/devinfra/test-layout.md §4;
///     `xtask test layout` reconciles that list with the pushes below.
/// Order is stable (declaration order) so probe output is deterministic.
pub fn builtin_platform_caps(ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let mut caps: Vec<&str> = Vec::new();
    #[cfg(feature = "jit")]                 caps.push("jit");
    #[cfg(feature = "native-interop")]      caps.push("native-interop");
    #[cfg(feature = "bundled-compression")] caps.push("bundled-compression");
    #[cfg(not(target_arch = "wasm32"))]     caps.push("threads");
    // fix-wasm-corpus-capability-gate: `socket` = real OS networking (TCP/UDP/HTTP/
    // WS) — compiled in wherever `std::net` works, i.e. everywhere except wasm.
    #[cfg(not(target_arch = "wasm32"))]     caps.push("socket");
    #[cfg(not(target_arch = "wasm32"))]     caps.push("fs");
    #[cfg(not(target_arch = "wasm32"))]     caps.push("clock");
    #[cfg(not(target_arch = "wasm32"))]     caps.push("entropy");
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios", target_os = "android")))] caps.push("process");
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios", target_os = "android")))] caps.push("tty");
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios", target_os = "android")))] caps.push("env");
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios", target_os = "android")))] caps.push("socket-listen");
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]                    caps.push("hardlink");
    let list: Vec<Value> = caps.into_iter().map(|s| Value::Str(s.to_string().into())).collect();
    Ok(ctx.heap().alloc_array(list))
}

/// `__platform_exec_modes() -> string[]` — execution backends this binary can
/// dispatch. `interp` is always present; `jit` / `aot` are cfg-gated. Mirrors
/// the `exec modes:` line of `main.rs::print_build_info`.
///
/// NOTE: an `aot`-feature build lists `"aot"` here even though `aot.rs` is a
/// stub — this reports "compiled in", not "executable". The exec-profile support
/// matrix's *policy overlay* is what marks any `aot_pkgs ≠ []` composition as
/// `skipped-not-yet` (M9), independent of this list.
pub fn builtin_platform_exec_modes(ctx: &VmContext, _: &[Value]) -> Result<Value> {
    let mut modes: Vec<&str> = vec!["interp"];
    #[cfg(feature = "jit")] modes.push("jit");
    #[cfg(feature = "aot")] modes.push("aot");
    let list: Vec<Value> = modes.into_iter().map(|s| Value::Str(s.to_string().into())).collect();
    Ok(ctx.heap().alloc_array(list))
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod platform_tests;
