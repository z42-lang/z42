//! `KNOWN_KNOBS` —— 每个 `Z42_*` 运行时旋钮的**唯一登记处**。
//!
//! 新增一个旋钮 = 这里加一行 + 在 `consumed_by` 处读它。CLI（`--set`）、环境变量、
//! 两个配置文件层、`--info` / `--list-knobs` / `--show-config`、以及 z42 脚本的
//! `Std.Runtime.RuntimeConfig` 全部由本表驱动，没有第二处需要改。
//!
//! **不变式**（由 `config_tests.rs` 守）：按 `name` 字母序、无重复；`requires` 里
//! 的 feature 名都在 `availability::KNOWN_FEATURES` 内；元旋钮（`toml_key` 空）的
//! `tier` 必为 `Internal` 且 `sources ⊆ {Cli, Env}`；`aliases` 全局唯一、不与任何
//! `toml_key` 冲突。
//!
//! 每条只写**偏离基线的字段**（`..PUBLIC` / `..TUNING` / `..META`），读表时一眼看到
//! 的就是"这个旋钮哪里特殊"。基线定义见 [`super::knobs`]。
//!
//! complete-runtime-settings P0（2026-09-05）：自 knobs.rs 拆出并扩为完整 schema。

use super::knobs::*;

/// `Z42_GC_MODE` 的合法取值（含 `-mark-sweep` 全称别名）。与
/// `config/parse.rs::parse_gc_mode` 的 match 臂一一对应。
const GC_MODES: &[&str] = &[
    "stw", "stw-mark-sweep",
    "concurrent", "concurrent-mark-sweep",
    "generational", "generational-mark-sweep",
];

/// `Z42_MODE` 的合法取值。jit / aot 的**逐值** feature 门控由
/// `main.rs::resolve_config_mode` 负责（interp 在任何 build 都可用），
/// 故本旋钮的 `requires` 为空——见 design.md Decision 2 的例外说明。
const EXEC_MODES: &[&str] = &["interp", "jit", "aot"];

/// 采样 profiler / native 扩展不适用的平台。wasm 无后台线程、无 dlopen。
const NOT_WASM: PlatformAvail = PlatformAvail::Except(&["wasm"]);

pub const KNOWN_KNOBS: &[KnobSpec] = &[
    KnobSpec {
        name: "Z42_APP_CONFIG",
        value: ValueKind::Path,
        description: "path to the application's `<app>.runtimeconfig.toml` sidecar; its `[runtime]` table forms the lowest config layer (user `Z42_CONFIG` wins per key)",
        default_hint: "unset; no app-config layer (launcher sets it when a sidecar exists)",
        consumed_by: "config/source.rs + toolchain launcher",
        ..META
    },
    KnobSpec {
        name: "Z42_CONFIG",
        value: ValueKind::Path,
        description: "path to a TOML file whose `[runtime]` table supplies knob values (CLI and env still win)",
        default_hint: "unset; no user config-file layer",
        consumed_by: "config/source.rs + main.rs",
        ..META
    },
    KnobSpec {
        name: "Z42_CRASH_DIR",
        toml_key: "crash-dir",
        value: ValueKind::Path,
        description: "directory for panic + signal crash report files",
        default_hint: "unset; reports go to stderr only",
        consumed_by: "main.rs (panic hook) + signal_handler.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_GC_MINOR_THRESHOLD",
        toml_key: "gc-minor-threshold",
        value: ValueKind::Float { min: 0.0, max: 1.0 },
        description: "fraction (0.0–1.0) of young entries surviving minor GC above which the next collect escalates to major immediately",
        default_hint: "unset; defaults to 0.75 (survival ratio)",
        consumed_by: "gc/arc_heap.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_GC_MODE",
        toml_key: "gc-mode",
        value: ValueKind::Enum(GC_MODES),
        description: "GC algorithm: `stw` / `concurrent` / `generational` (with `-mark-sweep` aliases)",
        default_hint: "unset; defaults to `stw-mark-sweep`",
        consumed_by: "gc/mode.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_GC_NEAR_LIMIT_RATIO",
        toml_key: "gc-near-limit-ratio",
        value: ValueKind::Float { min: 0.0, max: 1.0 },
        description: "heap-used fraction (0.0–1.0) of the max-bytes limit at/above which an auto-collect trips and a NearHeapLimit event fires",
        default_hint: "unset; defaults to 0.90",
        consumed_by: "gc/arc_heap/alloc.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_GC_PAUSE_WINDOW",
        toml_key: "gc-pause-window",
        value: ValueKind::Int { min: 1, max: 65536 },
        description: "capacity (entries) of the per-heap rolling GC pause-time deque, clamped to [1, 65536]",
        default_hint: "unset; defaults to 1024",
        consumed_by: "gc/types.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_GC_PRESSURE_RATIO",
        toml_key: "gc-pressure-ratio",
        value: ValueKind::Float { min: 0.0, max: 1.0 },
        description: "heap-used fraction (0.0–1.0) in [pressure, near) that fires an AllocationPressure event; stays below the near-limit ratio",
        default_hint: "unset; defaults to 0.75",
        consumed_by: "gc/arc_heap/alloc.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_GC_SOFT_THRESHOLD",
        toml_key: "gc-soft-threshold",
        value: ValueKind::Float { min: 0.0, max: 1.0 },
        description: "heap pressure ratio (0.0–1.0) above which SoftHandle refs become GC-eligible",
        default_hint: "unset; defaults to 0.80",
        consumed_by: "gc/soft_registry.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_GC_THROTTLE_RATIO",
        toml_key: "gc-throttle-ratio",
        value: ValueKind::Float { min: 0.0, max: 1.0 },
        description: "min heap-used growth (fraction 0.0–1.0 of the max-bytes limit) since the last auto-collect before another auto-collect may trip — debounces back-to-back collects",
        default_hint: "unset; defaults to 0.10",
        consumed_by: "gc/arc_heap/alloc.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_JIT_PROFILE",
        toml_key: "jit-profile",
        value: ValueKind::Bool,
        requires: &["jit"],
        description: "enable JIT compilation profiling (any non-empty value turns it on)",
        default_hint: "unset; JIT profiling off",
        consumed_by: "jit/lazy.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_LIBS",
        toml_key: "libs",
        value: ValueKind::Path,
        description: "stdlib zpkg search directory",
        default_hint: "unset; falls back to artifacts/build/libraries/dist/release relative to z42vm binary",
        consumed_by: "main.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_LOG",
        toml_key: "log",
        value: ValueKind::Str,
        description: "tracing-subscriber EnvFilter directive (e.g. z42::jit=debug,z42=warn)",
        default_hint: "unset; defaults to z42=warn (or z42=info under --verbose)",
        consumed_by: "main.rs (init_tracing)",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_MODE",
        toml_key: "mode",
        value: ValueKind::Enum(EXEC_MODES),
        description: "default execution mode: `interp` / `jit` / `aot` (below `--mode` CLI, above the build default)",
        default_hint: "unset; build default (jit if compiled in, else interp)",
        consumed_by: "main.rs (effective_mode)",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_NATIVE_PATH",
        toml_key: "native-path",
        value: ValueKind::PathList,
        requires: &["native-interop"],
        platforms: NOT_WASM,
        description: "search path for native .dylib/.so/.dll modules (platform-separated)",
        default_hint: "unset; falls back to package-relative search",
        consumed_by: "native/ext.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_PATH",
        toml_key: "path",
        value: ValueKind::PathList,
        description: "module search paths (platform-separated)",
        default_hint: "unset; falls back to <cwd>, <cwd>/modules",
        consumed_by: "main.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_SAFEPOINT_THROTTLE",
        toml_key: "safepoint-throttle",
        value: ValueKind::Int { min: 1, max: u32::MAX as i64 },
        description: "per-thread safepoint check throttle (skip N safepoints between heap polls)",
        default_hint: "unset; defaults to 1024",
        consumed_by: "gc/safepoint.rs",
        ..TUNING
    },
    KnobSpec {
        name: "Z42_SAMPLE_HZ",
        toml_key: "sample-hz",
        value: ValueKind::Int { min: 1, max: u32::MAX as i64 },
        platforms: NOT_WASM,
        description: "safepoint sampling-profiler frequency (Hz); any value ≥1 turns z42-level CPU sampling on",
        default_hint: "unset; sampling off (zero-cost — no background thread, hot path unchanged)",
        consumed_by: "gc/sampler.rs (via vm_context.rs)",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_SAMPLE_OUT",
        toml_key: "sample-out",
        value: ValueKind::Path,
        platforms: NOT_WASM,
        description: "folded-stacks output path for the sampling flamegraph (inferno format)",
        default_hint: "unset; defaults to z42-samples.folded (only written when Z42_SAMPLE_HZ set)",
        consumed_by: "app.rs (flush) + gc/sampler.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_STRESS_ITERS",
        toml_key: "stress-iters",
        value: ValueKind::Int { min: 1, max: i64::MAX },
        sources: LayerMask::ENV_ONLY,
        build: BuildAvail::DebugOnly,
        tier: Tier::Internal,
        description: "iteration count for GC stress tests (test code only)",
        default_hint: "unset; defaults to 100",
        consumed_by: "gc/arc_heap_tests/stress.rs",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_STRICT_CONFIG",
        value: ValueKind::Bool,
        description: "escalate config warnings (unknown / unavailable / invalid knob values from env or config files) to fatal errors — CI drift gate",
        default_hint: "unset; non-CLI config problems warn and fall back to the default",
        consumed_by: "config/resolve.rs (diagnostic severity)",
        ..META
    },
    KnobSpec {
        name: "Z42_TARGET",
        toml_key: "target",
        value: ValueKind::Str,
        tier: Tier::Internal,
        description: "reserved: cross-compilation / execution target selector (not yet implemented)",
        default_hint: "unset; reserved",
        consumed_by: "reserved (not yet implemented)",
        ..PUBLIC
    },
    KnobSpec {
        name: "Z42_TRACE_OUT",
        toml_key: "trace-out",
        value: ValueKind::Path,
        platforms: NOT_WASM,
        description: "chrome/perfetto sample-trace JSON output path; set → also record a per-sample timeline",
        default_hint: "unset; no trace written (folded flamegraph still produced)",
        consumed_by: "app.rs (flush) + gc/sampler.rs",
        ..PUBLIC
    },
];

/// 按完整 key 或显式 alias 查旋钮（`--set` 与 `[runtime]` 表用）。
/// **不**接受 `Z42_*` env 名形式——那是 env 层的写法。
pub fn knob_by_key(key: &str) -> Option<&'static KnobSpec> {
    KNOWN_KNOBS.iter().find(|k| k.matches_key(key))
}

/// 按环境变量名查旋钮。
pub fn knob_by_env_name(name: &str) -> Option<&'static KnobSpec> {
    KNOWN_KNOBS.iter().find(|k| k.name == name)
}
