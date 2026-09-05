use anyhow::Result;
use clap::{Parser, ValueEnum};

// 启动期辅助（stdlib 定位 / tracing / panic hook / --info / 模块路径）——
// 见 startup.rs 顶部关于为什么拆开的说明。
mod startup;
use startup::*;

// z42c self-compile is malloc-bound (~31% in the system allocator, profiled).
// Route the z42vm process's global allocator through mimalloc — cuts z42c
// compile ~40% and alloc-heavy (string-building) workloads ~3×. Gated on the
// default-on `mimalloc-alloc` feature so wasm/mobile presets (built
// `--no-default-features`) fall back to the system allocator.
//
// `dhat-heap` (script-profiling P0, `xtask profile --heap`) replaces the global
// allocator with dhat's heap-profiling shim, so it is mutually exclusive with
// mimalloc — a build carries at most one `#[global_allocator]`. Only ever built
// on demand into a throwaway target-dir by `xtask profile`; never shipped.
#[cfg(all(feature = "mimalloc-alloc", not(feature = "dhat-heap")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Parser)]
#[command(name = "z42vm", about = "z42 Virtual Machine", version)]
struct Cli {
    /// Bytecode file to execute.
    /// Accepted formats: .zbc (single-file), .zpkg (project package).
    /// Optional only when `--info` is set.
    file: Option<String>,

    /// Optional entry-function override (positional). When omitted the VM
    /// reads the `Entry` baked into the zpkg by `z42c build` (which itself
    /// auto-detects `Main()` at compile time — see auto-detect-main spec).
    /// Bare `.zbc` files without zpkg metadata REQUIRE this positional
    /// argument; no silent fallback. **z42-test-runner** is the main
    /// consumer: it forks `z42vm <file> <test_method>` per `[Test]`
    /// discovered via TIDX.
    entry: Option<String>,

    /// Execution mode override (default: jit when built with the `jit`
    /// feature, else interp)
    #[arg(long, value_enum)]
    mode: Option<ExecMode>,

    /// Enable verbose tracing
    #[arg(short, long)]
    verbose: bool,

    /// Print runtime build info (version / target / build profile / enabled features /
    /// exec modes / libs dir / Z42_PATH) and exit. Useful for bug reports and CI
    /// preflight. docs/review.md Part 4 D5 (2026-05-25).
    #[arg(long)]
    info: bool,

    /// Set a runtime knob for this run: `--set <key>=<value>`, repeatable.
    /// Highest-precedence layer (above `Z42_*` env vars and config files).
    /// Keys are the knob's `[runtime]` key (e.g. `gc-mode`); run
    /// `z42vm --list-knobs` for the full list.
    #[arg(long = "set", value_name = "KEY=VALUE")]
    set: Vec<String>,

    /// Print every runtime knob's schema (type / settable layers / availability /
    /// default) and exit. `--all` additionally lists unsupported + internal knobs.
    #[arg(long)]
    list_knobs: bool,

    /// Print each knob's effective value with its source layer — and, for values
    /// that did not take effect, why — then exit.
    #[arg(long)]
    show_config: bool,

    /// Include unsupported + internal knobs in `--list-knobs` / `--show-config`.
    #[arg(long)]
    all: bool,

    /// Emit `--list-knobs` / `--show-config` as JSON instead of text.
    #[arg(long)]
    json: bool,

    /// Treat config problems from env / config files as fatal instead of
    /// warning + falling back to the default. Equivalent to
    /// `Z42_STRICT_CONFIG=1`. CLI (`--set`) problems are always fatal.
    /// complete-runtime-settings P1 (2026-09-05) — the CI config-drift gate.
    #[arg(long)]
    strict_config: bool,

    /// Print runtime counter snapshot (builtin_calls / native_calls /
    /// jit_methods_compiled / exceptions_thrown / etc.) to stderr after
    /// the script exits cleanly. docs/review.md Part 4 D6 (2026-05-26).
    #[arg(long)]
    print_stats_on_exit: bool,

    /// Output format for `--print-stats-on-exit` (script-profiling P0):
    /// `text` (default, human block) or `json` (one-line object that
    /// `xtask profile` scrapes off stderr). No effect without
    /// `--print-stats-on-exit`.
    #[arg(long, value_enum, default_value = "text")]
    stats_format: StatsFormat,

    /// add-z42-launcher (2026-06-02): arguments forwarded to the running z42
    /// program. Everything after a literal `--` separator is collected here
    /// and exposed to z42 code via `Std.IO.Environment.GetCommandLineArgs()`
    /// — NOT parsed by z42vm itself. e.g. `z42vm app.zpkg Main -- a b c` →
    /// the program sees `["a", "b", "c"]`.
    #[arg(last = true)]
    args: Vec<String>,
}
// 2026-05-11 retire-z-codes: `--explain` / `--list-errors` were removed
// alongside the Rust-side `diagnostics` catalog. Use `z42c explain E####`
// for compile-time codes; runtime errors are typed z42 exceptions now.

// 2026-05-07 add-runtime-feature-flags (P4.1): variants are feature-gated so
// `--help` only advertises modes the binary can actually run, and clap rejects
// unsupported `--mode jit` requests with a friendly enum-list error.
/// `--stats-format` selector (script-profiling P0). `Text` is the historical
/// human block; `Json` is a single-line object for tooling.
#[derive(Clone, PartialEq, Eq, ValueEnum)]
enum StatsFormat {
    Text,
    Json,
}

#[derive(Clone, ValueEnum)]
enum ExecMode {
    Interp,
    #[cfg(feature = "jit")]
    Jit,
    #[cfg(feature = "aot")]
    Aot,
}

/// `--mode` value as the knob's string form — used to report a `--mode` /
/// `--set mode=` conflict in the user's own words.
fn exec_mode_name(m: &ExecMode) -> &'static str {
    match m {
        ExecMode::Interp => "interp",
        #[cfg(feature = "jit")]
        ExecMode::Jit => "jit",
        #[cfg(feature = "aot")]
        ExecMode::Aot => "aot",
    }
}

/// Resolve the config-driven default execution mode (`Z42_MODE` / `[runtime].mode`,
/// unify-run-modes P2). Sits below the `--mode` CLI flag and above the build
/// default. Returns `None` to fall through to the build default when: unset, an
/// unrecognized value, or a feature-gated mode (jit / aot) not compiled into this
/// build — each non-empty invalid case warns once on stderr so it isn't silent.
fn resolve_config_mode(mode: Option<&str>) -> Option<z42::metadata::ExecMode> {
    match mode {
        None => None,
        Some("interp") => Some(z42::metadata::ExecMode::Interp),
        Some("jit") => {
            #[cfg(feature = "jit")]
            { Some(z42::metadata::ExecMode::Jit) }
            #[cfg(not(feature = "jit"))]
            { eprintln!("z42: Z42_MODE/[runtime].mode=jit but this build has no jit feature; using build default"); None }
        }
        Some("aot") => {
            #[cfg(feature = "aot")]
            { Some(z42::metadata::ExecMode::Aot) }
            #[cfg(not(feature = "aot"))]
            { eprintln!("z42: Z42_MODE/[runtime].mode=aot but this build has no aot feature; using build default"); None }
        }
        Some(other) => {
            eprintln!("z42: Z42_MODE/[runtime].mode={other:?} not recognized (interp/jit/aot); using build default");
            None
        }
    }
}

fn main() -> Result<()> {
    // Heap profiling (script-profiling P0, `xtask profile --heap`): the dhat
    // profiler must outlive the whole run — held here so it flushes `dhat-heap.json`
    // (in cwd) when it drops at main() exit. Only present in the `dhat-heap` build.
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    let cli = Cli::parse();

    // Centralized runtime config — single source of truth for Z42_* env vars
    // consumed at boot (docs/review.md Part 4 D1, 2026-05-26). Subsystem
    // reads go through the process-global runtime_config().
    //
    // Precedence chain (unify-run-modes P0): env > [runtime] TOML file > default.
    // The optional config-file layer is named by Z42_CONFIG; a malformed file
    // is fatal (explicit error, never silent-default). Install the resolved
    // config as the global BEFORE tracing / subsystems read it, so the
    // [runtime] layer is visible everywhere. Z42_CONFIG unset → resolve(env,
    // None) == the previous env-only from_env() (non-breaking).
    let getenv = |n: &str| std::env::var(n).ok();

    // CLI layer (complete-runtime-settings P2) — parsed before anything reads the
    // config, since `--set log=...` has to be visible to init_tracing below.
    let cli_knobs = match z42::config::parse_set_args(&cli.set) {
        Ok(m) => m,
        Err(msg) => { eprintln!("{msg}"); std::process::exit(2); }
    };
    // `--mode` and `--set mode=` are the same precedence layer; refuse to guess.
    if let Err(msg) = z42::config::reject_flag_conflict(
        &cli_knobs, "Z42_MODE", "--mode", cli.mode.as_ref().map(exec_mode_name)
    ) {
        eprintln!("{msg}");
        std::process::exit(2);
    }

    // Two independent file layers: the user's own (`Z42_CONFIG`) and the app's
    // build-generated sidecar (`Z42_APP_CONFIG`). They merge per key with the user
    // winning — setting Z42_CONFIG must not discard what the app ships with.
    let runtime_table = z42::config::load_runtime_toml(&getenv)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let app_table = z42::config::load_app_config(&getenv)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let build_ctx = z42::config::BuildCtx::current();
    let inputs = z42::config::Inputs {
        cli: cli_knobs,
        user_config: runtime_table.as_ref(),
        app_config: app_table.as_ref(),
    };
    let (cfg, mut resolution) =
        z42::config::RuntimeConfig::resolve_with(&getenv, &inputs, &build_ctx);

    // Unknown keys are only reported for layers whose namespace z42vm owns — the
    // `[runtime]` table here, and `--set` keys (P2). Environment variables are NOT
    // scanned: the `Z42_` prefix is shared with the launcher, the test harness and
    // embedders, so "unknown knob Z42_HOME" would fire on every single run.
    for (table, layer) in [
        (runtime_table.as_ref(), z42::config::Layer::UserConfig),
        (app_table.as_ref(), z42::config::Layer::AppConfig),
    ] {
        let Some(table) = table else { continue };
        for key in z42::config::unknown_table_keys(table) {
            resolution.diagnostics.push(z42::config::unknown_key_diagnostic(layer, &key));
        }
    }

    // Severity split (complete-runtime-settings design.md Decision 3): CLI problems
    // are always fatal; env / config-file problems warn and fall back, unless strict
    // mode is on. Exit code 2 distinguishes "your configuration is wrong" from a
    // normal runtime failure (1).
    let strict = cli.strict_config
        || getenv("Z42_STRICT_CONFIG").and_then(|s| z42::config::parse_bool(&s)).unwrap_or(false);
    if let Err(msg) = resolution.into_result(strict) {
        eprintln!("{msg}");
        std::process::exit(2);
    }
    let resolution = resolution;

    if z42::config::init_runtime_config(cfg.clone()).is_err() {
        eprintln!("z42: runtime config already initialised before main(); [runtime] layer may be ignored");
    }

    init_tracing(cli.verbose, &cfg);
    install_panic_hook(cfg.crash_dir.clone());
    #[cfg(unix)]
    z42::signal_handler::install(cfg.crash_dir.as_deref());

    // Verbose-mode startup banner (docs/review.md D5 item 2, 2026-05-26).
    // One-line tracing::info — gated by EnvFilter / `--verbose` so the
    // default quiet boot is preserved. `--info` users get the full dump
    // from `print_build_info` instead.
    tracing::info!(
        "z42vm {} ({}) starting [pid={}]",
        env!("CARGO_PKG_VERSION"),
        build_feature_tag(),
        std::process::id(),
    );

    // Knob query surfaces (complete-runtime-settings P3). Like --info they run
    // before any module loading and do not need a <FILE>.
    if cli.list_knobs {
        print!("{}", if cli.json {
            z42::config::list_knobs_json(cli.all, &build_ctx)
        } else {
            z42::config::list_knobs_text(cli.all, &build_ctx)
        });
        return Ok(());
    }
    if cli.show_config {
        print!("{}", if cli.json {
            z42::config::show_config_json(&resolution, cli.all)
        } else {
            z42::config::show_config_text(&resolution, cli.all)
        });
        return Ok(());
    }

    // --info: print build info to stdout and exit before doing any module loading.
    if cli.info {
        print_build_info(&resolution, &build_ctx, &cfg);
        return Ok(());
    }

    // Resolve module search paths (Z42_PATH + cwd + cwd/modules); log only for now.
    let module_paths = resolve_module_paths(&cfg);
    if cli.verbose {
        log_module_paths(&module_paths);
    }

    // `file` is required when not in --info mode. clap can't express "required
    // unless --info" cleanly, so enforce it here.
    let file = cli.file.as_deref()
        .ok_or_else(|| anyhow::anyhow!(
            "missing required argument <FILE> (or pass --info / --list-knobs / --show-config)"))?;
    tracing::debug!("z42vm loading {}", file);

    // Locate stdlib libs directory.
    let libs_dir = resolve_libs_dir(&cfg);

    // Publish the resolved dir into $Z42_LIBS so an in-process program (notably
    // the z42c compiler, which reads $Z42_LIBS directly for cross-package dep
    // resolution) sees the same libs dir the VM uses — SDK layout works with no
    // manual `Z42_LIBS=` set. Only fills an unset/empty var; explicit values
    // are respected.
    if let Some(val) = libs_env_to_publish(
        std::env::var("Z42_LIBS").ok().as_deref(),
        libs_dir.as_deref(),
    ) {
        // Safety: z42 is single-threaded; no concurrent env reads during boot.
        unsafe { std::env::set_var("Z42_LIBS", &val); }
    }

    // Verbose libs-dir log (was inline before the former run sequence).
    if cli.verbose {
        match &libs_dir {
            Some(dir) => log_libs(dir),
            None => tracing::info!("libs dir: not found (set $Z42_LIBS or run package.sh)"),
        }
    }

    // Resolve effective execution mode ONCE: explicit `--mode` wins; else
    // config-driven (`Z42_MODE` / `[runtime].mode`, unify-run-modes P2); else the
    // build default (jit if compiled in, else interp). Feature-gated arms must
    // themselves be gated (constructor absent when the feature is off).
    let effective_mode: z42::metadata::ExecMode = match cli.mode {
        #[cfg(feature = "jit")]
        Some(ExecMode::Jit) => z42::metadata::ExecMode::Jit,
        #[cfg(feature = "aot")]
        Some(ExecMode::Aot) => z42::metadata::ExecMode::Aot,
        Some(ExecMode::Interp) => z42::metadata::ExecMode::Interp,
        None => resolve_config_mode(z42::config::runtime_config().mode.as_deref())
            .unwrap_or_else(|| {
                #[cfg(feature = "jit")]
                { z42::metadata::ExecMode::Jit }
                #[cfg(not(feature = "jit"))]
                { z42::metadata::ExecMode::Interp }
            }),
    };

    // Shared app-run core (add-embedded-app-run): load the app + execute its
    // entry — the same core the embedding path (z42-host::run_app) uses.
    z42::app::run(
        file,
        cli.entry.as_deref(),
        z42::app::RunOpts {
            mode: effective_mode,
            libs_dir,
            program_args: cli.args.clone(),
            print_stats: cli.print_stats_on_exit,
            stats_json: cli.stats_format == StatsFormat::Json,
        },
    )
}

#[cfg(test)]
mod main_tests;

#[cfg(test)]
mod startup_tests;
