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
mod cli;
mod knob_table;
mod knobs;
mod parse;
mod render;
mod source;
mod resolve;
pub use availability::*;
pub use cli::*;
pub use knob_table::{knob_by_env_name, knob_by_key, KNOWN_KNOBS};
pub use knobs::*;
pub use render::*;
pub use resolve::*;
pub use source::*;
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
    /// `Z42_GC_MAX_BYTES` — soft heap budget that **arms auto-collect**
    /// (add-gc-runtime-knobs, 2026-09-05). `None` (the historical default) means
    /// *no automatic collection ever happens*: `maybe_auto_collect` bails out
    /// while this is unset, so a long-running program grows until it exits.
    /// The three ratios below are fractions of this budget and are inert
    /// without it. Accepts `512MB` / `2G` / a plain byte count.
    pub gc_max_bytes: Option<u64>,
    /// `Z42_GC_TRACE` — per-collection stderr trace (add-gc-runtime-knobs,
    /// 2026-09-05): one line per cycle with kind, heap used before/after,
    /// bytes reclaimed and pause µs. Any non-empty value except `0`/`false`
    /// turns it on. Off = zero cost (no observer installed).
    pub gc_trace: bool,
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

    // ── adopt-inline-env-knobs (2026-09-05): 收编的 8 个旋钮 ─────────────
    // 此前它们各自在 `consumed_by` 处 `std::env::var`，于是只有 env 层能设。
    // 收编后四层全收，且比原来**更快**——`env::var` 每次要加锁 + 查环境块 +
    // 分配 String，这里是 OnceLock 的一次 acquire load + 字段读。
    /// `Z42_JIT_THRESHOLD` — 函数被排入 JIT 编译的调用次数。clamp ≥ 1。
    pub jit_threshold: u32,
    /// `Z42_OSR_THRESHOLD` — 触发 OSR（把运行中的 interp 帧换成 JIT 帧）的回边次数。clamp ≥ 1。
    pub osr_threshold: u32,
    /// `Z42_JIT_DEBUG_PROMOTE` — 打印每次 interp→JIT 晋升决策。
    pub jit_debug_promote: bool,
    /// `Z42_NO_FUSION` — 关掉解释器超级指令融合（kill-switch / A-B 用）。
    pub no_fusion: bool,
    /// `Z42_NO_TYPED_FUSION` — 只关掉融合里的类型特化那一半。
    ///
    /// 字段名跟着**旋钮名**（反向），不存正向的 `typed_fusion_enabled`：表里的名字
    /// 是唯一 SoT，`--show-config` 打印 `no-typed-fusion`，结构体叫别的会让读代码的
    /// 人每次都要在脑子里反转一遍。反转只发生在真正需要它的那一行。
    pub no_typed_fusion: bool,
    /// `Z42_FUSION_DEBUG` — 把融合识别结果打到 stderr。
    pub fusion_debug: bool,
    /// `Z42_STACKALLOC` — 逃逸分析栈上分配的开关 / 统计。原样存字符串，五路 match
    /// 留在消费点（`interp/stack_alloc.rs`），那里还有一层 `AtomicU32` 缓存。
    pub stackalloc: Option<String>,
    /// `Z42_REPL_NATIVE` — REPL 行编辑 cdylib 的路径覆盖（文件或其所在目录）。
    pub repl_native: Option<PathBuf>,

    // ── add-app-properties (2026-09-05) ──────────────────────────────────
    /// 应用自定义配置属性——**app 侧车的 `[properties]` 段原样搬来**。
    ///
    /// 它**不是旋钮**：不在 `KNOWN_KNOBS` 里、不进 `resolved`、不参与五层分层、
    /// 不校验、未知键不是错误。VM 不理解它的含义，只把它交给
    /// `Std.Runtime.AppProperties` 供 app 自己读（对照 .NET 的
    /// `runtimeOptions.configProperties` + `AppContext.GetData`）。
    ///
    /// 只从 **app-config 层**（`Z42_APP_CONFIG` 或由 app 文件推导的侧车）来——
    /// 按设计它是**项目**配置，不是运行时旋钮，所以没有 CLI / env / 用户配置的覆盖。
    pub app_properties: Option<toml::Table>,

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
            gc_max_bytes: None,
            gc_trace: false,
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
            jit_threshold: 2,
            osr_threshold: 10_000,
            jit_debug_promote: false,
            no_fusion: false,
            no_typed_fusion: false,
            fusion_debug: false,
            stackalloc: None,
            repl_native: None,
            app_properties: None,
            resolved: Vec::new(),
        }
    }
}

impl RuntimeConfig {
    /// Build from the process environment: env vars **plus** the two config-file
    /// layers the environment names (`Z42_CONFIG` → user, `Z42_APP_CONFIG` → app
    /// sidecar). Empty strings are treated as unset.
    ///
    /// This is the path every **non-`z42vm`** entry point takes — `runtime_config()`
    /// lazily initialises through it, so `z42_host_run_app` (desktop self-contained
    /// apphost / wasm / iOS / Android / testhost) lands here. `z42vm`'s `main()`
    /// does not: it assembles the CLI layer itself and installs the result via
    /// [`init_runtime_config`] before anything reads it.
    ///
    /// sidecar-reaches-published-apps (2026-09-05) added the file layers here.
    /// They used to be loaded **only** in `main()`, so every embedder silently
    /// ignored `Z42_CONFIG` and `Z42_APP_CONFIG` — the documented precedence
    /// chain simply did not hold outside the `z42vm` binary.
    ///
    /// **Never exits.** A malformed config file downgrades to one stderr line and
    /// "that layer does not exist": this is a library path that may be running
    /// inside a host process (an iOS app, an Android JNI thread, wasm), where
    /// killing the process over a config typo is not ours to do. `z42vm`'s
    /// `main()` keeps the stricter treatment (fatal + `--strict-config`).
    pub fn from_env() -> Self {
        let get = |name: &str| std::env::var(name).ok();
        let user = load_layer_lenient(&get, "Z42_CONFIG");
        let app = load_layer_lenient(&get, "Z42_APP_CONFIG");
        let inputs = Inputs {
            user_config: user.as_ref(),
            app_config: app.as_ref(),
            ..Default::default()
        };
        let (cfg, resolution) = Self::resolve_with(&get, &inputs, &BuildCtx::current());
        // Warn-only: no `--strict-config` on this path (see the doc above).
        let _ = resolution.into_result(false);
        cfg
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
            gc_max_bytes:        parse_gc_max_bytes(&get),
            gc_near_limit_ratio: parse_gc_ratio(&get, "Z42_GC_NEAR_LIMIT_RATIO", 0.90),
            gc_pressure_ratio:   parse_gc_ratio(&get, "Z42_GC_PRESSURE_RATIO",   0.75),
            gc_throttle_ratio:   parse_gc_ratio(&get, "Z42_GC_THROTTLE_RATIO",   0.10),
            safepoint_throttle:  parse_safepoint_throttle(&get),
            native_search_paths: parse_native_search_paths(&get),
            // 两个 bool 旋钮共用 add-gc-runtime-knobs 引入的 `parse_bool_knob`。
            // 它对"非 0/false/off/no 即真"是宽松的，但两者都声明了
            // `ValueKind::Bool`，非布尔字符串在 `resolve_knobs` 就被判 Invalid +
            // 诊断、根本到不了这里——宽松与严格在这条链上不冲突。
            gc_trace:            parse_bool_knob(&get, "Z42_GC_TRACE"),
            jit_profile:         parse_bool_knob(&get, "Z42_JIT_PROFILE"),
            mode:                get("Z42_MODE"),
            sample_hz:           parse_sample_hz(&get),
            sample_out:          get("Z42_SAMPLE_OUT").map(PathBuf::from)
                                    .unwrap_or_else(|| PathBuf::from("z42-samples.folded")),
            trace_out:           get("Z42_TRACE_OUT").map(PathBuf::from),

            // adopt-inline-env-knobs：语义与各自原先的内联读取逐字相同
            // （两个 threshold 仍 clamp ≥1；四个开关现在是真布尔，见 knob_table）。
            jit_threshold:       parse_u32_knob(&get, "Z42_JIT_THRESHOLD", 2),
            osr_threshold:       parse_u32_knob(&get, "Z42_OSR_THRESHOLD", 10_000),
            jit_debug_promote:   parse_bool_knob(&get, "Z42_JIT_DEBUG_PROMOTE"),
            no_fusion:           parse_bool_knob(&get, "Z42_NO_FUSION"),
            no_typed_fusion:     parse_bool_knob(&get, "Z42_NO_TYPED_FUSION"),
            fusion_debug:        parse_bool_knob(&get, "Z42_FUSION_DEBUG"),
            stackalloc:          get("Z42_STACKALLOC"),
            repl_native:         get("Z42_REPL_NATIVE").map(PathBuf::from),

            // 属性不参与解析链——由调用方在装配后单独装入（见 `with_app_properties`）。
            app_properties: None,

            resolved: res.knobs.clone(),
        }
    }

    /// 装入 app 侧车的 `[properties]` 段。与旋钮分开是因为它不参与分层解析：
    /// 没有优先级、没有校验、没有 provenance——就是一张原样搬运的表。
    pub fn with_app_properties(mut self, props: Option<toml::Table>) -> Self {
        self.app_properties = props;
        self
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
