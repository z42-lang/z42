use super::isa_cache::IsaCache;
use crate::metadata::TypeDesc;

fn td(n: usize) -> *const TypeDesc {
    // Distinct, 16-byte-aligned fake addresses — the cache only ever compares them.
    (0x1000 + n * 64) as *const TypeDesc
}

#[test]
fn miss_then_hit_and_verdict_roundtrip() {
    let c = IsaCache::new();
    let target = String::from("Std.Exception");
    assert_eq!(c.get(td(1), &target), None);
    c.put(td(1), &target, true);
    assert_eq!(c.get(td(1), &target), Some(true));
    c.put(td(1), &target, false);
    assert_eq!(c.get(td(1), &target), Some(false));
}

#[test]
fn keyed_on_identity_not_content() {
    let c = IsaCache::new();
    let a = String::from("Std.Object");
    let b = String::from("Std.Object"); // equal content, different address
    c.put(td(2), &a, true);
    assert_eq!(c.get(td(2), &a), Some(true));
    assert_eq!(c.get(td(2), &b), None, "a different string address must not hit");
    assert_eq!(c.get(td(3), &a), None, "a different TypeDesc must not hit");
}

#[test]
fn collision_overwrites_without_false_hit() {
    let c = IsaCache::new();
    let t = String::from("T");
    // Find two TypeDesc addresses that map to the same slot for this target.
    let base = td(10);
    let tgt = t.as_ptr() as usize;
    let i0 = IsaCache::index(base as usize, tgt);
    let other = (11..100_000usize).map(td).find(|p| IsaCache::index(*p as usize, tgt) == i0)
        .expect("some address collides within the probe range");
    c.put(base, &t, true);
    c.put(other, &t, false);
    assert_eq!(c.get(other, &t), Some(false));
    assert_eq!(c.get(base, &t), None, "evicted entry must miss, never answer with the other's verdict");
}

#[test]
fn clear_forgets_everything() {
    let c = IsaCache::new();
    let t = String::from("X");
    c.put(td(4), &t, true);
    c.clear();
    assert_eq!(c.get(td(4), &t), None);
}
