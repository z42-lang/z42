//! Diagnostics emitted when an application run finishes or exits the process.
use crate::vm::VmContext;
use std::path::Path;

struct ExitReport {
    print_stats: bool,
    stats_json: bool,
}

/// First-wins: an embedder or a nested `run` must not clobber the CLI's choice.
static EXIT_REPORT: std::sync::OnceLock<ExitReport> = std::sync::OnceLock::new();

pub(crate) fn configure_exit_report(print_stats: bool, stats_json: bool) {
    let _ = EXIT_REPORT.set(ExitReport {
        print_stats,
        stats_json,
    });
}

/// Emit the end-of-run counter snapshot (when requested) and flush the sampling
/// profiler. Called on the normal return path by `run_app` and, before the process
/// dies, by `Environment.Exit`. Safe to call with no `EXIT_REPORT` recorded
/// (pure-embedder VMs): stats are then skipped and only the env-driven sampler
/// flush applies.
pub(crate) fn report_on_exit(
    ctx: &VmContext,
    sampling_paths: impl FnOnce() -> (&'static Path, Option<&'static Path>),
) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    // Nested and repeated embedded runs share the same process report.
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let report = EXIT_REPORT.get();
    let diagnostics = ctx.core().diagnostics();
    // --stats: snapshot counters AFTER vm.run (even on error).
    // JSON form (--stats=json) is one line for `xtask profile` to scrape;
    // both go to stderr so program stdout stays the script's own output.
    // Merge the RuntimeCounters snapshot with heap-owned numbers (allocations +
    // GC minor/major/reclaimed) into one ProfileSnapshot so the profile picture
    // is complete in a single line/block.
    if let Some(report) = report.filter(|report| report.print_stats) {
        let (counters, h) = super::capture_counters(ctx);
        // add-concurrency-probes (P1b): safepoint-park distribution (always-on)
        // + user-lock contention counters (0 unless the `profile-contention`
        // feature is built in).
        let (park_count, park_us_total, park_max_us) = {
            let ph = diagnostics.park_histogram.lock();
            (
                ph.count,
                ph.total_us,
                if ph.count == 0 { 0 } else { ph.max_us },
            )
        };
        use std::sync::atomic::Ordering::Relaxed;
        let snap = crate::diagnostics::ProfileSnapshot::new(
            counters,
            h.allocations,
            h.minor_collections,
            h.major_collections,
            h.reclaimed_bytes,
        )
        .with_footprint(h.used_bytes, h.committed_bytes)
        .with_concurrency(
            park_count,
            park_us_total,
            park_max_us,
            diagnostics.lock_contentions.load(Relaxed),
            diagnostics.lock_wait_us.load(Relaxed),
        );
        if report.stats_json {
            eprintln!("{}", snap.to_json());
        } else {
            eprintln!("{snap}");
        }
    }

    // Sampling output is independent of counter reporting.
    if diagnostics.sampler.enabled() {
        let (folded_path, trace_path) = sampling_paths();
        let folded_path = folded_path.to_string_lossy();
        match diagnostics.sampler.flush_folded(&folded_path) {
            Ok(()) => eprintln!(
                "z42: sampling profiler → folded stacks: {folded_path}  \
                 (flamegraph: `inferno-flamegraph < {folded_path} > flame.svg`)"
            ),
            Err(e) => eprintln!("z42: failed to write sample folded stacks to {folded_path}: {e}"),
        }
        if let Some(trace_path) = trace_path {
            let trace_path = trace_path.to_string_lossy();
            match diagnostics.sampler.flush_trace(&trace_path) {
                Ok(()) => eprintln!(
                    "z42: sampling profiler → perfetto trace: {trace_path}  \
                     (open at https://ui.perfetto.dev)"
                ),
                Err(e) => eprintln!("z42: failed to write sample trace to {trace_path}: {e}"),
            }
        }
    }
}
