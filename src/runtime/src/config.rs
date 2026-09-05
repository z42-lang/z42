//! `RuntimeConfig` — centralized declaration of every `Z42_*` runtime knob
//! consumed by `main.rs` startup.
//!
//! Roslyn / CoreCLR parallel: `inc/clrconfigvalues.h` macro table that
//! registers every runtime knob with a default + type + description in
//! one place. docs/review.md Part 4 D1 — was P0 because adding a new
//! knob previously meant scattering `std::env::var("Z42_X")` calls
//! across `main.rs` / `signal_handler.rs` / `gc/` / `native/`, with no
//! single place to discover what knobs exist.
//!
//! # Scope (this Phase 1 refactor)
//!
//! Centralizes the **5 startup-consumed** env vars:
//! `Z42_LIBS` / `Z42_PATH` / `Z42_LOG` / `Z42_CRASH_DIR` / `Z42_TARGET`
//! (reserved). These are the knobs `--info` reports and that `main.rs`
//! reads at boot.
//!
//! Subsystem-local OnceLock-cached env reads (`Z42_NATIVE_PATH` in
//! `native/ext.rs`, `Z42_SAFEPOINT_THROTTLE` / `Z42_GC_*` in `gc/`,
//! `Z42_STRESS_ITERS` in tests) keep their existing inline reads —
//! migrating those is Phase 2 (separate small refactor). The `KNOWN_KNOBS`
//! table here lists ALL of them for discovery / `--info` purposes
//! even though the actual values are read elsewhere.
//!
//! # Why a `&'static [KnobSpec]` table not just struct fields
//!
//! Each knob has metadata (`name` / `description` / `default_hint`) that
//! `--info` needs to render. Putting them in a const table makes
//! enumeration trivial. The struct also stores the *resolved values*
//! for the 5 startup knobs; subsystem-local knobs are listed but not
//! pre-resolved (they need to stay lazy for OnceLock cache semantics).

use crate::gc::GcMode;
use std::path::PathBuf;
use std::sync::OnceLock;

// refactor-split-config（2026-09-03）：旋钮表 → `config/knobs.rs`，解析函数 → `config/parse.rs`，
// 单测 → `config_tests.rs`；本文件留 RuntimeConfig 本体 / 默认值 / from_env / toml 加载 / 全局单例。
// complete-runtime-settings P0（2026-09-05）：schema 类型层 → `config/knobs.rs`，
// 旋钮登记表 → `config/knob_table.rs`，可用性求值 → `config/availability.rs`。
mod availability;
mod knob_table;
mod knobs;
mod parse;
mod resolve;
pub use availability::*;
pub use knob_table::{knob_by_env_name, knob_by_key, KNOWN_KNOBS};
pub use knobs::*;
pub use resolve::*;
pub(crate) use parse::*;

/// Resolved values of **every `Z42_*` runtime knob the runtime consumes**.
///
/// Phase 1 (2026-05-25, refactor-runtime-config) introduced the 4 startup
/// knobs (`Z42_LIBS` / `Z42_PATH` / `Z42_LOG` / `Z42_CRASH_DIR`) — read
/// once at `main()` and threaded through setup.
///
/// Phase 2 (2026-06-03, runtime-config-phase2) folded in the 6 subsystem-
/// local knobs (`Z42_GC_MODE` / `Z42_GC_MINOR_THRESHOLD` /
/// `Z42_GC_PAUSE_WINDOW` / `Z42_GC_SOFT_THRESHOLD` / `Z42_SAFEPOINT_THROTTLE` /
/// `Z42_NATIVE_PATH`) that previously each kept their own
/// `OnceLock` cache + `eprintln` warning. They now share this struct +
/// the single global [`runtime_config()`] accessor; warnings collapse
/// into one place.
///
/// Test-only knobs (`Z42_STRESS_ITERS` / `Z42_STRESS_SEED`) intentionally
/// stay inline in their test files — they don't deserve a slot in the
/// production config.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    // ── Phase 1: startup knobs (main.rs paths / tracing init) ────────────
    /// `Z42_LIBS` — stdlib zpkg search dir override. `None` = use fallback.
    pub libs_dir: Option<PathBuf>,
    /// `Z42_PATH` — colon-separated module search paths.
    pub module_path: Vec<PathBuf>,
    /// `Z42_LOG` — tracing-subscriber filter directive. `None` = use default.
    pub log_filter: Option<String>,
    /// `Z42_CRASH_DIR` — panic / signal crash report directory. `None` = stderr only.
    pub crash_dir: Option<PathBuf>,

    // ── Phase 2: subsystem knobs (read via [`runtime_config()`]) ─────────
    /// `Z42_GC_MODE` — algorithm selector.
    pub gc_mode: GcMode,
    /// `Z42_GC_MINOR_THRESHOLD` (0.0–1.0) — fraction of young entries
    /// surviving minor GC above which the next collect escalates to
    /// major immediately. Falls back to 0.75 on missing / invalid.
    pub gc_minor_threshold: f32,
    /// `Z42_GC_PAUSE_WINDOW` — capacity of the per-heap rolling pause-
    /// time deque (entries × 8 bytes). Clamped to `[1, 65536]`.
    /// Falls back to 1024 on missing / invalid.
    pub gc_pause_window: usize,
    /// `Z42_GC_SOFT_THRESHOLD` (0.0–1.0) — heap pressure ratio above
    /// which SoftHandle refs become GC-eligible. Falls back to 0.80 on
    /// missing / invalid.
    pub gc_soft_threshold: f64,
    /// `Z42_GC_NEAR_LIMIT_RATIO` (0.0–1.0) — heap-used fraction of the
    /// max-bytes limit at/above which the allocator trips an auto-collect
    /// and fires `NearHeapLimit`. Falls back to 0.90; clamped to `[0,1]`.
    pub gc_near_limit_ratio: f64,
    /// `Z42_GC_PRESSURE_RATIO` (0.0–1.0) — heap-used fraction in
    /// `[pressure, near)` that fires `AllocationPressure`. Expected below
    /// `gc_near_limit_ratio`. Falls back to 0.75; clamped to `[0,1]`.
    pub gc_pressure_ratio: f64,
    /// `Z42_GC_THROTTLE_RATIO` (0.0–1.0) — minimum heap-used growth (as a
    /// fraction of the max-bytes limit) since the last auto-collect before
    /// another may trip; debounces back-to-back collects. Falls back to
    /// 0.10; clamped to `[0,1]`.
    pub gc_throttle_ratio: f64,
    /// `Z42_SAFEPOINT_THROTTLE` — per-thread fast-path counter; every
    /// Nth check runs the real Mutex-lock poll. `1` disables throttling.
    /// Falls back to 1024 on missing / invalid.
    pub safepoint_throttle: u32,
    /// `Z42_NATIVE_PATH` — pre-split search paths for native modules.
    /// Empty list = no override (consumer applies SDK-relative fallback).
    pub native_search_paths: Vec<PathBuf>,
    /// `Z42_JIT_PROFILE` — enable JIT compilation profiling. A proper boolean
    /// (`true/false`, `1/0`, `yes/no`, `on/off`); anything else is rejected with
    /// a diagnostic and falls back to off. Read by `jit/lazy.rs`.
    ///
    /// complete-runtime-settings P1 fixed a doc/impl divergence here: the field
    /// doc always promised `false` = off, but the implementation was
    /// `.is_some()` on a non-empty string — so `Z42_JIT_PROFILE=false` turned
    /// profiling **on**. Now that the knob declares `ValueKind::Bool`, the value
    /// is actually parsed.
    pub jit_profile: bool,
    /// `Z42_MODE` — default execution mode (`interp` / `jit` / `aot`), raw
    /// (unvalidated) string. `None` = unset. Sits below `--mode` CLI and above
    /// the build default; `main.rs` validates the value + feature-gates jit/aot.
    pub mode: Option<String>,

    // ── script-profiling P2: safepoint sampling profiler ─────────────────
    /// `Z42_SAMPLE_HZ` — safepoint sampling frequency (Hz). `None` = sampling
    /// off (zero-cost). `Some(hz)` (hz ≥ 1) starts the background timer thread
    /// + z42-level CPU sampling. Read once at `VmCore::new_internal`.
    pub sample_hz: Option<u32>,
    /// `Z42_SAMPLE_OUT` — folded-stacks output path (inferno flamegraph input).
    /// Defaults to `z42-samples.folded`; only written when `sample_hz` is set.
    pub sample_out: PathBuf,
    /// `Z42_TRACE_OUT` — chrome/perfetto sample-trace JSON output path. `None`
    /// = no trace (folded still produced). `Some` → additionally record a
    /// per-sample `(ts, stack)` timeline and serialize it on exit.
    pub trace_out: Option<PathBuf>,

    // ── complete-runtime-settings P1: provenance ─────────────────────────
    /// 每个旋钮的生效值 + 来源层 + 被丢弃的值。由 [`RuntimeConfig::resolve_with`]
    /// 一次产出，`--info` / `--show-config` / `__cfg_*` builtin 纯读——优先级链
    /// 只有一份实现。用 `Default::default()` 直接构造时为空。
    pub resolved: Vec<ResolvedKnob>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            libs_dir: None,
            module_path: Vec::new(),
            log_filter: None,
            crash_dir: None,
            gc_mode: GcMode::default(),
            gc_minor_threshold: 0.75,
            gc_pause_window: 1024,
            gc_soft_threshold: 0.80,
            gc_near_limit_ratio: 0.90,
            gc_pressure_ratio: 0.75,
            gc_throttle_ratio: 0.10,
            safepoint_throttle: 1024,
            native_search_paths: Vec::new(),
            jit_profile: false,
            mode: None,
            sample_hz: None,
            sample_out: PathBuf::from("z42-samples.folded"),
            trace_out: None,
            resolved: Vec::new(),
        }
    }
}

impl RuntimeConfig {
    /// Build from the process environment (POSIX getenv / Windows GetEnvironmentVariable).
    /// Empty strings are treated as unset.
    pub fn from_env() -> Self {
        Self::from_getter(|name| std::env::var(name).ok())
    }

    /// Build using an injectable env getter. Test-friendly form —
    /// avoids `std::env::set_var` global races when running cargo test in
    /// parallel. The getter returns `Some(string)` if "set" (any value),
    /// `None` if "unset". Equivalent to `resolve(get, None)` (env-only, no
    /// config-file layer).
    pub fn from_getter<F>(get: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        Self::resolve(get, None)
    }

    /// Layered resolution — the precedence chain for every `Z42_*` knob:
    /// **CLI > env > user config file > app config sidecar > built-in default**.
    ///
    /// This two-argument form is the compatibility shape: env + one config table
    /// (the user layer), no CLI, no app sidecar. `resolve(get, None)` is exactly
    /// the env-only form and is byte-for-byte the old [`from_getter`] behaviour
    /// — the non-breaking guarantee.
    ///
    /// [`from_getter`]: RuntimeConfig::from_getter
    pub fn resolve<F>(get: F, runtime_table: Option<&toml::Table>) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let inputs = Inputs { user_config: runtime_table, ..Default::default() };
        Self::resolve_with(&get, &inputs, &BuildCtx::current()).0
    }

    /// Full resolution: all four input layers + provenance.
    ///
    /// Returns the typed config **and** the [`Resolution`] that produced it —
    /// the caller decides what to do with `resolution.diagnostics` (see
    /// [`Resolution::into_result`] for the CLI-fatal / else-warn split).
    pub fn resolve_with<F>(get: &F, inputs: &Inputs, ctx: &BuildCtx) -> (Self, Resolution)
    where
        F: Fn(&str) -> Option<String>,
    {
        let resolution = resolve_knobs(get, inputs, ctx);
        let cfg = Self::from_resolution(&resolution);
        (cfg, resolution)
    }

    /// Project the resolved raw strings onto the typed fields.
    ///
    /// Each `parse_*` keeps owning its own **range** policy (clamp vs fall back)
    /// — `resolve_knobs` only rejected values that could not be parsed as the
    /// declared type at all, so this stays behaviourally identical to the
    /// pre-P1 code for every value the parsers used to see.
    fn from_resolution(res: &Resolution) -> Self {
        let get = |name: &str| res.get(name).and_then(|k| k.raw.clone());
        Self {
            // ── Phase 1 startup knobs ─────────────────────────────────────
            libs_dir:    get("Z42_LIBS").map(PathBuf::from),
            module_path: get("Z42_PATH").map(|s| split_paths(&s)).unwrap_or_default(),
            log_filter:  get("Z42_LOG"),
            crash_dir:   get("Z42_CRASH_DIR").map(PathBuf::from),

            // ── Phase 2 subsystem knobs ──────────────────────────────────
            gc_mode:             parse_gc_mode(&get),
            gc_minor_threshold:  parse_gc_minor_threshold(&get),
            gc_pause_window:     parse_gc_pause_window(&get),
            gc_soft_threshold:   parse_gc_soft_threshold(&get),
            gc_near_limit_ratio: parse_gc_ratio(&get, "Z42_GC_NEAR_LIMIT_RATIO", 0.90),
            gc_pressure_ratio:   parse_gc_ratio(&get, "Z42_GC_PRESSURE_RATIO",   0.75),
            gc_throttle_ratio:   parse_gc_ratio(&get, "Z42_GC_THROTTLE_RATIO",   0.10),
            safepoint_throttle:  parse_safepoint_throttle(&get),
            native_search_paths: parse_native_search_paths(&get),
            // Real boolean parse — see the field doc for the divergence this fixed.
            jit_profile:         get("Z42_JIT_PROFILE").and_then(|s| parse_bool(&s)).unwrap_or(false),
            mode:                get("Z42_MODE"),
            sample_hz:           parse_sample_hz(&get),
            sample_out:          get("Z42_SAMPLE_OUT").map(PathBuf::from)
                                    .unwrap_or_else(|| PathBuf::from("z42-samples.folded")),
            trace_out:           get("Z42_TRACE_OUT").map(PathBuf::from),

            resolved: res.knobs.clone(),
        }
    }
}

/// Read the `[runtime]` table from the TOML file named by `Z42_CONFIG`, if any.
///
/// - `Z42_CONFIG` unset / empty → `Ok(None)` (no config-file layer; env + defaults).
/// - file missing → `Ok(None)` + a `warn` (not fatal — env / defaults still apply).
/// - malformed TOML, or `[runtime]` present but not a table → `Err(msg)`
///   (**explicit** — the caller surfaces it and exits; never silently defaults).
/// - present but no `[runtime]` table → `Ok(None)`.
pub fn load_runtime_toml<F>(get: F) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(path) = get("Z42_CONFIG").filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // eprintln (not tracing) — this runs before the subscriber is
            // installed, and matches the other one-line boot warnings here.
            eprintln!("z42: Z42_CONFIG={path:?} not found; ignoring config-file layer");
            return Ok(None);
        }
        Err(e) => return Err(format!("Z42_CONFIG={path:?}: {e}")),
    };
    let doc: toml::Table =
        toml::from_str(&text).map_err(|e| format!("Z42_CONFIG={path:?}: invalid TOML: {e}"))?;
    match doc.get("runtime") {
        Some(toml::Value::Table(t)) => Ok(Some(t.clone())),
        Some(_) => Err(format!("Z42_CONFIG={path:?}: [runtime] must be a table")),
        None => Ok(None),
    }
}

// ── Process-global accessor ──────────────────────────────────────────────────

/// Process-wide resolved config. Populated either by [`init_runtime_config`]
/// (main.rs, with the layered `[runtime]` config-file applied) or — if never
/// initialised (tests / embedders) — lazily from the env alone on first read.
static RUNTIME_CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();

/// Install the process-wide resolved config. Call **once, early in `main()`,
/// before any subsystem reads [`runtime_config()`]** (so the `[runtime]`
/// config-file layer is visible to GC / JIT / native). Returns `Err(cfg)` if a
/// read already raced the init in (leaving the env-only fallback installed) —
/// the caller may warn but must not retry.
pub fn init_runtime_config(cfg: RuntimeConfig) -> Result<(), RuntimeConfig> {
    RUNTIME_CONFIG.set(cfg)
}

/// Read the process-wide [`RuntimeConfig`]. Returns the value installed by
/// [`init_runtime_config`]; if none was installed, initialises lazily from the
/// env alone (the pre-existing behaviour — no config-file layer). Use this from
/// any subsystem that needs a `Z42_*` knob without threading it through.
#[inline]
pub fn runtime_config() -> &'static RuntimeConfig {
    RUNTIME_CONFIG.get_or_init(RuntimeConfig::from_env)
}

fn split_paths(s: &str) -> Vec<PathBuf> {
    let sep = if cfg!(windows) { ';' } else { ':' };
    s.split(sep)
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
