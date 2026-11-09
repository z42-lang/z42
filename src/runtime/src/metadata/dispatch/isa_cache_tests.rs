use super::isa_cache::{pair_key, IsaCache};

#[test]
fn miss_then_hit_and_verdict_roundtrip() {
    let c = IsaCache::new();
    assert_eq!(c.get(1, 7), None);
    c.put(1, 7, true);
    assert_eq!(c.get(1, 7), Some(true));
    c.put(1, 7, false);
    assert_eq!(c.get(1, 7), Some(false));
}

#[test]
fn keyed_on_both_ids() {
    let c = IsaCache::new();
    c.put(2, 9, true);
    assert_eq!(c.get(2, 9), Some(true));
    assert_eq!(c.get(2, 10), None, "a different target key must not hit");
    assert_eq!(c.get(3, 9), None, "a different receiver id must not hit");
    assert_eq!(c.get(9, 2), None, "the pair is ordered");
}

#[test]
fn id_zero_is_a_real_key_not_the_empty_slot() {
    let c = IsaCache::new();
    c.put(5, 5, false); // allocate the slots
    assert_eq!(c.get(0, 0), None, "an empty slot never answers");
    c.put(0, 0, true);
    assert_eq!(c.get(0, 0), Some(true));
}

#[test]
fn collision_overwrites_without_false_hit() {
    let c = IsaCache::new();
    let i0 = IsaCache::index(pair_key(10, 3));
    let other = (11..1_000_000u32).find(|r| IsaCache::index(pair_key(*r, 3)) == i0)
        .expect("some receiver id collides within the probe range");
    c.put(10, 3, true);
    c.put(other, 3, false);
    assert_eq!(c.get(other, 3), Some(false));
    assert_eq!(c.get(10, 3), None, "evicted entry must miss, never answer with the other's verdict");
}

#[test]
fn verdict_bit_does_not_leak_into_the_key() {
    let c = IsaCache::new();
    // `target | VERDICT_BIT` must not read back as another target's entry.
    c.put(4, 1, true);
    assert_eq!(c.get(4, 1), Some(true));
    assert_eq!(c.get(4, 1 | (1 << 30)), None);
}

#[test]
fn clear_forgets_everything() {
    let c = IsaCache::new();
    c.put(4, 8, true);
    c.clear();
    assert_eq!(c.get(4, 8), None);
}
