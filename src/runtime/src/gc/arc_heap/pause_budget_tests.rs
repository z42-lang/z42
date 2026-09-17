//! add-pause-budget-nursery: the cost model is a pure function of what a minor reports, so these
//! drive it directly — no heap, no collection.

use super::*;

const MIB: u64 = 1024 * 1024;

/// One minor: `scanned` young entries cost `pause_us`, `survivors` stay listed, and `grown` bytes
/// were allocated since the previous one.
fn sample(pause_us: u64, scanned: u64, survivors: u64, grown: u64, used: &mut u64) -> MinorSample {
    let used_before = *used + grown;
    let s = MinorSample { pause_us, scanned, survivors, used_before, used_after: *used };
    s
}

#[test]
fn it_converges_on_the_nursery_the_budget_buys() {
    // 60 ns an entry (the measured shape), ~48 B of allocation per entry, no survivors:
    // a 10 ms budget buys ~166 k entries ≈ 8 MiB.
    let b = PauseBudget::new(10_000);
    let mut used = 0;
    let mut nursery = 16 * MIB;
    for _ in 0..24 {
        let entries = nursery / 48;
        nursery = b.observe(sample(entries * 60 / 1000, entries, 0, nursery, &mut used), nursery).expect("adaptive");
    }
    assert!((7 * MIB..=9 * MIB).contains(&nursery), "expected ~8 MiB, got {nursery}");
}

#[test]
fn survivors_already_on_the_bill_shrink_the_headroom() {
    // Same cost per entry, but 150 k entries are already listed against a 166 k budget: only the
    // remaining ~16 k may be allocated, so the nursery must come down hard.
    let b = PauseBudget::new(10_000);
    let mut used = 0;
    let mut nursery = 16 * MIB;
    for _ in 0..10 {
        let entries = 150_000 + nursery / 48;
        nursery = b.observe(sample(entries * 60 / 1000, entries, 150_000, nursery, &mut used), nursery)
            .expect("adaptive");
    }
    assert_eq!(nursery, MIN_NURSERY, "a young list that big leaves no headroom");
}

#[test]
fn it_grows_slowly_and_shrinks_fast() {
    assert_eq!(PauseBudget::next_nursery(64 * MIB, 16 * MIB), 18 * MIB, "grow at most +1/8");
    assert_eq!(PauseBudget::next_nursery(MIN_NURSERY, 16 * MIB), 8 * MIB, "shrink at most -1/2");
}

#[test]
fn a_cheap_young_set_stops_at_the_ceiling_and_an_expensive_one_at_the_floor() {
    assert_eq!(PauseBudget::next_nursery(u64::MAX, MAX_NURSERY), MAX_NURSERY);
    assert_eq!(PauseBudget::next_nursery(0, MIN_NURSERY), MIN_NURSERY);
}

#[test]
fn a_zero_target_leaves_the_nursery_alone() {
    let b = PauseBudget::new(0);
    let mut used = 0;
    assert!(!b.enabled());
    assert_eq!(b.observe(sample(50_000, 500_000, 0, 16 * MIB, &mut used), 16 * MIB), None);
}

#[test]
fn a_collection_that_scanned_nothing_teaches_nothing() {
    let b = PauseBudget::new(10_000);
    let mut used = 0;
    assert_eq!(b.observe(sample(5_000, 0, 0, 16 * MIB, &mut used), 16 * MIB), None);
    assert_eq!(b.cost_ns_per_entry(), 0, "the model must not learn from it");
}
