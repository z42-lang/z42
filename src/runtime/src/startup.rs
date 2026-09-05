//! `z42vm` 启动期辅助：stdlib 定位、tracing 初始化、panic hook、构建信息打印、
//! 模块搜索路径解析。
//!
//! refactor-split-main（complete-runtime-settings P5 前置，2026-09-05）：自
//! `main.rs` 逐行搬出的**纯移动**（无逻辑改动）。main.rs 在 line-limit 棘轮基线上
//! （568 行），本 change 给它加了 CLI `--set` / `--list-knobs` / `--show-config` 的
//! 接线后涨到 666——越界文件不得增长，故先拆。`main.rs` 留 CLI 定义 + `main()`
//! 编排，这里放它调用的启动步骤。

use std::path::PathBuf;
use z42::config::{RuntimeConfig, KNOWN_KNOBS};

/// Locate the stdlib libs/ directory.
///
/// Search order (redesign-artifact-layout, 2026-05-12):
///   1. `Z42_LIBS` knob (env / `[runtime].libs` / `--set libs=`)  — explicit override
///   2. `<binary-dir>/../libs/`                             — packages/<pkg>/libs/ adjacent
///   3. `<cwd>/artifacts/build/libraries/dist/release/`               — dev flat view (xtask build stdlib)
///   4. `<cwd>/artifacts/build/libraries/dist/debug/`                 — dev flat view (debug profile)
///   5. `<cwd>/artifacts/z42/libs/`                         — legacy fallback (pre-2026-05-12)
///
/// fix-phase1-knobs-bypass-config (2026-09-05): step 1 reads the **resolved**
/// `cfg.libs_dir`, not `std::env::var("Z42_LIBS")`. The raw env read silently
/// dropped every non-env layer, so `[runtime].libs` in a config file resolved
/// into `RuntimeConfig` (and was reported as `[user-config]` by `--info`) while
/// the actual lookup never saw it — `--info` contradicted itself in one run.
/// The knob's own spec declares `toml_key: "libs"` + `consumed_by: "main.rs"`,
/// so honouring the file layer is the documented contract, not a new feature.
pub fn resolve_libs_dir(cfg: &RuntimeConfig) -> Option<PathBuf> {
    // 1. Z42_LIBS knob (all layers)
    if let Some(p) = cfg.libs_dir.as_ref() {
        if p.is_dir() {
            return Some(p.clone());
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
pub fn libs_env_to_publish(current: Option<&str>, resolved: Option<&std::path::Path>) -> Option<String> {
    let unset = current.map_or(true, |v| v.trim().is_empty());
    if unset {
        resolved.map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Log discovered stdlib modules in libs_dir (verbose mode only).
pub fn log_libs(libs_dir: &PathBuf) {
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
pub fn init_tracing(verbose: bool, cfg: &z42::config::RuntimeConfig) {
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

pub fn default_filter(verbose: bool) -> &'static str {
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
///
/// fix-phase1-knobs-bypass-config (2026-09-05): the crash directory is captured
/// from the resolved `cfg.crash_dir` at install time rather than read from
/// `std::env::var("Z42_CRASH_DIR")` inside the hook. Besides honouring the
/// config-file layer, resolving *before* the hook runs keeps the panic path off
/// `getenv` — one less thing to go wrong while already panicking.
pub fn install_panic_hook(crash_dir: Option<PathBuf>) {
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

        // Optionally persist to the resolved crash dir for offline analysis
        if let Some(dir) = crash_dir.as_ref() {
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
pub fn build_feature_tag() -> String {
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
pub fn print_build_info(
    resolution: &z42::config::Resolution,
    ctx: &z42::config::BuildCtx,
    cfg: &RuntimeConfig,
) {
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

    // Config-file layers actually in play.
    for (var, label) in [("Z42_CONFIG", "user config"), ("Z42_APP_CONFIG", "app config")] {
        match std::env::var(var) {
            Ok(p) if !p.trim().is_empty() => println!("{label}: {p}"),
            _ => println!("{label}: (unset)"),
        }
    }

    // Knob block — rendered from the SAME Resolution the VM is actually running
    // with, by the same renderer --show-config uses. It used to re-read the
    // environment here and re-derive provenance, which meant the precedence chain
    // had two implementations that could drift (complete-runtime-settings P3).
    println!("--- runtime knobs ({}) ---", KNOWN_KNOBS.len());
    print!("{}", z42::config::show_config_text(resolution, true));
    println!("---");
    let unavailable: Vec<&str> = KNOWN_KNOBS.iter()
        .filter(|k| !z42::config::is_available(k, ctx))
        .map(|k| if k.toml_key.is_empty() { k.name } else { k.toml_key })
        .collect();
    if !unavailable.is_empty() {
        println!("unavailable in this build: {}", unavailable.join(", "));
    }

    // Effective libs dir lookup result. Uses the same resolved `cfg` the real
    // boot path uses, so this line can no longer contradict the knob table above
    // (fix-phase1-knobs-bypass-config).
    match resolve_libs_dir(cfg) {
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
///
/// fix-phase1-knobs-bypass-config (2026-09-05): entries come from the resolved
/// `cfg.module_path` (which already split on the platform separator), not a raw
/// `std::env::var("Z42_PATH")` — same config-layer bypass as `resolve_libs_dir`.
pub fn resolve_module_paths(cfg: &RuntimeConfig) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    // 1. Z42_PATH knob entries (all layers)
    for p in &cfg.module_path {
        if p.is_dir() && !paths.contains(p) {
            paths.push(p.clone());
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
pub fn log_module_paths(module_paths: &[PathBuf]) {
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
