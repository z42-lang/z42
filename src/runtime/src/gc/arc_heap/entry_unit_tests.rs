//! Entry-capped gate units: the size reading, the cap, the probe, and a heap whose small objects
//! trip the default gates at the same object count large ones would.

use super::*;

const MIB: u64 = 1024 * 1024;

#[test]
fn unmeasured_gates_stay_bytes_but_are_consulted_at_the_probe() {
    let u = EntryUnit::new(true);
    assert_eq!(u.cap(16 * MIB, UNIT_ENTRIES), 16 * MIB, "nothing measured: the byte gate");
    assert_eq!(u.probe(16 * MIB), UNIT_ENTRIES * MIN_ENTRY_BYTES, "consult early enough to measure");
    assert_eq!(u.probe(MIB), MIB, "a gate already below the probe is left alone");
}

#[test]
fn objects_of_64_bytes_or_more_keep_the_byte_gates() {
    let u = EntryUnit::new(true);
    u.sample(64 * 10_000, 10_000);
    assert_eq!(u.cap(16 * MIB, UNIT_ENTRIES), 16 * MIB);
    let big = EntryUnit::new(true);
    big.sample(400 * 10_000, 10_000);
    assert_eq!(big.cap(16 * MIB, UNIT_ENTRIES), 16 * MIB, "the byte cap binds for large objects");
}

#[test]
fn smaller_objects_get_gates_equal_in_objects() {
    let u = EntryUnit::new(true);
    u.sample(24 * 10_000, 10_000);
    assert_eq!(u.cap(16 * MIB, UNIT_ENTRIES), UNIT_ENTRIES * 24, "24 B objects: 6 MiB");
    assert_eq!(u.probe(16 * MIB), 16 * MIB, "measured: no probe");
}

#[test]
fn a_small_sample_is_not_read_and_the_reading_is_smoothed() {
    let u = EntryUnit::new(true);
    u.sample(1 << 20, 10);
    assert_eq!(u.reading(), 0, "a handful of allocations is not a size");
    u.sample(32 * 8192, 8192);
    assert_eq!(u.reading(), 32);
    u.sample(32 * 8192 + 64 * 8192, 2 * 8192);
    assert_eq!(u.reading(), (3 * 32 + 64) / 4, "EWMA, not the last sample");
}

#[test]
fn a_stated_byte_nursery_turns_the_cap_off() {
    let u = EntryUnit::new(true);
    u.sample(16 * 10_000, 10_000);
    u.disable();
    assert_eq!(u.cap(16 * MIB, UNIT_ENTRIES), 16 * MIB);
    assert_eq!(u.probe(16 * MIB), 16 * MIB);
    assert_eq!(u.reading(), 0);
}

#[test]
fn the_heap_unit_follows_the_allocation_size() {
    use crate::gc::MagrGC;
    let heap = crate::gc::ArcMagrGC::new();
    if heap.allowance_unit_bytes.load(std::sync::atomic::Ordering::Relaxed) != 16 * MIB {
        return; // a configured nursery (env) turns the cap off; nothing to check here
    }
    // Small strings: far fewer bytes per allocation than 64.
    let mut keep = Vec::new();
    for i in 0..20_000u32 {
        let s = heap.alloc_str(if i % 2 == 0 { "a" } else { "bc" });
        if i % 1000 == 0 {
            keep.push(s);
        }
    }
    let used = heap.used_bytes_atomic();
    let reclaimed = heap.stats().reclaimed_bytes;
    heap.sample_entry_unit(used, reclaimed);
    let bpe = heap.entry_unit.reading();
    assert!(bpe > 0 && bpe < 64, "small strings measure under 64 B, got {bpe}");
    assert_eq!(heap.allowance_unit(), UNIT_ENTRIES * bpe, "unit = UNIT_ENTRIES allocations");
    assert_eq!(heap.nursery_bytes.load(std::sync::atomic::Ordering::Relaxed), heap.allowance_unit(),
        "before the pause budget sizes it, the nursery is the unit");
    let b = heap.nursery_bounds();
    assert_eq!((b.min, b.max), (UNIT_ENTRIES / 4 * bpe, UNIT_ENTRIES * 4 * bpe));
    drop(keep);
}
