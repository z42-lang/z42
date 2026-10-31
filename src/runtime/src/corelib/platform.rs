//! `Std.Platform` builtins — OS / architecture / family identity.
//!
//! All values pass through from `std::env::consts::{OS, ARCH, FAMILY}` which
//! are compile-time constants from rustc's target triple — except OS on wasm32,
//! where rustc reports `""` and we report `"wasm"` (see [`platform_os`]). The Kind value
//! mapping below must stay in lockstep with
//! `src/libraries/z42.io/src/Platform.z42` constants `OSKind::*` /
//! `ArchKind::*` (the z42 stdlib spec lists the canonical values).
//!
//!  add-platform-os-stdlib.

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;

/// 宿主 OS 名。wasm32 报 `"wasm"` 而不是 `consts::OS`（见 [`crate::pal::system::os_name`]），
/// 否则 `OSKind.Wasm`、`IsWasm()`、`[Skip(platform: "wasm")]` 在 wasm 上都不生效。
pub fn platform_os() -> &'static str { crate::pal::system::os_name() }

pub fn platform_arch() -> &'static str { std::env::consts::ARCH }

pub fn platform_family() -> &'static str { std::env::consts::FAMILY }

/// Keep in sync with `Std.OSKind` in
/// `src/libraries/z42.core/src/Platform.z42`.
pub fn platform_os_kind() -> i64 {
    match platform_os() {
        "linux"   => 1,
        "macos"   => 2,
        "windows" => 3,
        "android" => 4,
        "ios"     => 5,
        "wasm"    => 6,
        "freebsd" => 7,
        _         => 0,
    }
}

/// Keep in sync with `Std.ArchKind` in
/// `src/libraries/z42.core/src/Platform.z42`. Names use the .NET-style
/// short forms (X64 / Arm64 / Wasm / X86); the integer values are what
/// matters at the ABI boundary.
pub fn platform_arch_kind() -> i64 {
    match std::env::consts::ARCH {
        "x86_64"  => 1,
        "aarch64" => 2,
        "wasm32"  => 3,
        "x86"     => 4,
        _         => 0,
    }
}

/// `__platform_caps() -> string[]` — runtime capability set of THIS binary.
///
/// add-exec-profile-matrix : the exec-profile bench/test harness
/// probes the target VM binary with `Std.Platform.Capabilities()` and tags each
/// result with the caps the binary actually reports — never a static guess. The
/// values are the ground truth of "what was compiled in":
///   - cargo-feature caps (`jit` / `native-interop` / `bundled-compression` / `socket` ← `net`) via cfg.
///   - `threads`: real OS-thread support (`corelib::threading`) — everywhere except
///     wasm. Not a cargo feature: it comes from `pal::system::OS_CAPS`.
///   - OS-facility caps (`fs` / `clock` / `entropy` / `process` / `tty` / `env` /
///     `socket-listen` / `hardlink`): what the TARGET's process environment
///     provides (`pal::system::OS_CAPS`). Three tiers:
///       · wasm32 — none of them (no real fs, no SystemTime, no OS entropy …);
///       · ios / android — a real native process inside an APP SANDBOX: fs, clock and
///         entropy yes; no fork/exec, no TTY, no mutable env / desktop OS identity,
///         no server sockets (bind/listen is denied, CI has no network); Android's
///         sandbox additionally denies POSIX hard links;
///       · desktop — all of them.
///     These back `// requires-caps:` / `[Skip(feature:)]` in the test corpus: the
///     on-device bundle runner skips a case whose declared caps are not all here.
///     The vocabulary (and which caps are live) is docs/internals/src/devinfra/test-layout.md §4;
///     `xtask check layout` reconciles that list with the pushes below.
/// Order is stable (declaration order) so probe output is deterministic.
pub fn platform_caps(ctx: &VmContext) -> Value {
    let mut caps: Vec<&str> = Vec::new();
    #[cfg(feature = "jit")]                 caps.push("jit");
    #[cfg(feature = "native-interop")]      caps.push("native-interop");
    #[cfg(feature = "bundled-compression")] caps.push("bundled-compression");
    let os = crate::pal::system::OS_CAPS;
    if os.threads { caps.push("threads"); }
    // fix-wasm-corpus-capability-gate: `socket` = real OS networking (TCP/UDP/HTTP/
    // WS) — the `net` feature (off in the wasm preset; without it the builtins throw).
    #[cfg(feature = "net")]                 caps.push("socket");
    if os.fs { caps.push("fs"); }
    if os.clock { caps.push("clock"); }
    if os.entropy { caps.push("entropy"); }
    if os.process { caps.push("process"); }
    if os.tty { caps.push("tty"); }
    if os.env { caps.push("env"); }
    if os.socket_listen { caps.push("socket-listen"); }
    if os.hardlink { caps.push("hardlink"); }
    let list: Vec<Value> = caps.into_iter().map(|s| Value::Str(s.to_string().into())).collect();
    ctx.heap().alloc_array(list)
}

/// `__platform_exec_modes() -> string[]` — execution backends this binary can
/// dispatch. `interp` is always present; `jit` / `aot` are cfg-gated. Mirrors
/// the `exec modes:` line of `main.rs::print_build_info`.
///
/// NOTE: an `aot`-feature build lists `"aot"` here even though `aot.rs` is a
/// stub — this reports "compiled in", not "executable". The exec-profile support
/// matrix's *policy overlay* is what marks any `aot_pkgs ≠ []` composition as
/// `skipped-not-yet` (M9), independent of this list.
pub fn platform_exec_modes(ctx: &VmContext) -> Value {
    let mut modes: Vec<&str> = vec!["interp"];
    #[cfg(feature = "jit")] modes.push("jit");
    #[cfg(feature = "aot")] modes.push("aot");
    let list: Vec<Value> = modes.into_iter().map(|s| Value::Str(s.to_string().into())).collect();
    ctx.heap().alloc_array(list)
}

#[cfg(test)]
#[path = "platform_tests.rs"]
mod platform_tests;

/// 平台身份迁 builtin ABI v2(P1-8 批次 12):编译期常量全 PURE;caps /
/// exec_modes 返回数组,实现内直构。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__platform_os", pure)]
    pub fn os() -> &'static str { platform_os() }

    #[builtin(name = "__platform_arch", pure)]
    pub fn arch() -> &'static str { platform_arch() }

    #[builtin(name = "__platform_family", pure)]
    pub fn family() -> &'static str { platform_family() }

    #[builtin(name = "__platform_os_kind", pure)]
    pub fn os_kind() -> i64 { platform_os_kind() }

    #[builtin(name = "__platform_arch_kind", pure)]
    pub fn arch_kind() -> i64 { platform_arch_kind() }

    #[builtin(name = "__platform_caps")]
    pub fn caps(ctx: &VmContext) -> Value { platform_caps(ctx) }

    #[builtin(name = "__platform_exec_modes")]
    pub fn exec_modes(ctx: &VmContext) -> Value { platform_exec_modes(ctx) }
}
