use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

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

/// Build the declared-but-not-loaded zpkg candidate set for the lazy loader.
///
/// Sources (in order, deduped by zpkg file name):
///   1. `.zpkg` main artifact's `dependencies` (DEPS section)
///   2. `.zbc`  main artifact's `import_namespaces` — reverse-lookup into
///      `libs_dir` for zpkgs declaring each namespace
///
/// Entries whose file name is already in `initially_loaded` (e.g. `z42.core.zpkg`
/// eager-loaded at startup, or — AOT only — deps already merged by the BFS) are
/// excluded. make-vm-loading-lazy: JIT now populates this set like interp (only
/// z42.core is pre-loaded), so JIT lazily loads deps on first cross-zpkg call.
fn build_declared_candidates(
    user_artifact: &z42::metadata::LoadedArtifact,
    search_dirs:   &[PathBuf],
    initially_loaded: &[String],
) -> Vec<(String, z42::metadata::lazy_loader::ZpkgCandidate)> {
    let mut declared: Vec<(String, z42::metadata::lazy_loader::ZpkgCandidate)> = Vec::new();
    if search_dirs.is_empty() { return declared; }

    let loaded_has = |name: &str| initially_loaded.iter().any(|f| f == name);
    let declared_has = |d: &[(String, _)], name: &str| d.iter().any(|(f, _)| f == name);

    // Namespace reverse-lookup searches every dep dir (entry-zpkg dir + libs).
    let libs_paths = search_dirs.to_vec();

    // .zpkg dependencies (DEPS): file field is authoritative; fall back to
    // the sibling `namespaces` field if the literal filename does not resolve
    // (e.g. GoldenTests writes `${ns}.zpkg` which will not match real stdlib
    // package filenames like `z42.collections.zpkg`).
    for dep in &user_artifact.dependencies {
        if loaded_has(&dep.file) || declared_has(&declared, &dep.file) { continue; }
        if let Ok(cand) = z42::metadata::lazy_loader::ZpkgCandidate::build_in_dirs(search_dirs, &dep.file) {
            declared.push((dep.file.clone(), cand));
            continue;
        }
        // Fallback: reverse lookup by namespaces.
        for ns in &dep.namespaces {
            let Ok(zpkg_paths) = z42::metadata::resolve_namespace(ns, &[], &libs_paths) else {
                continue;
            };
            for zpkg_path in zpkg_paths {
                let Some(file_name) = zpkg_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
                else { continue };
                if loaded_has(&file_name) || declared_has(&declared, &file_name) { continue; }
                match z42::metadata::lazy_loader::ZpkgCandidate::build_in_dirs(search_dirs, &file_name) {
                    Ok(cand) => declared.push((file_name, cand)),
                    Err(e)   => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
                }
            }
        }
    }

    // .zbc import_namespaces — reverse lookup
    for ns in &user_artifact.import_namespaces {
        let Ok(zpkg_paths) = z42::metadata::resolve_namespace(ns, &[], &libs_paths) else {
            continue;
        };
        for zpkg_path in zpkg_paths {
            let Some(file_name) = zpkg_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
            else { continue };
            if loaded_has(&file_name) { continue; }
            if declared_has(&declared, &file_name) { continue; }
            match z42::metadata::lazy_loader::ZpkgCandidate::build_in_dirs(search_dirs, &file_name) {
                Ok(cand) => declared.push((file_name, cand)),
                Err(e)   => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
            }
        }
    }

    declared
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

    // Dependency search dirs (support-colocated-zpkg-deps, 2026-06-20): resolve
    // a dep zpkg from the ENTRY zpkg's own directory first, then the stdlib
    // `libs/`. This lets an apphost ship its payload + that payload's package
    // deps together — e.g. `programs/z42c/z42c.driver.zpkg` finds its sibling
    // `z42c.core.zpkg` even though those aren't in `libs/`. Order is fixed
    // (entry dir, then libs) so resolution stays deterministic; de-duped so a
    // self-contained dir doesn't get scanned twice.
    let search_dirs: Vec<PathBuf> = {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(entry_dir) = std::path::Path::new(file).parent() {
            let entry_dir = if entry_dir.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                entry_dir.to_path_buf()
            };
            if entry_dir.is_dir() {
                dirs.push(entry_dir);
            }
        }
        if let Some(libs) = &libs_dir {
            if !dirs.iter().any(|d| d == libs) {
                dirs.push(libs.clone());
            }
        }
        dirs
    };

    if cli.verbose {
        match &libs_dir {
            Some(dir) => log_libs(dir),
            None => tracing::info!("libs dir: not found (set $Z42_LIBS or run package.sh)"),
        }
    }

    let mut modules: Vec<z42::metadata::Module> = Vec::new();
    // Track canonical paths of loaded artifact files to prevent duplicate loading.
    let mut loaded_paths: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    // Track zpkg file names loaded eagerly at startup (initially_loaded input
    // for the lazy loader — these are excluded from on-demand candidate set).
    let mut initially_loaded_zpkgs: Vec<String> = Vec::new();

    // add-runtime-observer (2026-05-26): collect (name, byte_size) for every
    // module loaded during boot, then replay-emit as `ModuleLoaded` events
    // AFTER VmContext::with_module installs the observer registry. We can't
    // emit at load time because the registry doesn't exist until VmCore is
    // built. Phase 2: lazy_loader will emit synchronously per on-demand load.
    let mut loaded_for_replay: Vec<(String, Option<u64>)> = Vec::new();

    // add-crosspkg-impl-reflection: collect (target_fq, trait_fq) impl pairs
    // from every eagerly-loaded artifact; seeded into the lazy loader below.
    let mut eager_impl_pairs: Vec<(String, String)> = Vec::new();

    // 5.1b — unconditionally try to load z42.core.zpkg if present.
    if let Some(ref dir) = libs_dir {
        let core_path = dir.join("z42.core.zpkg");
        if core_path.exists() {
            let core_canonical = core_path.canonicalize().unwrap_or(core_path.clone());
            let core_str = core_path.to_string_lossy().into_owned();
            match z42::metadata::load_artifact(&core_str) {
                Ok(a) => {
                    tracing::debug!("loaded stdlib z42.core from {core_str}");
                    let byte_size = std::fs::metadata(&core_path).ok().map(|m| m.len());
                    loaded_for_replay.push(("z42.core.zpkg".to_string(), byte_size));
                    eager_impl_pairs.extend(a.impl_pairs.iter().cloned());
                    modules.push(a.module);
                    loaded_paths.insert(core_canonical);
                    initially_loaded_zpkgs.push("z42.core.zpkg".to_string());
                }
                Err(e) => tracing::warn!("failed to load z42.core: {e:#}"),
            }
        } else {
            tracing::debug!("z42.core.zpkg not found in {}", dir.display());
        }
    }

    // 5.1c — load the user artifact.
    let user_artifact = z42::metadata::load_artifact(file)?;
    // add-runtime-observer: record user artifact for ModuleLoaded replay.
    {
        let byte_size = std::fs::metadata(file).ok().map(|m| m.len());
        loaded_for_replay.push((file.to_string(), byte_size));
    }

    // Resolve the effective execution mode ONCE so both the dependency-loading
    // strategy (below) and `Vm::new` (later) agree. Explicit `--mode` wins;
    // otherwise default to JIT when the feature is compiled in
    // (make-jit-default, 2026-06-20), falling back to interp for jit-less
    // builds (e.g. wasm / `--features interp-only`).
    // P4.1: arms referencing feature-gated CLI variants must themselves be
    // gated, else the constructor doesn't exist when the feature is off.
    let effective_mode: z42::metadata::ExecMode = match cli.mode {
        #[cfg(feature = "jit")]
        Some(ExecMode::Jit) => z42::metadata::ExecMode::Jit,
        #[cfg(feature = "aot")]
        Some(ExecMode::Aot) => z42::metadata::ExecMode::Aot,
        Some(ExecMode::Interp) => z42::metadata::ExecMode::Interp,
        // No `--mode` CLI → config-driven default (unify-run-modes P2):
        // `Z42_MODE` / `[runtime].mode` (below CLI, above build default), else
        // the build default (jit if compiled in, else interp).
        None => resolve_config_mode(z42::config::runtime_config().mode.as_deref())
            .unwrap_or_else(|| {
                #[cfg(feature = "jit")]
                { z42::metadata::ExecMode::Jit }
                #[cfg(not(feature = "jit"))]
                { z42::metadata::ExecMode::Interp }
            }),
    };

    // 5.1d — dependency loading strategy:
    //   Interp / JIT mode → pure lazy. Zpkgs are loaded on demand when a Call
    //     to an unresolved function is hit: interp via exec_instr, JIT via
    //     `jit_call` → `resolve_id_by_name` → `try_lookup_function` →
    //     `compile_fn` (native). See metadata/lazy_loader.rs.
    //   AOT mode → eager (transitive BFS): the whole program is compiled ahead
    //     of time, so every callee must be merged up front.
    //
    // make-vm-loading-lazy (2026-07-24): JIT dropped its eager BFS merge and now
    // shares interp's lazy loader — a short-lived program compiles only the
    // functions it actually calls (from only the zpkgs it actually touches),
    // instead of the entire transitive stdlib closure. Cross-zpkg calls, static
    // init (see `JitModule::run`), and ConstStr overflow (see `jit_const_str`)
    // all route through the lazy loader now.
    let is_eager = matches!(effective_mode, z42::metadata::ExecMode::Aot);
    if is_eager {
        // Eager + TRANSITIVE: BFS over the whole dependency graph so indirectly
        // declared zpkgs are merged too — not just the entry's direct deps.
        //
        // fix-jit-cross-zpkg-call (2026-06-20): JIT requires every callee
        // pre-compiled into the module, but the previous code loaded only the
        // entry's direct `dependencies` / `import_namespaces`. A transitive
        // target (e.g. `Std.Toml.TomlValue.Parse`, reached via
        // z42c.project → z42.toml) was therefore neither merged nor declared,
        // so it was unresolvable under `--mode jit` (interp dodged this by being
        // fully lazy with runtime transitive unfold). We drain a worklist of
        // dep files + namespaces, enqueuing each loaded artifact's own deps,
        // until the graph is exhausted. `loaded_paths` dedups by canonical path.
        if !search_dirs.is_empty() {
            use std::collections::VecDeque;
            // Resolve each dep file across all search dirs (entry-zpkg dir +
            // libs) — colocated package deps merge alongside the stdlib.
            let libs_paths = search_dirs.clone();
            let mut file_queue: VecDeque<String> =
                user_artifact.dependencies.iter().map(|d| d.file.clone()).collect();
            let mut ns_queue: VecDeque<String> =
                user_artifact.import_namespaces.iter().cloned().collect();
            loop {
                // Resolve pending namespaces to concrete zpkg files first.
                while let Some(ns) = ns_queue.pop_front() {
                    let Ok(zpkg_paths) = z42::metadata::resolve_namespace(&ns, &[], &libs_paths)
                    else { continue };
                    for zpkg_path in zpkg_paths {
                        if let Some(name) = zpkg_path.file_name().and_then(|n| n.to_str()) {
                            file_queue.push_back(name.to_string());
                        }
                    }
                }
                let Some(file) = file_queue.pop_front() else { break };
                // First search dir that actually has this file wins (fixed order
                // → deterministic; entry dir before libs).
                let Some(dep_path) = search_dirs.iter()
                    .map(|d| d.join(&file))
                    .find(|p| p.exists())
                else { continue };
                let canonical = dep_path.canonicalize().unwrap_or_else(|_| dep_path.clone());
                if !loaded_paths.insert(canonical) { continue; }  // already merged
                let dep_str = dep_path.to_string_lossy().into_owned();
                if let Ok(a) = z42::metadata::load_artifact(&dep_str) {
                    // Enqueue this artifact's own deps for transitive closure.
                    for d in &a.dependencies { file_queue.push_back(d.file.clone()); }
                    for ns in &a.import_namespaces { ns_queue.push_back(ns.clone()); }
                    eager_impl_pairs.extend(a.impl_pairs.iter().cloned());
                    modules.push(a.module);
                    initially_loaded_zpkgs.push(file.clone());
                }
            }
        }
    }

    // Build declared-but-not-loaded zpkg candidate set for the lazy loader,
    // BEFORE moving `user_artifact.module` into `modules` (partial-move).
    let declared_candidates = build_declared_candidates(
        &user_artifact,
        &search_dirs,
        &initially_loaded_zpkgs,
    );

    // 5.1e — push user module last, then merge everything.
    // Preserve the user module's name so entry-point lookup resolves correctly
    // (merge_modules uses the first module's name, which would be z42.core otherwise).
    let entry_hint = user_artifact.entry_hint.clone();
    let user_module_name = user_artifact.module.name.clone();
    eager_impl_pairs.extend(user_artifact.impl_pairs.iter().cloned());
    modules.push(user_artifact.module);

    let final_module = if modules.len() == 1 {
        modules.into_iter().next().unwrap()
    } else {
        let mut m = z42::metadata::merge_modules(modules)
            .with_context(|| format!("merging modules for `{}`", file))?;
        m.name = user_module_name;
        // interned_strings: populated inside merge_modules.
        z42::metadata::loader::build_type_registry(&mut m);
        z42::metadata::loader::verify_constraints(&m)
            .with_context(|| format!("constraint verification failed for `{}`", file))?;
        z42::metadata::loader::build_block_indices(&mut m);
        z42::metadata::loader::build_func_index(&mut m);
        m
    };

    // Construct the VmContext (consolidate-vm-state, 2026-04-28). The ctx
    // owns static-fields / pending-exception / lazy_loader; previously these
    // lived in thread_local slots scattered across interp/ and jit/.
    //
    // Lazy-loaded zpkgs will have their ConstStr indices offset past this
    // module's string-pool length. In interp AND JIT mode `declared_candidates`
    // drives on-demand loading (make-vm-loading-lazy unified the two); only AOT
    // pre-merges the closure into `modules` during 5.1d, leaving `declared` empty.
    // add-threading-stdlib (2026-05-20): module moves into VmCore (shared
    // across threads); Vm becomes a thin run-config wrapper.
    let string_pool_len = final_module.string_pool.len();
    let ctx = z42::vm_context::VmContext::with_module(final_module);
    // add-z42-launcher (2026-06-02): forward `-- <args>` to the program's
    // GetCommandLineArgs(). Done before vm.run so the program sees them.
    ctx.set_program_args(cli.args.clone());
    ctx.install_lazy_loader_with_deps(
        search_dirs.clone(),
        string_pool_len,
        declared_candidates,
        initially_loaded_zpkgs,
    );
    // fix-cross-pkg-subclass-fields (2026-05-14): seed lazy loader with merged
    // module's TypeDescs so cross-zpkg base classes are visible to the fixup
    // pass when a subclass-only zpkg is lazy-loaded later.
    let type_registry = ctx.module().unwrap().type_registry.clone();
    ctx.seed_lazy_loader_types(&type_registry);
    // add-crosspkg-impl-reflection: eagerly-loaded artifacts' impl pairs.
    ctx.seed_lazy_loader_impls(&eager_impl_pairs);

    // add-runtime-observer (2026-05-26): replay-emit ModuleLoaded for every
    // module loaded during boot. Empty registry = no-op. Once embedders
    // install observers (e.g. via z42.embedding API), they'll see boot loads
    // as if they had subscribed before startup.
    for (name, byte_size) in loaded_for_replay.drain(..) {
        ctx.fire_runtime_event(&z42::observer::RuntimeEvent::ModuleLoaded {
            name,
            byte_size,
        });
    }

    let vm = z42::vm::Vm::new(effective_mode);
    // CLI positional `entry` overrides any artifact-supplied entry hint.
    let effective_entry = cli.entry.as_deref().or(entry_hint.as_deref());
    let result = vm.run(&*ctx, effective_entry);

    // --print-stats-on-exit (docs/review.md Part 4 D6, 2026-05-26):
    // snapshot counters AFTER vm.run returns. On error (Err return) we
    // still print — partial run still has counter activity. Counters are
    // observation-only so even a failed run's counts are valid.
    if cli.print_stats_on_exit {
        let snap = ctx.counters().snapshot();
        eprintln!("{snap}");
    }

    result
}

#[cfg(test)]
mod main_tests;
