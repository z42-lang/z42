//! Automatic-collection policy: when allocation pressure trips a GC cycle.
//!
//! Split out of `alloc.rs` by add-gc-runtime-knobs (2026-09-05), which both gave
//! the policy something to decide (before `Z42_GC_MAX_BYTES` there was no way to
//! set a budget, so this code returned immediately every time) and exposed the
//! futility problem below.
//!
//! ## The policy
//!
//! Auto-collect trips when **both** hold:
//!
//! 1. `used >= gc_near_limit_ratio × max_bytes` — we are near the budget;
//! 2. `used` has grown by at least `gc_throttle_ratio × max_bytes` since the
//!    last trip — debounces back-to-back collections.
//!
//! `max_bytes` comes from `Z42_GC_MAX_BYTES`. **Unset means no automatic
//! collection at all** — the historical default, kept for compatibility.
//!
//! ## Futility backoff
//!
//! Rule 2 debounces on *growth*, which is not enough on its own: when the live
//! set genuinely exceeds the budget, every collection reclaims ~nothing, the
//! heap keeps growing anyway, and the growth gate re-arms forever. Measured on
//! `src/tests/perf/scenarios/09_alloc_ctorless` (whose 1.5M objects are all live) with a
//! 64MB budget: a full mark-sweep every ~6MB of growth, each freeing 0 bytes at
//! ~75ms, turning a 0.29s run into one that had not finished in 9 minutes.
//!
//! So each consecutive *unproductive* collection doubles the growth required
//! before trying again, capped at [`MAX_BACKOFF`]; one productive collection
//! resets it. A soft budget then behaves like a soft budget: try to honour it,
//! and stop burning CPU once it is provably unreachable.
//!
//! Productivity is read back from `stats.reclaimed_bytes` — the total is already
//! maintained by every collect path, so this needs no hook in any of them.

use std::sync::atomic::Ordering;

/// Cap on the growth-gate multiplier. With the 0.10 default throttle ratio this
/// means a hopeless heap re-collects at most a handful more times as it grows,
/// instead of every 10% of the budget forever.
const MAX_BACKOFF: u32 = 64;

impl crate::gc::arc_heap::ArcMagrGC {
    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: when the
    /// `external_needs_collect` flag is wired (post-`VmCore` construction) this
    /// only does `flag.store(true, Release)` — the collect itself is deferred to
    /// the next mutator `check_safepoint(ctx)` and runs inside its safepoint
    /// guard, so the root scanner never races a mutator's `regs` writes. With no
    /// flag wired (GC unit tests constructing `ArcMagrGC::new()` directly) it
    /// falls back to the inline collect, leaving single-threaded behaviour
    /// unchanged.
    pub(super) fn maybe_auto_collect(&self) {
        let (max_opt, last, paused, reclaimed_mark, backoff, total_reclaimed) = {
            let i = self.inner.lock();
            (i.stats.max_bytes, i.last_auto_collect_used, i.pause_count > 0,
             i.last_auto_collect_reclaimed,
             // `RcHeapInner` derives Default, so the multiplier starts at 0;
             // treat that as the neutral 1 rather than hand-writing a Default
             // impl for a 20-field struct (0 would zero the growth gate and
             // trip a collect on every allocation).
             i.auto_collect_backoff.max(1),
             i.stats.reclaimed_bytes)
        };
        let used = self.used_bytes_atomic();
        if paused { return; }
        let Some(limit) = max_opt else { return };
        let cfg = crate::config::runtime_config();
        let near_threshold = (limit as f64 * cfg.gc_near_limit_ratio) as u64;
        if used < near_threshold { return; }
        let base_delta = (limit as f64 * cfg.gc_throttle_ratio) as u64;
        if used.saturating_sub(last) < base_delta.saturating_mul(backoff as u64) { return; }

        // How much did the *previous* trip's collection actually reclaim? Less
        // than one growth-gate's worth means it did not buy us room; back off so
        // an over-budget live set stops re-collecting on every gate.
        let reclaimed_since = total_reclaimed.saturating_sub(reclaimed_mark);
        let next_backoff = if reclaimed_since < base_delta {
            backoff.saturating_mul(2).min(MAX_BACKOFF)
        } else {
            1
        };

        {
            // Mark the pre-collect watermarks so we don't re-trip on every
            // subsequent alloc while the deferred collect is still pending.
            let mut i = self.inner.lock();
            i.last_auto_collect_used = used;
            i.last_auto_collect_reclaimed = total_reclaimed;
            i.auto_collect_backoff = next_backoff;
        }

        // Defer to safepoint when wired (multi-thread safe path).
        if let Some(flag) = self.external_needs_collect.lock().clone() {
            flag.store(true, Ordering::Release);
            return;
        }
        // Fallback: legacy inline collect — preserves GC unit-test behaviour
        // (those tests construct ArcMagrGC::new() without VmCore wiring).
        self.collect_cycles();
    }
}

#[cfg(test)]
#[path = "auto_collect_tests.rs"]
mod auto_collect_tests;
