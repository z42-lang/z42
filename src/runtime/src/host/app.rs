//! One-shot application execution through the Rust host service.
use std::{path::{Path, PathBuf}, sync::Arc};
use anyhow::Result;
use crate::{zpkg::ExecMode, pal::fs::Vfs};
use super::{app_state::set_program_args, runtime::{LoadOptions, LoadedProgram}};
fn vfs_is_dir(fs: &dyn Vfs, p: &Path) -> bool { p.to_str().is_some_and(|s| fs.is_dir(s)) }
/// Build-default execution mode: JIT when compiled in (make-jit-default),
/// else Interp (jit-less builds — wasm / `--features interp-only`). Callers
/// without an explicit mode (embedding one-shot) use this.
pub fn default_mode() -> ExecMode {
    #[cfg(all(feature = "jit", z42_jit_supported))]
    { ExecMode::Jit }
    #[cfg(not(all(feature = "jit", z42_jit_supported)))]
    { ExecMode::Interp }
}

/// Resolved options for [`run_app`]. The caller resolves execution mode, the stdlib
/// `libs` dir, program args, and stat-printing; `run` performs the load + execute.
pub struct RunOpts {
    /// Effective execution mode (already resolved from CLI / config / build default).
    pub mode: ExecMode,
    /// Stdlib `libs/` directory (where `z42.core.zpkg` and deps live), if found.
    pub libs_dir: Option<PathBuf>,
    /// Program args forwarded to `GetCommandLineArgs()` (the app's `-- <args>`).
    pub program_args: Vec<String>,
    /// Print counter stats to stderr after the run (z42vm `--print-stats-on-exit`).
    pub print_stats: bool,
    /// When `print_stats`, emit a single-line JSON object instead of the human
    /// text block (z42vm `--stats-format=json`; `xtask profile` scrapes it).
    pub stats_json: bool,
    /// The process exits as soon as `run` returns (z42vm): leave the VM to the
    /// OS instead of freeing it block by block. Embedders and nested runs keep
    /// using the process, so they say `false`.
    pub exit_after_run: bool,
    /// The file system the app is loaded from and runs on (the VM's [`Vfs`]). `None` =
    /// [`crate::pal::fs::default_vfs`]. An embedder that mounted the app into a
    /// `MemoryFs` passes that here; a nested run passes its caller's.
    pub vfs: Option<Arc<dyn Vfs>>,
}

pub fn run_app(file: &str, entry: Option<&str>, opts: RunOpts) -> Result<()> {
    let libs_dir = opts.libs_dir;
    let vfs = opts.vfs.unwrap_or_else(crate::pal::fs::default_vfs);
    let fs = &*vfs;
    // Dependency search dirs (support-colocated-zpkg-deps): resolve a dep zpkg
    // from the ENTRY zpkg's own directory first, then any declared probing paths,
    // then the stdlib `libs/`. Fixed order for deterministic resolution; de-duped.
    //
    // add-deployment-model 批 3: `Z42_PROBING_PATHS` / `[runtime] probing-paths` sits
    // **between** entry-dir and libs — a dependency deployed next to the exe still wins
    // over a shared copy, and a shared copy still wins over the framework's own. This is
    // the whole of "declare it shared and it is not copied": the build leaves it out of
    // the dist, and this is where it is found instead.
    let search_dirs: Vec<PathBuf> = {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let entry_dir = match std::path::Path::new(file).parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        if vfs_is_dir(fs, &entry_dir) {
            dirs.push(entry_dir.clone());
        }
        // Relative probing paths resolve against the entry zpkg's dir, so the same
        // install behaves identically whatever cwd it is launched from.
        for p in crate::loader::locate::probing::expand_probing_paths(&entry_dir, &crate::config::runtime_config().probing_paths) {
            if !dirs.iter().any(|d| d == &p) {
                dirs.push(p);
            }
        }
        if let Some(libs) = &libs_dir {
            if !dirs.iter().any(|d| d == libs) {
                dirs.push(libs.clone());
            }
        }
        dirs
    };
    // 交给 z42 侧只读查询（`Std.Runtime.ModuleSearch.Dirs`）。存的是**解析结果**——相对项已按
    // entry zpkg 解析、通配符已展开 —— 免得调用方各自重做一遍规则然后慢慢漂移。
    crate::loader::locate::probing::set_search_dirs(search_dirs.clone());
    // add-sdk-libs D6：记下展开后不存在的 `${Z42_HOME}` 条目，供依赖解析失败时提示「是否没装 SDK」。
    {
        let entry_dir = match std::path::Path::new(file).parent() {
            Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
            _ => PathBuf::from("."),
        };
        crate::loader::locate::probing::set_unresolved_sdk_patterns(crate::loader::locate::probing::unresolved_z42_home_patterns(
            &entry_dir,
            &crate::config::runtime_config().probing_paths,
        ));
    }

    let program = LoadedProgram::load_file(file, LoadOptions {
        components: Default::default(), backends: Default::default(),
        mode: opts.mode, vfs, search_dirs, resolver: None, eager_dependencies: false,
    })?;
    set_program_args(program.context(), opts.program_args)?;
    let _ = EXIT_REPORT.set(ExitReport { print_stats: opts.print_stats, stats_json: opts.stats_json });
    let result = program.run_entry(entry);
    report_on_exit(program.context());
    if opts.exit_after_run { std::mem::forget(program); }
    result
}
/// End-of-run reporting knobs, recorded by [`run_app`] so the **`Environment.Exit`
/// path can produce the same output as a normal return**
/// (add-gc-runtime-knobs).
///
/// `Environment.Exit` is `std::process::exit`, which unwinds nothing and runs no
/// `Drop`. Before this existed it silently skipped both `--print-stats-on-exit`
/// and the sampling profiler's flush — so neither worked for *any* program that
/// exits that way, which includes z42c itself (`z42c.driver` ends every command
/// with `Environment.Exit(code)`). The VM's own profiling tooling was blind to
/// the project's most important workload.
struct ExitReport {
    print_stats: bool,
    stats_json: bool,
}

/// First-wins: an embedder or a nested `run` must not clobber the CLI's choice.
static EXIT_REPORT: std::sync::OnceLock<ExitReport> = std::sync::OnceLock::new();

/// Emit the end-of-run counter snapshot (when requested) and flush the sampling
/// profiler. Called on the normal return path by [`run_app`] and, before the process
/// dies, by `Environment.Exit`. Safe to call with no `EXIT_REPORT` recorded
/// (pure-embedder VMs): stats are then skipped and only the env-driven sampler
/// flush applies.
pub(crate) fn report_on_exit(ctx: &crate::context::VmContext) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    // A z42 `Environment.Exit` inside a program that also returns normally can
    // only happen once (process::exit never returns), but an embedder may call
    // `run` repeatedly — report at most once per process either way.
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let default = ExitReport { print_stats: false, stats_json: false };
    let report = EXIT_REPORT.get().unwrap_or(&default);
// --print-stats-on-exit: snapshot counters AFTER vm.run (even on error).
// JSON form (--stats-format=json) is one line for `xtask profile` to scrape;
// both go to stderr so program stdout stays the script's own output.
// Merge the RuntimeCounters snapshot with heap-owned numbers (allocations +
// GC minor/major/reclaimed) into one ProfileSnapshot so the profile picture
// is complete in a single line/block.
if report.print_stats {
    let counters = ctx.counters_snapshot();
    let h = ctx.heap().stats();
    // add-concurrency-probes (P1b): safepoint-park distribution (always-on)
    // + user-lock contention counters (0 unless the `profile-contention`
    // feature is built in).
    let (park_count, park_us_total, park_max_us) = {
        let ph = ctx.core().diagnostics().park_histogram.lock();
        (ph.count, ph.total_us, if ph.count == 0 { 0 } else { ph.max_us })
    };
    use std::sync::atomic::Ordering::Relaxed;
    let snap = crate::diagnostics::counters::ProfileSnapshot::new(
        counters, h.allocations,
        h.minor_collections, h.major_collections, h.reclaimed_bytes,
    )
    .with_footprint(h.used_bytes, h.committed_bytes)
    .with_concurrency(
        park_count, park_us_total, park_max_us,
        ctx.core().diagnostics().lock_contentions.load(Relaxed),
        ctx.core().diagnostics().lock_wait_us.load(Relaxed),
    );
    if report.stats_json {
        eprintln!("{}", snap.to_json());
    } else {
        eprintln!("{snap}");
    }
}

// add-sampling-profiler (P2): flush the safepoint sampling profiler's folded
// stacks (+ optional perfetto trace) after the run. Gated on the sampler
// being enabled (Z42_SAMPLE_HZ set) — the default-off path does nothing.
// Independent of --print-stats-on-exit: the sampler is its own env-driven
// profiling channel.
if ctx.core().diagnostics().sampler.enabled() {
    let cfg = crate::config::runtime_config();
    let folded_path = cfg.sample_out.to_string_lossy();
    match ctx.core().diagnostics().sampler.flush_folded(&folded_path) {
        Ok(()) => eprintln!(
            "z42: sampling profiler → folded stacks: {folded_path}  \
             (flamegraph: `inferno-flamegraph < {folded_path} > flame.svg`)"
        ),
        Err(e) => eprintln!("z42: failed to write sample folded stacks to {folded_path}: {e}"),
    }
    if let Some(trace_path) = cfg.trace_out.as_ref() {
        let trace_path = trace_path.to_string_lossy();
        match ctx.core().diagnostics().sampler.flush_trace(&trace_path) {
            Ok(()) => eprintln!(
                "z42: sampling profiler → perfetto trace: {trace_path}  \
                 (open at https://ui.perfetto.dev)"
            ),
            Err(e) => eprintln!("z42: failed to write sample trace to {trace_path}: {e}"),
        }
    }
}

}


