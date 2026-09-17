//! **Pause-budgeted nursery** — add-pause-budget-nursery (2026-09-17).
//!
//! # Why
//!
//! After `add-incremental-major-gc`, a major's pause no longer grows with the heap (its slices
//! are bounded by `Z42_GC_SLICE_MS`). Every remaining long pause is a **minor**, and the nursery
//! is what decides how much young set one has to chew through — but it was a **constant**
//! (`Z42_GC_NURSERY_BYTES`, default 16 MB), and no constant is right for every workload: what it
//! costs to collect a nursery depends on the survival rate and the object sizes of the program
//! running. This module measures that cost and sizes the nursery from it.
//!
//! # The model
//!
//! ```text
//!  every minor:  cost   = pause_us / entries scanned       ← decaying max, not a mean
//!                bpe    = bytes allocated / entries created  ← EWMA (a size, so it is stable)
//!                budget = target_us / cost                 ← entries one pause may scan
//!                want   = (budget - survivors) * bpe       ← only the headroom may be allocated
//!                nursery = clamp(want, cur/2, cur + cur/8), then clamp(MIN, MAX)
//! ```
//!
//! **Entries, not bytes.** A minor scans the whole young list, and an entry stays listed until it
//! has survived `promotion_age` minors — so it scans `new entries + the survivors of the previous
//! ones`. Measured on `13_gc_large_heap`, a minor gated at 4 MB of growth still marked **751 778**
//! entries, because that is what the young list held. A model denominated in allocated bytes
//! cannot see those survivors and prices them at zero, which is exactly how a 4 MB nursery still
//! produced a 36 ms pause. So: measure per entry, budget entries, and turn the **headroom**
//! (`budget − survivors already listed`) back into bytes with the measured object size.
//!
//! # What the nursery cannot buy
//!
//! Not all of a minor is proportional to the young set, and the part that is not sets a floor the
//! nursery cannot reach under. Two measured contributors, both visible in `Z42_GC_PHASES` output
//! (`minor roots` / `card seed` / `minor bfs`):
//!
//! - **The card scan is O(old generation).** `z42c.semantics`'s worst minor seeds from **280 382**
//!   old entries to find its young roots.
//! - **An open major's grey queue is a minor root.** On `13_gc_large_heap --large` that is
//!   **108 142** old entries re-traced by *every* minor while the cycle is open.
//!
//! This is why the model stops where it does rather than converging on the target: measured
//! against `Z42_GC_PAUSE_TARGET_MS=10`, `z42c.semantics` lands at **15.1 ms** (from 23.5) and the
//! synthetics at ~25 ms. Shrinking past that point only multiplies the fixed part by running more
//! minors — on `13_gc_large_heap --large`, forcing the nursery to [`MIN_NURSERY`] took total pause
//! time **up** 224%. Reaching 10 ms needs the minor itself sliced, which is a separate change.
//!
//! **The fixed part is also what makes the model self-limiting.** Feeding it the *whole* pause
//! means that as the nursery shrinks the measured cost per entry rises, so it stops on its own at
//! the point where shrinking further buys no pause. [`MIN_NURSERY`] is the floor under that,
//! chosen to also bound premature promotion (see the module note in `gc-tuning.md`).
//!
//! # What this module does **not** decide
//!
//! - **How large the heap may grow.** That is `auto_collect::collection_allowance`, denominated in
//!   the *configured* nursery (`allowance_unit`), which deliberately does not move with the
//!   adaptive one — see the note there for the 4 → 29 major cycles that cost.
//! - **Whether the futility backoff may grow the young set.** That is D4, `Z42_GC_BACKOFF_CAP`,
//!   **off** by default — see [`super::ArcMagrGC::pause_budget_cap`].

use std::sync::atomic::{AtomicU64, Ordering};

/// Smallest nursery the budget may ask for. **4 MB** (the project owner's call, 2026-09-17):
/// a smaller nursery halves how much allocation an object must outlive to be promoted, and
/// `retune-gc-nursery-and-promotion-age` measured what premature promotion does to the footprint
/// (16M at promotion age 2: peak RSS **405 MB** against 198 MB at 32M).
pub(super) const MIN_NURSERY: u64 = 4 * 1024 * 1024;

/// Largest nursery the budget may ask for — the historical default.
pub(super) const MAX_NURSERY: u64 = 64 * 1024 * 1024;

/// EWMA weight for the size reading: `new = (3 * old + sample) / 4`.
const EWMA_NUM: u64 = 3;
const EWMA_DEN: u64 = 4;

/// Decay applied to the **cost** reading when a minor comes in cheaper than the running figure:
/// `new = max(sample, old - old / DECAY_SHIFT_DEN)`. See [`PauseBudget::observe`].
const COST_DECAY_DEN: u64 = 16;

/// The pause-budget state. Lives on `ArcMagrGC`; only a minor's tail writes it.
///
/// # Why the model counts **entries**, not bytes
///
/// A minor's cost is what it *scans*, and the young list holds more than the last nursery's worth
/// of allocation: an entry stays young until it has survived `promotion_age` minors, so a minor
/// scans roughly `new entries + the survivors of the previous ones`. Measured on
/// `13_gc_large_heap`, a minor gated at 4 MB of growth still marked **751 778** entries, because
/// that is what the young list held. A model denominated in allocated bytes cannot see those
/// survivors, and it prices them at zero — which is exactly how a 4 MB nursery still produced a
/// 36 ms pause.
///
/// So: measure `ns per entry scanned`, budget `entries per minor`, and turn the **headroom**
/// (`budget − survivors already listed`) into the bytes the mutator may still allocate.
#[derive(Debug)]
pub(crate) struct PauseBudget {
    /// Target maximum minor pause, in microseconds. `0` = adaptation off (fixed nursery).
    target_us: u64,
    /// EWMA of nanoseconds per young entry scanned; `0` until the first minor is observed.
    cost_ns_per_entry: AtomicU64,
    /// EWMA of bytes of allocation per newly-listed young entry (how growth turns into work).
    bytes_per_entry: AtomicU64,
    /// Young entries left listed by the previous minor — the head start the next one inherits.
    survivors: AtomicU64,
    /// `used_bytes` right after the previous observed collection.
    last_post_used: AtomicU64,
    /// Diagnostics only: the unclamped nursery the last sample asked for, and the size reading.
    last_want: AtomicU64,
    last_bpe: AtomicU64,
}

/// What one minor reported.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MinorSample {
    pub(crate) pause_us: u64,
    /// Young entries listed when the minor started — what it had to scan.
    pub(crate) scanned: u64,
    /// Young entries still listed after it — what the next minor inherits.
    pub(crate) survivors: u64,
    pub(crate) used_before: u64,
    pub(crate) used_after: u64,
}

impl PauseBudget {
    pub(crate) fn new(target_us: u64) -> Self {
        PauseBudget {
            target_us,
            cost_ns_per_entry: AtomicU64::new(0),
            bytes_per_entry: AtomicU64::new(0),
            survivors: AtomicU64::new(0),
            last_post_used: AtomicU64::new(0),
            last_want: AtomicU64::new(0),
            last_bpe: AtomicU64::new(0),
        }
    }

    #[inline]
    pub(crate) fn enabled(&self) -> bool {
        self.target_us != 0
    }

    /// Fold one minor into the model and return the nursery the next one should use.
    pub(crate) fn observe(&self, s: MinorSample, nursery: u64) -> Option<u64> {
        if !self.enabled() {
            return None;
        }
        let grown = s.used_before.saturating_sub(self.last_post_used.swap(s.used_after, Ordering::Relaxed));
        let prev_survivors = self.survivors.swap(s.survivors, Ordering::Relaxed);
        if s.scanned == 0 || s.pause_us == 0 || grown == 0 {
            return None;
        }
        let cost = decaying_max(&self.cost_ns_per_entry, (s.pause_us * 1000 / s.scanned).max(1));
        // Entries that appeared since the last minor, and what each of them cost in allocation.
        let fresh = s.scanned.saturating_sub(prev_survivors).max(1);
        let bpe = ewma(&self.bytes_per_entry, (grown / fresh).max(1));

        let budget_entries = (self.target_us * 1000 / cost).max(1);
        // The survivors are already on the next minor's bill; only the headroom may be allocated.
        let headroom = budget_entries.saturating_sub(s.survivors);
        let want = headroom.saturating_mul(bpe);
        self.last_want.store(want, Ordering::Relaxed);
        self.last_bpe.store(bpe, Ordering::Relaxed);
        Some(Self::next_nursery(want, nursery))
    }

    /// Asymmetric limiting: **shrink fast, grow slowly**. Overshooting upwards costs a long pause
    /// (the thing being bounded); overshooting downwards costs only throughput, which the fixed
    /// allowance unit already protects. Measured before this was asymmetric: a cheap startup phase
    /// grew the nursery 16M → 43M, and walking it back cost three 21~36 ms minors on the way down.
    pub(super) fn next_nursery(want: u64, cur: u64) -> u64 {
        let cur = cur.clamp(MIN_NURSERY, MAX_NURSERY);
        let bounded = want.clamp(cur / 2, cur + cur / 8);
        bounded.clamp(MIN_NURSERY, MAX_NURSERY)
    }

    /// The configured target, for diagnostics.
    #[inline]
    pub(crate) fn target_us_for_diag(&self) -> u64 {
        self.target_us
    }

    /// The model's current reading (ns per young entry), for diagnostics.
    #[inline]
    pub(crate) fn cost_ns_per_entry(&self) -> u64 {
        self.cost_ns_per_entry.load(Ordering::Relaxed)
    }

    /// The unclamped nursery the last sample asked for, for diagnostics.
    #[inline]
    pub(crate) fn last_want(&self) -> u64 {
        self.last_want.load(Ordering::Relaxed)
    }

    /// The model's current size reading (bytes of allocation per young entry), for diagnostics.
    #[inline]
    pub(crate) fn last_bpe(&self) -> u64 {
        self.last_bpe.load(Ordering::Relaxed)
    }
}

/// The cost reading tracks a **maximum**, not a mean: the thing being bounded is the *worst*
/// minor, and a mean lets a run of cheap minors (a small heap, warm caches) talk the model into a
/// nursery the next expensive one cannot afford. Measured on `13_gc_large_heap`: the mean read
/// 13 ns/entry through startup, grew the nursery 16M → 32.9M, and then the same workload at a
/// 300 MB heap read 25 ns/entry — the three 29~33 ms pauses of that run are exactly the ramp.
///
/// It still has to come down, or one outlier pins the nursery at the floor forever; it comes down
/// by 1/16 per minor, which is slow enough that the walk down is not itself a long pause.
fn decaying_max(cell: &AtomicU64, sample: u64) -> u64 {
    let prev = cell.load(Ordering::Relaxed);
    let next = sample.max(prev - prev / COST_DECAY_DEN);
    cell.store(next, Ordering::Relaxed);
    next
}

fn ewma(cell: &AtomicU64, sample: u64) -> u64 {
    let prev = cell.load(Ordering::Relaxed);
    let next = if prev == 0 { sample } else { (EWMA_NUM * prev + sample) / EWMA_DEN };
    cell.store(next, Ordering::Relaxed);
    next
}

impl crate::gc::arc_heap::ArcMagrGC {
    /// Fold one minor's pause into the cost model and put the nursery it buys in force.
    pub(super) fn observe_minor_pause(&self, sample: MinorSample) {
        let cur = self.nursery_bytes.load(Ordering::Relaxed);
        let Some(next) = self.pause_budget.observe(sample, cur) else {
            return;
        };
        if next != cur {
            self.nursery_bytes.store(next, Ordering::Relaxed);
            self.rearm_auto_collect();
            crate::gc::phase_timer::note(format_args!(
                "nursery  {} -> {}  (want {}, {} ns/entry, {} B/entry, scanned {}, survivors {}, \
                 target {} us, promotion age {})",
                crate::gc::trace::human(cur), crate::gc::trace::human(next),
                crate::gc::trace::human(self.pause_budget.last_want()),
                self.pause_budget.cost_ns_per_entry(), self.pause_budget.last_bpe(),
                sample.scanned, sample.survivors,
                self.pause_budget.target_us_for_diag(), self.promotion_age()));
        }
    }

    /// The nursery a minor may grow to — the cap the futility backoff may not exceed when
    /// `Z42_GC_BACKOFF_CAP` is on. `None` (the default) leaves the backoff's gate multiplier
    /// alone; see the decision note at its one call site in `auto_collect::decide_trip`.
    ///
    /// Deliberately **not** tied to whether the pause budget is adapting: with the adaptation off
    /// the cap is the configured nursery, which is the plain reading of "one minor scans at most
    /// one nursery".
    #[inline]
    pub(super) fn pause_budget_cap(&self, cfg: &crate::config::RuntimeConfig) -> Option<u64> {
        cfg.gc_backoff_cap.then(|| self.nursery_bytes.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
#[path = "pause_budget_tests.rs"]
mod pause_budget_tests;
