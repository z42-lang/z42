//! 旋钮登记表：`KnobSpec` + `KNOWN_KNOBS`（每个 `Z42_*` 运行时旋钮的名字 / 默认值 / 类型 / 说明）。
//! refactor-split-config（2026-09-03）：自 config.rs 逐行搬出，对外路径经 hub 的 `pub use` 不变。

#![allow(unused_imports)]
use super::*;
use crate::gc::GcMode;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Metadata for a single `Z42_*` knob. Used by `--info` + future docgen
/// to enumerate every runtime knob in one place. Keep `KNOWN_KNOBS`
/// alphabetically sorted by `name` for stable `--info` output.
#[derive(Debug, Clone, Copy)]
pub struct KnobSpec {
    /// Env var name (e.g. `"Z42_LIBS"`).
    pub name: &'static str,
    /// Key under a `[runtime]` TOML table this knob maps to — kebab-case,
    /// `Z42_` prefix dropped and lowercased (`Z42_GC_MODE` → `"gc-mode"`).
    /// Empty string means "meta pointer, not a `[runtime]` value key" (e.g.
    /// `Z42_CONFIG` names the file itself, so it can't live inside it).
    pub toml_key: &'static str,
    /// One-line human description shown by `--info` / docgen.
    pub description: &'static str,
    /// Hint string for the default when unset (e.g. `"unset; falls back to ..."`).
    pub default_hint: &'static str,
    /// Where this knob is actually consumed — file path under `src/runtime/src/`.
    pub consumed_by: &'static str,
}

/// Authoritative list of every `Z42_*` env var the runtime reads.
///
/// Adding a new knob: append to this table + implement reader at
/// `consumed_by` location. CI check ([future]: stdlib API surface lint)
/// can grep `Z42_` across `src/runtime/src/` and diff against this table
/// to catch stragglers.
pub const KNOWN_KNOBS: &[KnobSpec] = &[
    KnobSpec {
        name: "Z42_CONFIG",
        toml_key: "",
        description: "path to a TOML file whose `[runtime]` table supplies knob defaults (env still wins)",
        default_hint: "unset; no config file layer (env / built-in defaults only)",
        consumed_by: "config.rs (load_runtime_toml) + main.rs",
    },
    KnobSpec {
        name: "Z42_CRASH_DIR",
        toml_key: "crash-dir",
        description: "directory for panic + signal crash report files",
        default_hint: "unset; reports go to stderr only",
        consumed_by: "main.rs (panic hook) + signal_handler.rs",
    },
    KnobSpec {
        name: "Z42_GC_MAX_BYTES",
        toml_key: "gc-max-bytes",
        description: "soft heap budget that ARMS automatic collection; the gc-*-ratio knobs are fractions of it and are inert without it. Accepts a byte count or a K/KB/M/MB/G/GB suffix (512MB, 2G)",
        default_hint: "unset; NO automatic collection happens at all (collect only on an explicit Std.GC.Collect())",
        consumed_by: "vm_context/construct.rs (set_max_heap_bytes) → gc/arc_heap/alloc.rs",
    },
    KnobSpec {
        name: "Z42_GC_MINOR_THRESHOLD",
        toml_key: "gc-minor-threshold",
        description: "fraction (0.0–1.0) of young entries surviving minor GC above which the next collect escalates to major immediately",
        default_hint: "unset; defaults to 0.75 (survival ratio)",
        consumed_by: "gc/arc_heap.rs",
    },
    KnobSpec {
        name: "Z42_GC_MODE",
        toml_key: "gc-mode",
        description: "GC algorithm: `stw` / `concurrent` / `generational` (with `-mark-sweep` aliases)",
        default_hint: "unset; defaults to `stw-mark-sweep`",
        consumed_by: "gc/mode.rs",
    },
    KnobSpec {
        name: "Z42_GC_NEAR_LIMIT_RATIO",
        toml_key: "gc-near-limit-ratio",
        description: "heap-used fraction (0.0–1.0) of the max-bytes limit at/above which an auto-collect trips and a NearHeapLimit event fires",
        default_hint: "unset; defaults to 0.90",
        consumed_by: "gc/arc_heap/alloc.rs",
    },
    KnobSpec {
        name: "Z42_GC_PAUSE_WINDOW",
        toml_key: "gc-pause-window",
        description: "capacity (entries) of the per-heap rolling GC pause-time deque, clamped to [1, 65536]",
        default_hint: "unset; defaults to 1024",
        consumed_by: "gc/types.rs",
    },
    KnobSpec {
        name: "Z42_GC_PRESSURE_RATIO",
        toml_key: "gc-pressure-ratio",
        description: "heap-used fraction (0.0–1.0) in [pressure, near) that fires an AllocationPressure event; stays below the near-limit ratio",
        default_hint: "unset; defaults to 0.75",
        consumed_by: "gc/arc_heap/alloc.rs",
    },
    KnobSpec {
        name: "Z42_GC_SOFT_THRESHOLD",
        toml_key: "gc-soft-threshold",
        description: "heap pressure ratio (0.0–1.0) above which SoftHandle refs become GC-eligible",
        default_hint: "unset; defaults to 0.80",
        consumed_by: "gc/soft_registry.rs",
    },
    KnobSpec {
        name: "Z42_GC_THROTTLE_RATIO",
        toml_key: "gc-throttle-ratio",
        description: "min heap-used growth (fraction 0.0–1.0 of the max-bytes limit) since the last auto-collect before another auto-collect may trip — debounces back-to-back collects",
        default_hint: "unset; defaults to 0.10",
        consumed_by: "gc/arc_heap/alloc.rs",
    },
    KnobSpec {
        name: "Z42_GC_TRACE",
        toml_key: "gc-trace",
        description: "print one stderr line per collection (kind, heap used before/after, bytes reclaimed, pause ms) plus near-limit / over-budget edges",
        default_hint: "unset; off (0/false/off/no also off). No observer installed when off",
        consumed_by: "gc/trace.rs, installed by vm_context/construct.rs",
    },
    KnobSpec {
        name: "Z42_JIT_PROFILE",
        toml_key: "jit-profile",
        description: "enable JIT compilation profiling (any non-empty value turns it on)",
        default_hint: "unset; JIT profiling off",
        consumed_by: "jit/lazy.rs",
    },
    KnobSpec {
        name: "Z42_LIBS",
        toml_key: "libs",
        description: "stdlib zpkg search directory",
        default_hint: "unset; falls back to artifacts/build/libraries/dist/release relative to z42vm binary",
        consumed_by: "main.rs",
    },
    KnobSpec {
        name: "Z42_LOG",
        toml_key: "log",
        description: "tracing-subscriber EnvFilter directive (e.g. z42::jit=debug,z42=warn)",
        default_hint: "unset; defaults to z42=warn (or z42=info under --verbose)",
        consumed_by: "main.rs (init_tracing)",
    },
    KnobSpec {
        name: "Z42_MODE",
        toml_key: "mode",
        description: "default execution mode: `interp` / `jit` / `aot` (below `--mode` CLI, above the build default)",
        default_hint: "unset; build default (jit if compiled in, else interp)",
        consumed_by: "main.rs (effective_mode)",
    },
    KnobSpec {
        name: "Z42_NATIVE_PATH",
        toml_key: "native-path",
        description: "search path for native .dylib/.so/.dll modules (colon-separated)",
        default_hint: "unset; falls back to package-relative search",
        consumed_by: "native/ext.rs",
    },
    KnobSpec {
        name: "Z42_PATH",
        toml_key: "path",
        description: "module search paths (colon-separated)",
        default_hint: "unset; falls back to <cwd>, <cwd>/modules",
        consumed_by: "main.rs",
    },
    KnobSpec {
        name: "Z42_SAFEPOINT_THROTTLE",
        toml_key: "safepoint-throttle",
        description: "per-thread safepoint check throttle (skip N safepoints between heap polls)",
        default_hint: "unset; defaults to 1024",
        consumed_by: "gc/safepoint.rs",
    },
    KnobSpec {
        name: "Z42_SAMPLE_HZ",
        toml_key: "sample-hz",
        description: "safepoint sampling-profiler frequency (Hz); any value ≥1 turns z42-level CPU sampling on",
        default_hint: "unset; sampling off (zero-cost — no background thread, hot path unchanged)",
        consumed_by: "gc/sampler.rs (via vm_context.rs)",
    },
    KnobSpec {
        name: "Z42_SAMPLE_OUT",
        toml_key: "sample-out",
        description: "folded-stacks output path for the sampling flamegraph (inferno format)",
        default_hint: "unset; defaults to z42-samples.folded (only written when Z42_SAMPLE_HZ set)",
        consumed_by: "app.rs (flush) + gc/sampler.rs",
    },
    KnobSpec {
        name: "Z42_STRESS_ITERS",
        toml_key: "stress-iters",
        description: "iteration count for GC stress tests (test code only)",
        default_hint: "unset; defaults to 100",
        consumed_by: "gc/arc_heap_tests/stress.rs",
    },
    KnobSpec {
        name: "Z42_TARGET",
        toml_key: "target",
        description: "reserved: cross-compilation / execution target selector (not yet implemented)",
        default_hint: "unset; reserved",
        consumed_by: "reserved (not yet implemented)",
    },
    KnobSpec {
        name: "Z42_TRACE_OUT",
        toml_key: "trace-out",
        description: "chrome/perfetto sample-trace JSON output path; set → also record a per-sample timeline",
        default_hint: "unset; no trace written (folded flamegraph still produced)",
        consumed_by: "app.rs (flush) + gc/sampler.rs",
    },
];
