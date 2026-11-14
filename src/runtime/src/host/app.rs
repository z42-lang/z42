//! One-shot application execution through the Rust host service.
use anyhow::Result;
use crate::zpkg::ExecMode;
use super::{app_state::set_program_args, program::{LoadOptions, LoadedProgram}};
/// Build-default execution mode: JIT when compiled in (make-jit-default),
/// else Interp (jit-less builds — wasm / `--features interp-only`). Callers
/// without an explicit mode (embedding one-shot) use this.
pub fn default_mode() -> ExecMode {
    #[cfg(all(feature = "jit", z42_jit_supported))]
    { ExecMode::Jit }
    #[cfg(not(all(feature = "jit", z42_jit_supported)))]
    { ExecMode::Interp }
}

/// Query whether this host can prepare an execution mode, without loading code
/// or constructing a VM. Component identities remain an internal concern.
pub fn execution_available(mode: ExecMode) -> bool {
    use crate::components::api::{self, ExecutionRole};
    match mode {
        ExecMode::Interp => api::execution_available(ExecutionRole::Baseline),
        ExecMode::Jit => api::execution_available(ExecutionRole::Optimizing),
        ExecMode::Aot => false,
    }
}

/// A host implementation resolves loading inputs, program arguments and reporting.
/// This service loads and executes without choosing process search defaults.
pub struct RunOpts {
    /// Fully resolved loading inputs supplied by the host implementation.
    pub load_options: LoadOptions,
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

}

pub fn run_app(file: &str, entry: Option<&str>, opts: RunOpts) -> Result<()> {
    let program = LoadedProgram::load_file(file, opts.load_options)?;
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


