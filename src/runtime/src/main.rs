use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

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

/// Locate the stdlib libs/ directory.
///
/// Search order (redesign-artifact-layout, 2026-05-12):
///   1. `$Z42_LIBS`                                         — env override
///   2. `<binary-dir>/../libs/`                             — packages/<pkg>/libs/ adjacent
///   3. `<cwd>/artifacts/build/libraries/dist/release/`               — dev flat view (xtask build stdlib)
///   4. `<cwd>/artifacts/build/libraries/dist/debug/`                 — dev flat view (debug profile)
///   5. `<cwd>/artifacts/z42/libs/`                         — legacy fallback (pre-2026-05-12)
fn resolve_libs_dir() -> Option<PathBuf> {
    // 1. $Z42_LIBS
    if let Ok(v) = std::env::var("Z42_LIBS") {
        let p = PathBuf::from(v);
        if p.is_dir() {
            return Some(p);
        }
    }
    // 2. <binary-dir>/../libs/  (packages 布局)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            let p = bin_dir.parent().unwrap_or(bin_dir).join("libs");
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    // 3-4. dev flat view（xtask build stdlib 产出）
    if let Ok(cwd) = std::env::current_dir() {
        for p in [
            cwd.join("artifacts/build/libraries/dist/release"),
            cwd.join("artifacts/build/libraries/dist/debug"),
            cwd.join("artifacts/z42/libs"), // legacy fallback
        ] {
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    None
}

/// Decide the value to publish into `$Z42_LIBS` so an in-process program
/// (notably the z42c compiler) resolves stdlib/deps from the same directory the
/// VM resolved — no manual `Z42_LIBS=` needed in SDK layout.
///
/// Rule: fill only an unset/empty var with the VM-resolved dir. An explicit
/// value is left untouched (a valid one is already what `resolve_libs_dir`
/// returns; an explicit-but-broken one is the caller's deliberate choice).
/// Empty string counts as unset (mirrors `RuntimeConfig` env handling).
fn libs_env_to_publish(current: Option<&str>, resolved: Option<&std::path::Path>) -> Option<String> {
    let unset = current.map_or(true, |v| v.trim().is_empty());
    if unset {
        resolved.map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Log discovered stdlib modules in libs_dir (verbose mode only).
fn log_libs(libs_dir: &PathBuf) {
    tracing::info!("libs dir: {}", libs_dir.display());
    match std::fs::read_dir(libs_dir) {
        Ok(entries) => {
            let mut found = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "zpkg" || ext == "zbc" {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            found.push(name.to_owned());
                        }
                    }
                }
            }
            found.sort();
            for name in &found {
                tracing::info!("  stdlib module: {name}");
            }
            if found.is_empty() {
                tracing::info!("  (no .zbc/.zpkg files found — stdlib not yet compiled)");
            }
        }
        Err(e) => tracing::warn!("cannot read libs dir: {e}"),
    }
}

/// Initialize the tracing subscriber. Precedence (highest wins):
///   1. `cfg.log_filter` (sourced from `Z42_LOG` env var via `RuntimeConfig`)
///      — `tracing-subscriber` directive syntax
///      (e.g. `Z42_LOG=z42::jit=debug,z42::gc=trace,z42=warn`)
///   2. `--verbose` CLI flag — defaults to `z42=info`
///   3. Otherwise: `z42=warn` (errors + warnings only; quiet boot)
///
/// docs/review.md Part 4 D2 (2026-05-25) + D1 RuntimeConfig migration
/// (2026-05-26): env var consumed via `RuntimeConfig` not inline read.
fn init_tracing(verbose: bool, cfg: &z42::config::RuntimeConfig) {
    use tracing_subscriber::EnvFilter;

    let filter = match cfg.log_filter.as_deref() {
        Some(s) => EnvFilter::try_new(s)
            .unwrap_or_else(|_| EnvFilter::new(default_filter(verbose))),
        None => EnvFilter::new(default_filter(verbose)),
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn default_filter(verbose: bool) -> &'static str {
    if verbose { "z42=info" } else { "z42=warn" }
}

/// Install a custom panic hook that prints VM context + Rust backtrace on
/// internal panic. When `Z42_CRASH_DIR` env var is set and writable, also
/// writes the report to `<dir>/z42vm-crash-<unix_ts_ns>.txt` for offline
/// post-mortem. docs/review.md Part 4 D4 — Phase 1 (2026-05-25).
///
/// Phase 1 covers Rust `panic!()` / unwrap / index OOB / assertion failures.
/// Phase 2 (OS signal handler for SIGSEGV / SIGABRT) is a separate spec —
/// needs the `signal-hook` crate and async-signal-safe primitives.
///
/// Hook composes (not replaces) the default — calls default print first,
/// then appends z42-specific context, then aborts to preserve "panic = bug,
/// can't be caught" semantics.
fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);

        let mut report = String::new();
        report.push_str("\n=== z42vm internal panic ===\n");
        report.push_str(&format!("z42vm version: {}\n", env!("CARGO_PKG_VERSION")));
        report.push_str(&format!("target: {}/{}\n", std::env::consts::OS, std::env::consts::ARCH));
        report.push_str(&format!("build profile: {}\n",
            if cfg!(debug_assertions) { "debug" } else { "release" }));

        if let Some(loc) = info.location() {
            report.push_str(&format!("panic location: {}:{}:{}\n", loc.file(), loc.line(), loc.column()));
        }

        let payload = info.payload();
        let msg: &str = payload.downcast_ref::<&str>().copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(non-string payload)");
        report.push_str(&format!("payload: {msg}\n"));

        // Rust backtrace — env var `RUST_BACKTRACE=1` controls capture
        let bt = std::backtrace::Backtrace::capture();
        report.push_str(&format!("rust backtrace:\n{bt}\n"));

        report.push_str("(z42 call stack capture pending — Part 4 D4 Phase 2)\n");
        report.push_str("============================\n");

        // Always print to stderr
        eprint!("{report}");

        // Optionally persist to Z42_CRASH_DIR for offline analysis
        if let Ok(dir) = std::env::var("Z42_CRASH_DIR") {
            let dir = std::path::PathBuf::from(dir);
            // Best-effort: create dir, write file, swallow errors (already panicking).
            let _ = std::fs::create_dir_all(&dir);
            let ts_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = dir.join(format!("z42vm-crash-{ts_ns}.txt"));
            if let Err(e) = std::fs::write(&path, &report) {
                eprintln!("[panic hook] failed to write crash report to {}: {e}", path.display());
            } else {
                eprintln!("[panic hook] crash report written to {}", path.display());
            }
        }
    }));
}

/// Compact comma-separated list of enabled build features for the verbose
/// startup banner — e.g. `"interp"`, `"interp,jit"`, `"interp,jit,native"`.
/// Mirrors the `features:` line in `--info` but in one short tag.
fn build_feature_tag() -> String {
    let mut tags: Vec<&str> = vec!["interp"];
    #[cfg(feature = "jit")]            tags.push("jit");
    #[cfg(feature = "aot")]            tags.push("aot");
    #[cfg(feature = "native-interop")] tags.push("native");
    tags.join(",")
}

/// Print runtime build information to stdout. Triggered by `--info`.
/// Output is intentionally human-readable + grep-friendly (one `key: value`
/// per line). docs/review.md Part 4 D5 (2026-05-25) + D1 RuntimeConfig
/// migration (2026-05-26): enumerates `config::KNOWN_KNOBS` so adding a
/// new knob is one table edit instead of also updating this function.
fn print_build_info() {
    use z42::config::KNOWN_KNOBS;

    println!("z42vm {}", env!("CARGO_PKG_VERSION"));
    println!("target: {}", std::env::consts::OS);
    println!("arch: {}", std::env::consts::ARCH);
    println!("build profile: {}", if cfg!(debug_assertions) { "debug" } else { "release" });

    // Enabled feature flags
    let mut features: Vec<&str> = Vec::new();
    #[cfg(feature = "jit")]            features.push("jit");
    #[cfg(feature = "aot")]            features.push("aot");
    #[cfg(feature = "native-interop")] features.push("native-interop");
    println!("features: {}", if features.is_empty() { "(none)".to_string() } else { features.join(", ") });

    // Exec modes actually available (function of feature flags)
    let mut modes: Vec<&str> = vec!["interp"];
    #[cfg(feature = "jit")] modes.push("jit");
    #[cfg(feature = "aot")] modes.push("aot");
    println!("exec modes: {}", modes.join(", "));

    // Effective [runtime] config-file layer (Z42_CONFIG). Loaded the same way
    // boot does so --info reflects what a real run would see. A malformed file
    // reports the error here instead of a value. Loaded once and reused for the
    // per-knob provenance below.
    let runtime_table = match std::env::var("Z42_CONFIG") {
        Ok(p) if !p.trim().is_empty() => match z42::config::load_runtime_toml(|n| std::env::var(n).ok()) {
            Ok(t @ Some(_)) => { println!("config file: {p} ([runtime] applied)"); t }
            Ok(None)        => { println!("config file: {p} (no [runtime] table)"); None }
            Err(e)          => { println!("config file: {p} (error: {e})"); None }
        },
        _ => { println!("config file: (unset; env + built-in defaults only)"); None }
    };

    // Runtime knobs — enumerate from KNOWN_KNOBS so this stays automatically in
    // sync as new env vars get registered. Each line shows the env name, its
    // [runtime] TOML key, and the *effective* value with provenance following
    // the real precedence chain: [env] > [config] > [default].
    println!("--- runtime knobs ({}) ---", KNOWN_KNOBS.len());
    for knob in KNOWN_KNOBS {
        let toml = if knob.toml_key.is_empty() { "-".to_string() } else { format!("[runtime].{}", knob.toml_key) };
        match std::env::var(knob.name) {
            Ok(v) if !v.trim().is_empty() => println!("{} ({toml}): {v} [env]", knob.name),
            _ => {
                let from_cfg = (!knob.toml_key.is_empty())
                    .then(|| runtime_table.as_ref().and_then(|t| t.get(knob.toml_key)))
                    .flatten()
                    .map(|v| match v {
                        toml::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    });
                match from_cfg {
                    Some(val) => println!("{} ({toml}): {val} [config]", knob.name),
                    None => println!("{} ({toml}): ({}) [default]", knob.name, knob.default_hint),
                }
            }
        }
    }
    println!("---");

    // Effective libs dir lookup result.
    match resolve_libs_dir() {
        Some(dir) => println!("libs dir: {}", dir.display()),
        None => println!("libs dir: (not found — run xtask build stdlib or set Z42_LIBS)"),
    }
}

/// Resolve module search paths from Z42_PATH, <cwd>/, and <cwd>/modules/.
///
/// Returns a deduplicated list of existing directories in priority order:
///   1. Each entry in `Z42_PATH` (colon-separated on Unix)
///   2. `<cwd>/`
///   3. `<cwd>/modules/`
fn resolve_module_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. Z42_PATH entries
    if let Ok(z42_path) = std::env::var("Z42_PATH") {
        for part in z42_path.split(':') {
            let p = PathBuf::from(part.trim());
            if p.is_dir() && !paths.contains(&p) {
                paths.push(p);
            }
        }
    }

    // 2. <cwd>/
    if let Ok(cwd) = std::env::current_dir() {
        if !paths.contains(&cwd) {
            paths.push(cwd.clone());
        }
        // 3. <cwd>/modules/
        let modules = cwd.join("modules");
        if modules.is_dir() && !paths.contains(&modules) {
            paths.push(modules);
        }
    }

    paths
}

/// Log discovered module paths and .zbc files in verbose mode.
fn log_module_paths(module_paths: &[PathBuf]) {
    for dir in module_paths {
        tracing::info!("module path: {}", dir.display());
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                let mut found = Vec::new();
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("zbc") {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            found.push(name.to_owned());
                        }
                    }
                }
                found.sort();
                for name in &found {
                    tracing::info!("  module: {name}");
                }
            }
            Err(e) => tracing::warn!("cannot read module path {}: {e}", dir.display()),
        }
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
    let runtime_table = z42::config::load_runtime_toml(|n| std::env::var(n).ok())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let cfg = z42::config::RuntimeConfig::resolve(
        |n| std::env::var(n).ok(),
        runtime_table.as_ref(),
    );
    if z42::config::init_runtime_config(cfg.clone()).is_err() {
        eprintln!("z42: runtime config already initialised before main(); [runtime] layer may be ignored");
    }

    init_tracing(cli.verbose, &cfg);
    install_panic_hook();
    #[cfg(unix)]
    z42::signal_handler::install();

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

    // --info: print build info to stdout and exit before doing any module loading.
    if cli.info {
        print_build_info();
        return Ok(());
    }

    // Resolve module search paths (Z42_PATH + cwd + cwd/modules); log only for now.
    let module_paths = resolve_module_paths();
    if cli.verbose {
        log_module_paths(&module_paths);
    }

    // `file` is required when not in --info mode. clap can't express "required
    // unless --info" cleanly, so enforce it here.
    let file = cli.file.as_deref()
        .ok_or_else(|| anyhow::anyhow!(
            "missing required argument <FILE> (or pass --info to print build info)"))?;
    tracing::debug!("z42vm loading {}", file);

    // Locate stdlib libs directory.
    let libs_dir = resolve_libs_dir();

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
