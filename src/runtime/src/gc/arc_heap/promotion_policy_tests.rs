//! Unit tests for [`PromotionPolicy`](super::PromotionPolicy).

use super::*;

/// Feed `n` observations of a tier with the given survival rate, in per-mille.
fn feed(p: &PromotionPolicy, n: u64, survival_permille: u64) {
    for i in 0..n {
        p.observe(i % 1000 < survival_permille);
    }
}

/// A tier that nearly all survives reclaims nothing → drop it.
#[test]
fn a_tier_that_survives_gets_dropped() {
    let p = PromotionPolicy::default();
    assert_eq!(p.age_for_this_minor(3), 3, "starts at the configured age");
    feed(&p, 10_000, 999); // 99.9% survival — `z42c.semantics`'s shape
    p.settle();
    assert_eq!(p.age_for_this_minor(3), 2);
}

/// A tier that nearly all dies is exactly what the tier is for → never drop it.
#[test]
fn a_tier_that_dies_is_kept() {
    let p = PromotionPolicy::default();
    feed(&p, 10_000, 16); // 1.6% survival — `12_gc_churn`'s shape
    p.settle();
    assert_eq!(p.age_for_this_minor(3), 3);
}

/// The decision **latches**: once the tier is dropped it stays dropped, even if a later
/// minor sees a tier that dies. Raising the age back is not a safe operation — entries that
/// left the young list under the lower line would be re-classified young while no longer
/// being listed, so the mark phase would mark them and nothing would ever clear that mark
/// (the stale-mark failure `fix-minor-stale-mark-on-old-roots` exists for).
#[test]
fn the_decision_latches() {
    let p = PromotionPolicy::default();
    feed(&p, 10_000, 999);
    p.settle();
    assert_eq!(p.age_for_this_minor(3), 2);
    feed(&p, 10_000, 16); // a tier that dies, arriving too late to matter
    p.settle();
    assert_eq!(p.age_for_this_minor(3), 2, "the age must never go back up");
}

/// Below `MIN_SAMPLES` the rate is noise — the standing decision holds, in **both**
/// directions. (`12_gc_churn`'s array region reports 9 entries, all survivors; read
/// literally that is 100% and would wrongly drop the tier.)
#[test]
fn a_tiny_sample_does_not_move_the_decision() {
    let p = PromotionPolicy::default();
    feed(&p, 9, 1000); // nine entries, all survivors
    p.settle();
    assert_eq!(p.age_for_this_minor(3), 3, "nine entries decided anything");
}

/// The evidence is the tier below the **configured** age, never below the current one —
/// reading it off the current age is an oscillator (measured: 99.9% at one tier, 90.2% at
/// the next, so the decision flips every collection).
#[test]
fn the_observed_tier_follows_the_configured_age() {
    let p = PromotionPolicy::default();
    assert_eq!(p.observed_age(3), 2);
    feed(&p, 10_000, 999);
    p.settle();
    assert_eq!(p.age_for_this_minor(3), 2, "now sweeping one tier lower");
    assert_eq!(p.observed_age(3), 2, "but still watching the same tier");
}

/// `settle` clears the counters, or one decisive minor would keep voting forever.
#[test]
fn settling_resets_the_counters() {
    let p = PromotionPolicy::default();
    feed(&p, MIN_SAMPLES, 1000);
    p.settle();
    let (live, total, _) = p.snapshot();
    assert_eq!((live, total), (0, 0));
}

/// A configured age of 1 has no tier to drop — the policy must leave it alone rather than
/// underflow to 0 (which would mean "promote everything at birth").
#[test]
fn an_age_of_one_has_no_tier_to_drop() {
    let p = PromotionPolicy::default();
    feed(&p, 10_000, 999);
    p.settle();
    assert_eq!(p.age_for_this_minor(1), 1);
}
