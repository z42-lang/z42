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
//!    *end of the last collection* — debounces back-to-back collections.
//!
//! Rule 2's baseline matters more than it looks. It used to be the `used` read
//! at the last *trip* — i.e. the pre-collect high-water mark, which sits at
//! `gc_near_limit_ratio × max_bytes` by construction (rule 1 just let it
//! through). Re-tripping then demanded
//! `(near_limit_ratio + throttle_ratio) × max_bytes` = **100% of the budget**
//! with the multiplier at 1, and more once it doubled — a bar a working
//! collector keeps the heap below, so the second cycle never came. Measured on
//! `z42c.semantics --release --no-incremental` with a 256MB budget: cycle 1
//! freed 186.3MB at 230.4MB used, the gate then wanted 295.3MB (110% of
//! budget, the multiplier having already doubled — see below), and the run
//! ended at 273.6MB having collected exactly once.
//!
//! The baseline is recovered without a hook into the collect paths: the
//! previous trip's collection freed `reclaimed_since` bytes, so `last -
//! reclaimed_since` is where `used` stood when it finished.
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
//! It always describes the collection the *previous* trip asked for, which
//! leaves the very first trip with nothing to judge: reading 0 reclaimed there
//! penalised a heap that had never been collected at all, so the multiplier
//! was already 2 before the first cycle had run. `gc_cycles == 0` now means
//! "neutral", not "futile".

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
        let (max_opt, last, paused, reclaimed_mark, backoff, total_reclaimed, cycles) = {
            let i = self.inner.lock();
            (i.stats.max_bytes, i.last_auto_collect_used, i.pause_count > 0,
             i.last_auto_collect_reclaimed,
             // `RcHeapInner` derives Default, so the multiplier starts at 0;
             // treat that as the neutral 1 rather than hand-writing a Default
             // impl for a 20-field struct (0 would zero the growth gate and
             // trip a collect on every allocation).
             i.auto_collect_backoff.max(1),
             i.stats.reclaimed_bytes,
             i.stats.gc_cycles)
        };
        let used = self.used_bytes_atomic();
        if paused { return; }
        let Some(limit) = max_opt else { return };
        let cfg = crate::config::runtime_config();
        let near_threshold = (limit as f64 * cfg.gc_near_limit_ratio) as u64;
        if used < near_threshold { return; }
        let base_delta = (limit as f64 * cfg.gc_throttle_ratio) as u64;

        // How much did the *previous* trip's collection actually reclaim?
        let reclaimed_since = total_reclaimed.saturating_sub(reclaimed_mark);
        // `last` is a pre-collect reading; the collection it tripped then freed
        // `reclaimed_since`. Growth is measured from where that left the heap,
        // so the gate asks for one throttle-ratio of *new* allocation rather
        // than for the heap to climb back past its own high-water mark.
        let baseline = last.saturating_sub(reclaimed_since);
        if used.saturating_sub(baseline) < base_delta.saturating_mul(backoff as u64) { return; }

        // Less than one growth-gate's worth reclaimed means the last collection
        // did not buy us room; back off so an over-budget live set stops
        // re-collecting on every gate. With no cycle behind us there is nothing
        // to judge — stay neutral rather than reading the absent collection as
        // a futile one.
        let next_backoff = if cycles == 0 {
            1
        } else if reclaimed_since < base_delta {
            backoff.saturating_mul(2).min(MAX_BACKOFF)
        } else {
            1
        };

        // A collect this path already asked for may still be pending at the
        // safepoint. Re-tripping would overwrite the watermarks with readings
        // taken before it ran, losing that cycle from the accounting — and
        // would not buy a second collection anyway, the flag is already set.
        let pending = self.external_needs_collect.lock().clone();
        if let Some(f) = &pending {
            if f.load(Ordering::Acquire) { return; }
        }

        {
            // Mark the pre-collect watermarks so we don't re-trip on every
            // subsequent alloc while the deferred collect is still pending.
            let mut i = self.inner.lock();
            i.last_auto_collect_used = used;
            i.last_auto_collect_reclaimed = total_reclaimed;
            i.auto_collect_backoff = next_backoff;
        }

        // Defer to safepoint when wired (multi-thread safe path).
        if let Some(flag) = pending {
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
