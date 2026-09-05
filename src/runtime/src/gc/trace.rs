//! `Z42_GC_TRACE` — per-collection stderr trace (add-gc-runtime-knobs, 2026-09-05).
//!
//! The GC family had three tuning ratios and no way to see whether a collection
//! ever happened, which made the knobs unfalsifiable in practice: a program that
//! never collects and one that collects perfectly look identical from outside.
//! (`--print-stats-on-exit` gives end-of-run totals, but only for programs that
//! reach the normal exit path, and it cannot show *when* or *how often*.)
//!
//! One line per cycle, on stderr so it never mixes into program stdout:
//!
//! ```text
//! z42-gc: Full  used 412.3M -> 118.7M  freed 293.6M  pause 41.2ms  (cycle 7)
//! z42-gc: near heap limit: used 968.1M / 1.0G
//! ```
//!
//! Installed as a normal [`GcObserver`] only when the knob is on, so the
//! default path pays nothing (the observer list stays empty and `fire_event`
//! short-circuits).

use super::types::{GcEvent, GcKind, GcObserver};
use std::sync::atomic::{AtomicU64, Ordering};

/// Human-readable byte count: `118.7M`, `1.0G`, `512B`.
fn human(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b < K { return format!("{bytes}B"); }
    if b < K * K { return format!("{:.1}K", b / K); }
    if b < K * K * K { return format!("{:.1}M", b / (K * K)); }
    format!("{:.1}G", b / (K * K * K))
}

fn kind_str(k: &GcKind) -> &'static str {
    match k {
        GcKind::Full => "Full",
        GcKind::Minor => "Minor",
        GcKind::CycleCollector => "Cycle",
    }
}

/// Stderr GC tracer. Holds the `used_bytes` seen at `BeforeCollect` so the
/// `AfterCollect` line can show the before → after transition; a collection is
/// bracketed by exactly one of each on the collecting thread.
#[derive(Debug, Default)]
pub struct GcTracer {
    used_before: AtomicU64,
    cycles: AtomicU64,
    /// `OutOfMemory` fires **per allocation** once the budget is exceeded (the
    /// non-strict heap keeps allocating), so an un-deduped trace drowns out
    /// everything else — observed the first time this was run against a
    /// 64MB budget. Report the first, count the rest, summarise at the next
    /// collection.
    oom_suppressed: AtomicU64,
}

impl GcObserver for GcTracer {
    fn on_event(&self, event: &GcEvent) {
        match event {
            GcEvent::BeforeCollect { used_bytes, .. } => {
                self.used_before.store(*used_bytes, Ordering::Relaxed);
            }
            GcEvent::AfterCollect { kind, freed_bytes, pause_us } => {
                let before = self.used_before.load(Ordering::Relaxed);
                let n = self.cycles.fetch_add(1, Ordering::Relaxed) + 1;
                let oom = self.oom_suppressed.swap(0, Ordering::Relaxed);
                if oom > 1 {
                    eprintln!("z42-gc: ...{} further over-budget allocations since the last line",
                        oom - 1);
                }
                eprintln!(
                    "z42-gc: {:<5} used {} -> {}  freed {}  pause {:.1}ms  (cycle {n})",
                    kind_str(kind),
                    human(before),
                    human(before.saturating_sub(*freed_bytes)),
                    human(*freed_bytes),
                    *pause_us as f64 / 1000.0,
                );
            }
            GcEvent::NearHeapLimit { used_bytes, limit_bytes } => {
                eprintln!("z42-gc: near heap limit: used {} / {}",
                    human(*used_bytes), human(*limit_bytes));
            }
            GcEvent::OutOfMemory { requested_bytes, limit_bytes } => {
                if self.oom_suppressed.fetch_add(1, Ordering::Relaxed) == 0 {
                    eprintln!(
                        "z42-gc: over budget: requested {}, limit {}                          (allocation still served; further events counted, not printed)",
                        human(*requested_bytes), human(*limit_bytes));
                }
            }
            // AllocationPressure fires per allocation above the pressure ratio —
            // far too chatty for a trace line; the NearHeapLimit edge covers it.
            GcEvent::AllocationPressure { .. } => {}
        }
    }
}

#[cfg(test)]
#[path = "trace_tests.rs"]
mod trace_tests;
