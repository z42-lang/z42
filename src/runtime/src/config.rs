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
        name: "Z42_GC_PAUSE_WINDOW",
        toml_key: "gc-pause-window",
        description: "capacity (entries) of the per-heap rolling GC pause-time deque, clamped to [1, 65536]",
        default_hint: "unset; defaults to 1024",
        consumed_by: "gc/types.rs",
    },
    KnobSpec {
        name: "Z42_GC_SOFT_THRESHOLD",
        toml_key: "gc-soft-threshold",
        description: "heap pressure ratio (0.0–1.0) above which SoftHandle refs become GC-eligible",
        default_hint: "unset; defaults to 0.80",
        consumed_by: "gc/soft_registry.rs",
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
];

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
    /// `Z42_SAFEPOINT_THROTTLE` — per-thread fast-path counter; every
    /// Nth check runs the real Mutex-lock poll. `1` disables throttling.
    /// Falls back to 1024 on missing / invalid.
    pub safepoint_throttle: u32,
    /// `Z42_NATIVE_PATH` — pre-split search paths for native modules.
    /// Empty list = no override (consumer applies SDK-relative fallback).
    pub native_search_paths: Vec<PathBuf>,
    /// `Z42_JIT_PROFILE` — enable JIT compilation profiling. Any non-empty
    /// value turns it on; `false` = off. Read by `jit/lazy.rs`.
    pub jit_profile: bool,
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
            safepoint_throttle: 1024,
            native_search_paths: Vec::new(),
            jit_profile: false,
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

    /// Layered resolution — the single precedence chain for `Z42_*` knobs:
    /// **explicit env value (`get`) > `[runtime]` TOML value > built-in
    /// default**. Each knob is matched to its TOML key via [`KNOWN_KNOBS`]
    /// (`toml_key`). `resolve(get, None)` is exactly the env-only form and is
    /// byte-for-byte the pre-existing [`from_getter`] behaviour — the
    /// non-breaking guarantee: with no config-file layer, nothing changes.
    ///
    /// CLI flags (e.g. `--mode`) sit *above* this chain and are applied by
    /// `main.rs`, not here. Project `[profile.*]` sits *below* env and above
    /// defaults; wiring that in is a later phase and does not touch this fn.
    pub fn resolve<F>(get: F, runtime_table: Option<&toml::Table>) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        // A getter that layers the `[runtime]` table under the real env:
        // a non-empty env value wins; otherwise the knob's `toml_key` is
        // looked up in the table; otherwise `None` → each parser's default.
        let layered = |name: &str| -> Option<String> {
            if let Some(v) = get(name) {
                if !v.trim().is_empty() {
                    return Some(v);
                }
            }
            let table = runtime_table?;
            let key = toml_key_for(name)?;
            if key.is_empty() {
                return None; // meta pointer (e.g. Z42_CONFIG) — never a table value
            }
            table.get(key).and_then(toml_scalar_to_string)
        };

        Self {
            // ── Phase 1 startup knobs ─────────────────────────────────────
            libs_dir:    layered("Z42_LIBS")    .filter(|s| !s.trim().is_empty()).map(PathBuf::from),
            module_path: layered("Z42_PATH")    .filter(|s| !s.trim().is_empty()).map(|s| split_paths(&s)).unwrap_or_default(),
            log_filter:  layered("Z42_LOG")     .filter(|s| !s.trim().is_empty()),
            crash_dir:   layered("Z42_CRASH_DIR").filter(|s| !s.trim().is_empty()).map(PathBuf::from),

            // ── Phase 2 subsystem knobs ──────────────────────────────────
            // Each parser absorbs its own validation: missing / empty
            // / invalid → default with an `eprintln!` warning so misconfigured
            // production runs surface the problem in one stderr line at
            // process start rather than silent-degrading per-subsystem.
            gc_mode:             parse_gc_mode(&layered),
            gc_minor_threshold:  parse_gc_minor_threshold(&layered),
            gc_pause_window:     parse_gc_pause_window(&layered),
            gc_soft_threshold:   parse_gc_soft_threshold(&layered),
            safepoint_throttle:  parse_safepoint_throttle(&layered),
            native_search_paths: parse_native_search_paths(&layered),
            jit_profile:         layered("Z42_JIT_PROFILE").filter(|s| !s.trim().is_empty()).is_some(),
        }
    }
}

/// Look up a knob's `[runtime]` TOML key from its env name via [`KNOWN_KNOBS`].
/// Returns `None` for names not in the table (defensive; all real knobs are).
fn toml_key_for(env_name: &str) -> Option<&'static str> {
    KNOWN_KNOBS.iter().find(|k| k.name == env_name).map(|k| k.toml_key)
}

/// Render a scalar TOML value the way the env-string parsers expect. Non-scalar
/// values (array / table / datetime) are not valid knob values → `None`.
fn toml_scalar_to_string(v: &toml::Value) -> Option<String> {
    match v {
        toml::Value::String(s)  => Some(s.clone()),
        toml::Value::Integer(i) => Some(i.to_string()),
        toml::Value::Float(f)   => Some(f.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

// ── Phase 2 parsers (one per subsystem knob) ─────────────────────────────────
//
// Centralised so `from_getter` reads as a flat list of field assignments;
// each parser owns its own default + invalid-value `eprintln`.

fn parse_gc_mode<F>(get: &F) -> GcMode
where F: Fn(&str) -> Option<String> {
    let Some(s) = get("Z42_GC_MODE").filter(|s| !s.trim().is_empty()) else {
        return GcMode::default();
    };
    match s.as_str() {
        "concurrent" | "concurrent-mark-sweep"     => GcMode::ConcurrentMarkSweep,
        "generational" | "generational-mark-sweep" => GcMode::GenerationalMarkSweep,
        "stw" | "stw-mark-sweep"                   => GcMode::StwMarkSweep,
        other => {
            eprintln!("z42: Z42_GC_MODE={other:?} not recognized; falling back to stw-mark-sweep");
            GcMode::StwMarkSweep
        }
    }
}

fn parse_gc_minor_threshold<F>(get: &F) -> f32
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_GC_MINOR_THRESHOLD").filter(|s| !s.trim().is_empty()) else {
        return 0.75;
    };
    match raw.parse::<f32>() {
        Ok(v) if v > 0.0 && v <= 1.0 => v,
        _ => {
            eprintln!("z42: invalid Z42_GC_MINOR_THRESHOLD={raw:?}; using default 0.75");
            0.75
        }
    }
}

/// Hard ceiling — 65536 × 8 bytes per slot = 512 KB per heap pause-window
/// deque. Generous but prevents a hostile env from allocating GB.
const GC_PAUSE_WINDOW_MAX: usize = 65536;
const GC_PAUSE_WINDOW_DEFAULT: usize = 1024;

fn parse_gc_pause_window<F>(get: &F) -> usize
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_GC_PAUSE_WINDOW").filter(|s| !s.trim().is_empty()) else {
        return GC_PAUSE_WINDOW_DEFAULT;
    };
    match raw.parse::<i64>() {
        Ok(n) if n >= 1 => (n as usize).min(GC_PAUSE_WINDOW_MAX),
        _               => GC_PAUSE_WINDOW_DEFAULT,
    }
}

fn parse_gc_soft_threshold<F>(get: &F) -> f64
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_GC_SOFT_THRESHOLD").filter(|s| !s.trim().is_empty()) else {
        return 0.80;
    };
    raw.parse::<f64>()
        .map(|v| v.clamp(0.0, 1.0))
        .unwrap_or(0.80)
}

fn parse_safepoint_throttle<F>(get: &F) -> u32
where F: Fn(&str) -> Option<String> {
    let Some(raw) = get("Z42_SAFEPOINT_THROTTLE").filter(|s| !s.trim().is_empty()) else {
        return 1024;
    };
    match raw.parse::<u32>() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("z42: invalid Z42_SAFEPOINT_THROTTLE={raw:?}; using default 1024");
            1024
        }
    }
}

fn parse_native_search_paths<F>(get: &F) -> Vec<PathBuf>
where F: Fn(&str) -> Option<String> {
    get("Z42_NATIVE_PATH")
        .filter(|s| !s.trim().is_empty())
        .map(|s| split_paths(&s))
        .unwrap_or_default()
}

// ── Config-file (`[runtime]`) loading ────────────────────────────────────────

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
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Build a fake env getter from a static map — avoids global env-var
    /// race when cargo runs tests in parallel. Returns owned strings so
    /// the closure can outlive the input slice.
    fn fake_env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    #[test]
    fn known_knobs_alphabetical_and_unique() {
        let names: Vec<&str> = KNOWN_KNOBS.iter().map(|k| k.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "KNOWN_KNOBS must be alphabetically sorted by name");

        let mut uniq = sorted.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), sorted.len(), "KNOWN_KNOBS contains duplicate names");
    }

    #[test]
    fn known_knobs_match_struct_fields_for_startup_knobs() {
        // The 4 path-ish fields on RuntimeConfig must each appear in KNOWN_KNOBS.
        let names: Vec<&str> = KNOWN_KNOBS.iter().map(|k| k.name).collect();
        for required in ["Z42_LIBS", "Z42_PATH", "Z42_LOG", "Z42_CRASH_DIR"] {
            assert!(names.contains(&required),
                "RuntimeConfig field expects {required} in KNOWN_KNOBS");
        }
    }

    #[test]
    fn from_getter_all_unset() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[]));
        assert!(cfg.libs_dir.is_none());
        assert!(cfg.log_filter.is_none());
        assert!(cfg.crash_dir.is_none());
        assert!(cfg.module_path.is_empty());
    }

    #[test]
    fn from_getter_empty_string_is_unset() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LIBS", "")]));
        assert!(cfg.libs_dir.is_none(), "empty Z42_LIBS should be treated as unset");
    }

    #[test]
    fn from_getter_whitespace_only_is_unset() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LOG", "   ")]));
        assert!(cfg.log_filter.is_none(), "whitespace-only Z42_LOG should be unset");
    }

    #[test]
    fn from_getter_libs_set() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LIBS", "/tmp/z42-libs")]));
        assert_eq!(cfg.libs_dir.as_deref(), Some(std::path::Path::new("/tmp/z42-libs")));
    }

    #[test]
    fn from_getter_path_splits_on_platform_separator() {
        let sep = if cfg!(windows) { ';' } else { ':' };
        let input = format!("/a{sep}/b{sep}/c");
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_PATH", &input)]));
        assert_eq!(
            cfg.module_path,
            vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn from_getter_path_skips_empty_segments() {
        let sep = if cfg!(windows) { ';' } else { ':' };
        let input = format!("/a{sep}{sep}/b{sep} {sep} /c");
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_PATH", &input)]));
        assert_eq!(
            cfg.module_path,
            vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn from_getter_log_filter_passes_through() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_LOG", "z42::jit=debug,z42=warn")]));
        assert_eq!(cfg.log_filter.as_deref(), Some("z42::jit=debug,z42=warn"));
    }

    #[test]
    fn from_getter_crash_dir_set() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_CRASH_DIR", "/var/log/z42")]));
        assert_eq!(cfg.crash_dir.as_deref(), Some(std::path::Path::new("/var/log/z42")));
    }

    #[test]
    fn from_getter_default_values_match_documented_defaults() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[]));
        assert_eq!(cfg.gc_mode,             GcMode::StwMarkSweep);
        assert_eq!(cfg.gc_minor_threshold,  0.75);
        assert_eq!(cfg.gc_pause_window,     1024);
        assert_eq!(cfg.gc_soft_threshold,   0.80);
        assert_eq!(cfg.safepoint_throttle,  1024);
        assert!(cfg.native_search_paths.is_empty());
    }

    // ── Phase 2 subsystem knob parsers ───────────────────────────────────────

    #[test]
    fn from_getter_gc_mode_recognised_aliases() {
        for (input, expected) in [
            ("concurrent",                 GcMode::ConcurrentMarkSweep),
            ("concurrent-mark-sweep",      GcMode::ConcurrentMarkSweep),
            ("generational",               GcMode::GenerationalMarkSweep),
            ("generational-mark-sweep",    GcMode::GenerationalMarkSweep),
            ("stw",                        GcMode::StwMarkSweep),
            ("stw-mark-sweep",             GcMode::StwMarkSweep),
        ] {
            let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MODE", input)]));
            assert_eq!(cfg.gc_mode, expected, "Z42_GC_MODE={input:?}");
        }
    }

    #[test]
    fn from_getter_gc_mode_unknown_falls_back_to_stw() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MODE", "bogus-algo")]));
        assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep);
    }

    #[test]
    fn from_getter_gc_minor_threshold_validates_range() {
        // Valid in (0.0, 1.0]
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MINOR_THRESHOLD", "0.5")]))
                .gc_minor_threshold,
            0.5,
        );
        // Out-of-range / invalid → default 0.75
        for bad in &["0", "-0.1", "1.5", "garbage"] {
            assert_eq!(
                RuntimeConfig::from_getter(fake_env(&[("Z42_GC_MINOR_THRESHOLD", bad)]))
                    .gc_minor_threshold,
                0.75,
                "bad input {bad:?} should default",
            );
        }
    }

    #[test]
    fn from_getter_gc_pause_window_clamps_and_validates() {
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_PAUSE_WINDOW", "2048")]))
                .gc_pause_window,
            2048,
        );
        // Clamp to MAX
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_PAUSE_WINDOW", "999999")]))
                .gc_pause_window,
            GC_PAUSE_WINDOW_MAX,
        );
        // 0 / negative / garbage → default
        for bad in &["0", "-1", "abc"] {
            assert_eq!(
                RuntimeConfig::from_getter(fake_env(&[("Z42_GC_PAUSE_WINDOW", bad)]))
                    .gc_pause_window,
                GC_PAUSE_WINDOW_DEFAULT,
            );
        }
    }

    #[test]
    fn from_getter_gc_soft_threshold_clamps_to_unit_range() {
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "0.42")]))
                .gc_soft_threshold,
            0.42,
        );
        // Clamp under / over
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "-1.0")]))
                .gc_soft_threshold,
            0.0,
        );
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "1.5")]))
                .gc_soft_threshold,
            1.0,
        );
        // Garbage → default 0.80
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_GC_SOFT_THRESHOLD", "xyz")]))
                .gc_soft_threshold,
            0.80,
        );
    }

    #[test]
    fn from_getter_safepoint_throttle_validates_positive_u32() {
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_SAFEPOINT_THROTTLE", "1")]))
                .safepoint_throttle,
            1,
        );
        assert_eq!(
            RuntimeConfig::from_getter(fake_env(&[("Z42_SAFEPOINT_THROTTLE", "4096")]))
                .safepoint_throttle,
            4096,
        );
        // 0 / negative / garbage → default 1024
        for bad in &["0", "-1", "abc"] {
            assert_eq!(
                RuntimeConfig::from_getter(fake_env(&[("Z42_SAFEPOINT_THROTTLE", bad)]))
                    .safepoint_throttle,
                1024,
                "bad input {bad:?}",
            );
        }
    }

    #[test]
    fn from_getter_native_path_splits_on_platform_separator() {
        let sep = if cfg!(windows) { ';' } else { ':' };
        let input = format!("/native/a{sep}/native/b");
        let cfg = RuntimeConfig::from_getter(fake_env(&[("Z42_NATIVE_PATH", &input)]));
        assert_eq!(
            cfg.native_search_paths,
            vec![PathBuf::from("/native/a"), PathBuf::from("/native/b")],
        );
    }

    #[test]
    fn from_getter_ignores_unrelated_env_vars() {
        let cfg = RuntimeConfig::from_getter(fake_env(&[
            ("RUST_BACKTRACE", "1"),  // unrelated env
        ]));
        assert!(cfg.libs_dir.is_none());
        assert!(cfg.log_filter.is_none());
        assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep);
    }

    // ── unify-run-modes P0: layered resolution + [runtime] config file ────────

    fn rt_table(src: &str) -> toml::Table {
        toml::from_str(src).expect("test TOML must parse")
    }

    /// Unique temp path per (pid, name) — no global env mutation, parallel-safe.
    fn write_temp(name: &str, content: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir()
            .join(format!("z42-p0cfg-{}-{name}.toml", std::process::id()));
        std::fs::write(&p, content).expect("write temp config");
        p
    }

    #[test]
    fn resolve_none_equals_from_getter_nonbreaking() {
        // The non-breaking guarantee: with no config-file layer, resolve is
        // byte-for-byte the old env-only behaviour.
        let env = &[("Z42_LIBS", "/l"), ("Z42_GC_MODE", "concurrent"), ("Z42_JIT_PROFILE", "1")];
        let a = RuntimeConfig::resolve(fake_env(env), None);
        let b = RuntimeConfig::from_getter(fake_env(env));
        assert_eq!(a.libs_dir, b.libs_dir);
        assert_eq!(a.gc_mode, b.gc_mode);
        assert_eq!(a.jit_profile, b.jit_profile);
        assert_eq!(a.gc_minor_threshold, b.gc_minor_threshold);
    }

    #[test]
    fn resolve_env_wins_over_table() {
        let t = rt_table("gc-mode = \"stw\"\ngc-minor-threshold = 0.5");
        let cfg = RuntimeConfig::resolve(fake_env(&[("Z42_GC_MODE", "concurrent")]), Some(&t));
        assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep, "env beats table");
        assert_eq!(cfg.gc_minor_threshold, 0.5, "table used where env absent");
    }

    #[test]
    fn resolve_table_wins_over_default() {
        let t = rt_table("gc-mode = \"generational\"\ngc-pause-window = 4096\njit-profile = true");
        let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&t));
        assert_eq!(cfg.gc_mode, GcMode::GenerationalMarkSweep);
        assert_eq!(cfg.gc_pause_window, 4096);
        assert!(cfg.jit_profile, "table boolean jit-profile=true → on");
    }

    #[test]
    fn resolve_all_unset_uses_defaults() {
        let t = rt_table("");
        let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&t));
        assert_eq!(cfg.gc_mode, GcMode::StwMarkSweep);
        assert_eq!(cfg.gc_minor_threshold, 0.75);
        assert!(!cfg.jit_profile);
    }

    #[test]
    fn resolve_empty_env_falls_to_table() {
        // Empty env value = unset (config-wide convention) → table consulted.
        let t = rt_table("gc-mode = \"generational\"");
        let cfg = RuntimeConfig::resolve(fake_env(&[("Z42_GC_MODE", "")]), Some(&t));
        assert_eq!(cfg.gc_mode, GcMode::GenerationalMarkSweep);
    }

    #[test]
    fn new_knobs_registered_with_toml_keys() {
        for name in ["Z42_JIT_PROFILE", "Z42_TARGET", "Z42_CONFIG"] {
            assert!(KNOWN_KNOBS.iter().any(|k| k.name == name), "{name} must be registered");
        }
        assert_eq!(toml_key_for("Z42_GC_MODE"), Some("gc-mode"));
        assert_eq!(toml_key_for("Z42_GC_MINOR_THRESHOLD"), Some("gc-minor-threshold"));
        // Z42_CONFIG is a meta pointer — no [runtime] value key.
        assert_eq!(toml_key_for("Z42_CONFIG"), Some(""));
    }

    #[test]
    fn gc_minor_threshold_hint_not_stale() {
        // Regression: the KNOWN_KNOBS entry used to say "64 KiB" / "bytes of
        // allocation", contradicting the actual survival-ratio 0.75 semantics.
        let k = KNOWN_KNOBS.iter().find(|k| k.name == "Z42_GC_MINOR_THRESHOLD").unwrap();
        assert!(!k.default_hint.contains("64 KiB"));
        assert!(!k.description.contains("bytes of allocation"));
        assert!(k.default_hint.contains("0.75"));
    }

    #[test]
    fn jit_profile_from_env() {
        assert!(RuntimeConfig::from_getter(fake_env(&[("Z42_JIT_PROFILE", "1")])).jit_profile);
        assert!(!RuntimeConfig::from_getter(fake_env(&[])).jit_profile);
        assert!(!RuntimeConfig::from_getter(fake_env(&[("Z42_JIT_PROFILE", "")])).jit_profile,
            "empty = unset (config convention)");
    }

    #[test]
    fn load_runtime_toml_unset_is_none() {
        let out = load_runtime_toml(fake_env(&[])).unwrap();
        assert!(out.is_none());
    }

    #[test]
    fn load_runtime_toml_missing_file_is_none_not_panic() {
        let out = load_runtime_toml(fake_env(&[("Z42_CONFIG", "/no/such/z42-cfg-xyz.toml")])).unwrap();
        assert!(out.is_none(), "missing file → None (warn), not error/panic");
    }

    #[test]
    fn load_runtime_toml_reads_runtime_section() {
        let p = write_temp("reads", "[runtime]\ngc-mode = \"concurrent\"\n[other]\nx = 1\n");
        let pstr = p.to_string_lossy().into_owned();
        let table = load_runtime_toml(fake_env(&[("Z42_CONFIG", &pstr)])).unwrap().unwrap();
        assert_eq!(table.get("gc-mode").and_then(|v| v.as_str()), Some("concurrent"));
        // end-to-end through resolve
        let cfg = RuntimeConfig::resolve(fake_env(&[]), Some(&table));
        assert_eq!(cfg.gc_mode, GcMode::ConcurrentMarkSweep);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn load_runtime_toml_no_runtime_section_is_none() {
        let p = write_temp("noruntime", "[other]\nx = 1\n");
        let pstr = p.to_string_lossy().into_owned();
        let out = load_runtime_toml(fake_env(&[("Z42_CONFIG", &pstr)])).unwrap();
        assert!(out.is_none(), "no [runtime] table → None");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn load_runtime_toml_malformed_errors_explicitly() {
        let p = write_temp("malformed", "this is = = not toml [[[");
        let pstr = p.to_string_lossy().into_owned();
        let out = load_runtime_toml(fake_env(&[("Z42_CONFIG", &pstr)]));
        assert!(out.is_err(), "malformed TOML → explicit error, not silent default");
        let _ = std::fs::remove_file(&p);
    }
}
