//! **adaptive-promotion (2026-09-12)**: decide, per minor, whether a survivor that has
//! already lived `promotion_age - 2` collections should be promoted **now** instead of
//! spending one more collection in the young generation.
//!
//! ## Why there is a decision to make at all
//!
//! `promotion_age` is a single global number (3, and 3 is also the ceiling — `gen_age` has
//! two bits). Measured, the same number is right for one workload and wrong for another:
//!
//! | | 停顿 age3 → age2 | 峰值 RSS age3 → age2 |
//! |---|---|---|
//! | `z42c.semantics` | **−12.9%** | −0.2% |
//! | `09_alloc_ctorless` | **−12%** | +3.2% |
//! | `12_gc_churn` | +6% | **+182%** (142 → 401 MB) |
//!
//! The last row is why the age is 3 (#575): churn's objects die in their third collection,
//! and promoting them a tier early strands them in the old generation where only a major
//! can reach them. The first two rows are why 3 is expensive everywhere else: their
//! survivors are *never* going to die young, so the extra tier is a full re-mark of the
//! live set that reclaims nothing.
//!
//! ## The signal that separates them
//!
//! The survival rate of the **`promotion_age - 1` age bucket** — "of the objects that have
//! already survived two collections, how many survive the third":
//!
//! | 负载 | age0 | age1 | **age2** |
//! |---|---|---|---|
//! | `z42c.semantics` | 43.3% | 90.2% | **99.9%** |
//! | `09_alloc_ctorless` | 100% | 100% | **100%** |
//! | `12_gc_churn` | 23.0% | 82.6% | **1.5%** |
//!
//! 66× apart, and it says exactly the right thing: when that bucket nearly all survives,
//! the tier buys nothing and costs a mark; when it nearly all dies, the tier is what keeps
//! the old generation small. Note the neighbouring bucket does **not** separate them
//! (90.2% vs 82.6%) — it has to be this one.
//!
//! ## Why a control group rather than a plain feedback loop
//!
//! Acting on the signal destroys it: skip the tier and the `promotion_age - 1` bucket is
//! empty, so the next decision has nothing to read. A fixed fraction of survivors
//! (`CONTROL_EVERY`) therefore always takes the slow path, which keeps the bucket populated
//! with an unbiased sample at a cost of 1/16 of the tier.
//!
//! Only the object and array regions skip; the variable-length region has no card table, so
//! a var block must never get older than whatever points at it. Its blocks still follow —
//! an array's element storage is raised to its owner's age by `age_backing_with_owner`,
//! which is the existing (and load-bearing, see the pause-line notes) mechanism for exactly
//! this invariant.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Survival fraction of the observed tier above which it is judged worthless. Deliberately
/// close to 1: the tier's whole value is the objects it lets die, so anything short of
/// "almost nothing dies here" means keep it.
const SURVIVAL_CUTOFF: f64 = 0.95;

/// Minimum tier population before the rate is read at all. Below this the decision holds —
/// a handful of entries is noise, not evidence. (Measured: the array region of
/// `12_gc_churn` puts **9** entries in this tier over a whole run, while its object region
/// puts 532 940 there.)
const MIN_SAMPLES: u64 = 4096;

/// Per-heap state for [the module's](self) decision. Every read happens inside the minor
/// sweep (STW), so `Relaxed` is enough everywhere.
#[derive(Debug, Default)]
pub(super) struct PromotionPolicy {
    /// Entries of the observed tier that survived the current minor.
    live: AtomicU64,
    /// Entries that were in that tier at all.
    total: AtomicU64,
    /// Whether the tier has been dropped. **Latches** — see the module note on why raising
    /// the age back is not a safe operation.
    lowered: AtomicBool,
}

impl PromotionPolicy {
    /// The age this minor should sweep at, given the configured one.
    pub(super) fn age_for_this_minor(&self, configured: u8) -> u8 {
        if configured >= 2 && self.lowered.load(Ordering::Relaxed) {
            configured - 1
        } else {
            configured
        }
    }

    /// The tier whose survival rate is the evidence: **always the one below the configured
    /// age**, never below whatever this minor happens to be sweeping at.
    ///
    /// Reading it off the current age is an oscillator, and a measured one: lower the age
    /// and the evidence becomes the *next* tier down, whose rate is a different number
    /// (90.2% against 99.9% on `z42c.semantics`) that flips the decision straight back.
    pub(super) fn observed_age(&self, configured: u8) -> u8 {
        configured.saturating_sub(1)
    }

    /// Record one entry of the observed tier. `survived` is its mark bit.
    #[inline]
    pub(super) fn observe(&self, survived: bool) {
        self.total.fetch_add(1, Ordering::Relaxed);
        if survived {
            self.live.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// End of a minor: decide from what this minor saw, then reset the counters.
    ///
    /// Once lowered it stays lowered, and the counters stop mattering — that is not a
    /// simplification, it is the safety property. See the module note.
    pub(super) fn settle(&self) {
        let total = self.total.swap(0, Ordering::Relaxed);
        let live = self.live.swap(0, Ordering::Relaxed);
        if self.lowered.load(Ordering::Relaxed) || total < MIN_SAMPLES {
            return;
        }
        if live as f64 / total as f64 >= SURVIVAL_CUTOFF {
            self.lowered.store(true, Ordering::Relaxed);
        }
    }

    /// Diagnostics for `Z42_GC_PHASES`.
    pub(super) fn snapshot(&self) -> (u64, u64, bool) {
        (
            self.live.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
            self.lowered.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
#[path = "promotion_policy_tests.rs"]
mod promotion_policy_tests;
