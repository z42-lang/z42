//! Executable startup policy and the Windows SDK process entry.

#[cfg(feature = "cli")]
mod cli {
    use std::path::{Path, PathBuf};
    use z42::config::KNOWN_KNOBS;

    pub fn resolve_search_dirs(
        file: &str, libs: Option<&std::path::Path>, vfs: &dyn z42::pal::fs::Vfs,
        config: &z42::config::RuntimeConfig,
    ) -> Vec<PathBuf> {
        use z42::loader::locate::{probing, search_paths};
        let entry = search_paths::entry_directory(file);
        let extra = probing::expand_probing_paths(entry, &config.probing_paths);
        probing::set_unresolved_sdk_patterns(probing::unresolved_z42_home_patterns(entry, &config.probing_paths));
        search_paths::program_search_dirs(vfs, file, &extra, libs)
    }

    /// Resolve explicit settings or the deployed SDK layout. Development paths
    /// are supplied by the build tool through the same configuration interface.
    pub fn resolve_libs_dir() -> Option<PathBuf> {
        let executable = std::env::current_exe().ok();
        resolve_libs_dir_from(z42::config::runtime_config().libs_dir.as_deref(), executable.as_deref())
    }

    pub fn resolve_libs_dir_from(configured: Option<&Path>, executable: Option<&Path>) -> Option<PathBuf> {
        if let Some(path) = configured.filter(|path| !path.as_os_str().is_empty()) {
            // Preserve even a missing explicit path: silently using another SDK
            // would give the VM and its child compiler different library worlds.
            return Some(path.to_path_buf());
        }
        let directory = executable?.parent()?;
        let libs = directory.parent().unwrap_or(directory).join("libs");
        libs.is_dir().then_some(libs)
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
    /// docs/review.md Part 4 D2  + D1 RuntimeConfig migration
    /// : env var consumed via `RuntimeConfig` not inline read.
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
    /// post-mortem. docs/review.md Part 4 D4 — Phase 1 .
    ///
    /// Phase 1 covers Rust `panic!()` / unwrap / index OOB / assertion failures.
    /// Phase 2 (OS signal handler for SIGSEGV / SIGABRT) is a separate spec —
    /// needs the `signal-hook` crate and async-signal-safe primitives.
    ///
    /// Hook composes (not replaces) the default — calls default print first,
    /// then appends z42-specific context, then aborts to preserve "panic = bug,
    /// can't be caught" semantics.
    pub fn install_panic_hook() {
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
            // adopt-inline-env-knobs: the `crash-dir` knob, not a raw env read.
            if let Some(dir) = z42::config::runtime_config().crash_dir.as_ref()
                .map(|p| p.display().to_string()) {
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
    pub fn build_feature_tag() -> String {
        let mut tags: Vec<&str> = vec!["interp"];
        #[cfg(all(feature = "jit", z42_jit_supported))]            tags.push("jit");
        #[cfg(feature = "aot")]            tags.push("aot");
        #[cfg(feature = "native-interop")] tags.push("native");
        tags.join(",")
    }

    /// Print runtime build information to stdout. Triggered by `--info`.
    /// Output is intentionally human-readable + grep-friendly (one `key: value`
    /// per line). docs/review.md Part 4 D5  + D1 RuntimeConfig
    /// migration : enumerates `config::KNOWN_KNOBS` so adding a
    /// new knob is one table edit instead of also updating this function.
    pub fn print_build_info(resolution: &z42::config::Resolution, ctx: &z42::config::BuildCtx) {
        println!("z42vm {}", env!("CARGO_PKG_VERSION"));
        println!("target: {}", std::env::consts::OS);
        println!("arch: {}", std::env::consts::ARCH);
        println!("build profile: {}", if cfg!(debug_assertions) { "debug" } else { "release" });

        // Enabled feature flags
        let mut features: Vec<&str> = Vec::new();
        #[cfg(all(feature = "jit", z42_jit_supported))]            features.push("jit");
        #[cfg(feature = "aot")]            features.push("aot");
        #[cfg(feature = "native-interop")] features.push("native-interop");
        println!("features: {}", if features.is_empty() { "(none)".to_string() } else { features.join(", ") });

        // Exec modes actually available (function of feature flags)
        let mut modes: Vec<&str> = Vec::new();
        if z42::host::execution_available(z42::host::ExecMode::Interp) { modes.push("interp"); }
        if z42::host::execution_available(z42::host::ExecMode::Jit) { modes.push("jit"); }
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

        // Effective libs dir lookup result.
        match resolve_libs_dir() {
            Some(dir) => println!("libs dir: {}", dir.display()),
            None => println!("libs dir: (not configured; set Z42_LIBS or use a complete SDK)"),
        }
    }
}

#[cfg(feature = "cli")]
pub(crate) use cli::*;

#[cfg(feature = "windows-launcher")]
use std::{ffi::OsString, path::{Path, PathBuf}, process::{Command, ExitStatus}};

#[cfg(feature = "windows-launcher")]
fn payload_path(executable: &Path) -> std::io::Result<PathBuf> {
    let root = executable.parent().and_then(Path::parent)
        .ok_or_else(|| std::io::Error::other("z42vm has no SDK parent directory"))?;
    Ok(root.join("native").join("z42vm.exe"))
}

#[cfg(feature = "windows-launcher")]
fn forward(payload: &Path, args: impl IntoIterator<Item = OsString>) -> std::io::Result<ExitStatus> {
    Command::new(payload).args(args).status()
}

#[cfg(feature = "windows-launcher")]
fn main() -> std::io::Result<()> {
    let payload = payload_path(&std::env::current_exe()?)?;
    let status = forward(&payload, std::env::args_os().skip(1))?;
    std::process::exit(status.code().unwrap_or(1))
}

#[cfg(all(test, feature = "windows-launcher"))]
#[path = "main_startup_tests.rs"]
mod tests;
