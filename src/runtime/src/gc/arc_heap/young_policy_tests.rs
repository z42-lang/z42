//! Tests for the young-generation policy (`young_policy.rs`).

use super::*;

const MB: u64 = 1024 * 1024;

/// A minor that freed `freed` bytes in `pause_us`, with neutral entry counts (half its 1 000
/// scanned entries reclaimed, no pause budget measured).
fn minor(freed: u64, pause_us: u64) -> MinorYield {
    MinorYield { freed, pause_us, gate: 4 * MB, scanned: 1_000, reclaimed: 500, survivors: 500, survivor_budget: None }
}

#[test]
fn futility_is_also_judged_in_entries() {
    // Plenty of bytes freed (a few large blocks), but 99% of the scanned entries survived.
    let p = YoungPolicy::new(true);
    let m = MinorYield { scanned: 100_000, reclaimed: 1_000, survivors: 99_000, ..minor(2 * MB, 5_000) };
    assert_eq!(p.observe_minor(m), Verdict::Futile);
    assert!(p.take_tenure_turn());
}

#[test]
fn survivors_that_fill_the_pause_budget_buy_a_tenure() {
    // The `13_gc_large_heap` shape on a denser heap: productive by bytes and by entries, but the
    // 675 k survivors left listed exceed the 627 k entries the budget leaves them next to the
    // floor nursery's fresh entries.
    let p = YoungPolicy::new(true);
    let base = MinorYield { scanned: 780_000, reclaimed: 105_000, survivors: 675_000, ..minor(5 * MB, 11_000) };
    assert_eq!(p.observe_minor(MinorYield { survivor_budget: Some(627_000), ..base }), Verdict::OverBudget);
    assert!(p.take_tenure_turn());
    // Below the budget it keeps running real minors; with no budget measured it cannot tell.
    assert_eq!(p.observe_minor(MinorYield { survivor_budget: Some(700_000), ..base }), Verdict::Productive);
    assert_eq!(p.observe_minor(base), Verdict::Productive);
    // Survivors that are a minority of what the minor scanned do not count, however few entries
    // the budget leaves (the fixed part inflated the cost reading).
    let few = MinorYield { scanned: 100_000, reclaimed: 60_000, survivors: 40_000, ..minor(5 * MB, 11_000) };
    assert_eq!(p.observe_minor(MinorYield { survivor_budget: Some(0), ..few }), Verdict::Productive);
}

#[test]
fn a_futile_minor_buys_a_tenure_and_the_run_doubles_up_to_the_cap() {
    let p = YoungPolicy::new(true);
    assert!(!p.take_tenure_turn(), "nothing judged yet: real minors");
    assert_eq!(p.observe_minor(minor(0, 5_000)), Verdict::Futile);
    assert!(p.take_tenure_turn());
    assert!(!p.take_tenure_turn(), "one tenure, then a probe");
    let mut runs = Vec::new();
    for _ in 0..5 {
        p.observe_minor(minor(100, 5_000));
        runs.push(p.run());
    }
    assert_eq!(runs, vec![2, 4, 8, MAX_TENURE_RUN, MAX_TENURE_RUN]);
    assert_eq!((0..20).filter(|_| p.take_tenure_turn()).count(), MAX_TENURE_RUN as usize);
}

#[test]
fn a_productive_minor_ends_the_run() {
    let p = YoungPolicy::new(true);
    p.observe_minor(minor(0, 5_000));
    p.observe_minor(minor(0, 5_000));
    assert_eq!(p.observe_minor(minor(3 * MB, 5_000)), Verdict::Productive);
    assert_eq!(p.run(), 0);
    assert!(!p.take_tenure_turn());
}

/// The `13_gc_large_heap` shape: a minor that frees an eighth of its gate is not futile, but a
/// major frees ~20× more per millisecond of pause — and the `z42c` shape, within the margin.
#[test]
fn a_minor_is_judged_against_the_last_majors_yield() {
    let p = YoungPolicy::new(true);
    assert_eq!(p.observe_minor(minor(MB / 2, 13_000)), Verdict::Productive, "no major measured yet");
    // A cycle of three slices: 46 MB in 60 ms → ~0.77 MB/ms.
    p.observe_major_work(10 * MB, 20_000, false, MB);
    p.observe_major_work(16 * MB, 20_000, false, MB);
    assert_eq!(p.major_reference(), (0, 0), "an open cycle is not a reference yet");
    p.observe_major_work(20 * MB, 20_000, true, MB);
    assert_eq!(p.major_reference(), (46 * MB, 60_000));
    assert_eq!(p.observe_minor(minor(MB / 2, 13_000)), Verdict::Outyielded, "0.04 MB/ms");
    assert_eq!(p.observe_minor(minor(2 * MB, 10_000)), Verdict::Productive, "0.2 MB/ms ≥ 0.77 / 4");
}

#[test]
fn without_a_clock_only_futility_counts() {
    let p = YoungPolicy::new(false);
    p.observe_major_work(46 * MB, 60_000, true, MB);
    assert_eq!(p.observe_minor(minor(MB / 2, 13_000)), Verdict::Productive);
    assert_eq!(p.observe_minor(minor(0, 13_000)), Verdict::Futile);
}

#[test]
fn a_futile_major_refutes_escalation_until_one_frees_something() {
    let p = YoungPolicy::new(true);
    assert!(!p.major_was_futile());
    p.observe_major_work(0, 30_000, true, MB);
    assert!(p.major_was_futile());
    p.observe_major_work(5 * MB, 30_000, true, MB);
    assert!(!p.major_was_futile());
}
