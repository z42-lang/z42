use super::*;
use crate::object::string::vstr::Str;

#[test]
fn ascii_char_len_and_char_at() {
    let s: Str = Str::from("hello");
    assert_eq!(char_len(&s), 5);
    assert_eq!(char_at(&s, 0), Some('h'));
    assert_eq!(char_at(&s, 4), Some('o'));
    assert_eq!(char_at(&s, 5), None);
}

#[test]
fn non_ascii_char_len_and_char_at() {
    // "a你b好c" — mixed ASCII + multibyte (你/好 are 3 bytes each in UTF-8).
    let s: Str = Str::from("a你b好c");
    assert_eq!(char_len(&s), 5);
    assert_eq!(char_at(&s, 0), Some('a'));
    assert_eq!(char_at(&s, 1), Some('你'));
    assert_eq!(char_at(&s, 2), Some('b'));
    assert_eq!(char_at(&s, 3), Some('好'));
    assert_eq!(char_at(&s, 4), Some('c'));
    assert_eq!(char_at(&s, 5), None);
}

#[test]
fn empty_string() {
    let s: Str = Str::from("");
    assert_eq!(char_len(&s), 0);
    assert_eq!(char_at(&s, 0), None);
}

#[test]
fn cache_hit_same_arc_is_consistent() {
    let s: Str = Str::from("café");   // é = 2 bytes
    assert_eq!(char_len(&s), 4);
    // Repeated queries (cache hits) return identical results.
    for _ in 0..3 {
        assert_eq!(char_at(&s, 3), Some('é'));
        assert_eq!(char_len(&s), 4);
    }
}

#[test]
fn distinct_strings_independent() {
    let a: Str = Str::from("abc");
    let b: Str = Str::from("日本語");
    assert_eq!(char_len(&a), 3);
    assert_eq!(char_len(&b), 3);
    assert_eq!(char_at(&a, 1), Some('b'));
    assert_eq!(char_at(&b, 1), Some('本'));
}

#[test]
fn eviction_recomputes_correctly() {
    // Fill past CACHE_CAP, then re-query an early string — must still be right.
    let strings: Vec<Str> = (0..(CACHE_CAP + 3))
        .map(|i| Str::from(format!("s{i}-ünïcode")))
        .collect();
    for s in &strings {
        assert_eq!(char_len(s), char_len(s)); // populate
    }
    // The first string was likely evicted; querying recomputes the same value.
    let expected = strings[0].chars().count();
    assert_eq!(char_len(&strings[0]), expected);
    assert_eq!(char_at(&strings[0], 2), strings[0].chars().nth(2));
}

// fix-wasm-string-ops regression: the cache is `thread_local` and outlives any single
// heap, so a torn-down heap's block address can be recycled by the next heap at the same
// address with generation 0 — a stale gen-0 entry would then false-hit (`is_live` only
// distinguishes reuse WITHIN one region). This is the wasm32 string-corruption bug
// (`"".Length == 13`). The per-heap monotonic epoch drops the cache on a heap switch, so
// a recycled address in a new heap can never return the old heap's metadata.
#[test]
fn cross_heap_recycled_address_no_false_hit() {
    use crate::gc::ambient::HeapGuard;
    use crate::gc::arc_heap::ArcMagrGC;
    use crate::gc::heap::MagrGC;

    // Heap 1: cache a long (13-char) string's metadata, then tear the heap down so its
    // VarRegion chunk memory is freed back to the system allocator.
    {
        let h1 = ArcMagrGC::new();
        let _g = HeapGuard::enter(&h1);
        let s = h1.alloc_str("ABCDEFGHIJKLM"); // 13 chars
        assert_eq!(char_len(&s), 13);
    } // h1 dropped: chunk freed, cache entry now dangles at a recyclable address.

    // Heap 2: a fresh heap (new epoch) whose first allocation is very likely to reuse
    // heap 1's just-freed chunk address at generation 0. Before the epoch fix, the stale
    // gen-0 entry from heap 1 would false-hit → char_len 13 instead of 2. The epoch guard
    // drops the cache on the heap switch, so the query recomputes for the real string.
    let h2 = ArcMagrGC::new();
    let _g = HeapGuard::enter(&h2);
    let s = h2.alloc_str("XY"); // 2 chars
    assert_eq!(char_len(&s), 2, "cross-heap false-hit: returned heap-1's cached length");
    assert_eq!(char_at(&s, 0), Some('X'));
    assert_eq!(char_at(&s, 1), Some('Y'));
    assert_eq!(char_at(&s, 2), None);
}

#[test]
fn ascii_bit_is_set_only_for_ascii_strings() {
    assert!(Str::from("hello").is_known_ascii());
    assert!(Str::from("").is_known_ascii());
    assert!(!Str::from("a你b").is_known_ascii());
    // The flag path and the cache path agree on ranges.
    let s = Str::from("hello");
    assert_eq!(byte_range(&s, 1, 3), Some((1, 4)));
    assert_eq!(byte_range(&s, 3, 3), None);
}

// D7 regression (runtime audit): an oversized string (> 64 KB payload) lives alone in a
// dedicated chunk, and when it dies the collection hands that chunk straight back to the
// allocator (`VarRegion::reclaim_dead_var_chunks`). The next block malloc places at the same
// address — another oversized string, or the first block of a fresh bump chunk — is born at
// `FIRST_GEN`, the very generation the dead string had, so a cache entry left behind by the
// dead string matched it on both the address and `is_live` and returned the dead string's
// char count / offsets. Same byte size here, so the allocator typically hands the freed chunk
// straight back (it does on macOS and glibc); the next test checks the mechanism without
// relying on that.
#[test]
fn freed_dedicated_chunk_reused_address_no_false_hit() {
    use crate::gc::ambient::HeapGuard;
    use crate::gc::arc_heap::ArcMagrGC;
    use crate::gc::heap::MagrGC;

    let h = ArcMagrGC::new();
    let _g = HeapGuard::enter(&h);
    // 80 000 bytes each (well past a 64 KB bump chunk ⇒ a dedicated chunk), non-ASCII so
    // both go through the cache, with different char counts.
    let dead_text = "é".repeat(40_000);
    let live_text = format!("{}é", "a".repeat(79_998));
    for _ in 0..8 {
        let dead = h.alloc_str(&dead_text);
        assert_eq!(char_len(&dead), 40_000);
        // `dead` is no root: the collection tombstones it and frees its dedicated chunk.
        h.force_collect();
        let live = h.alloc_str(&live_text);
        assert_eq!(char_len(&live), 79_999, "false hit on a freed dedicated chunk's stale entry");
        assert_eq!(char_at(&live, 0), Some('a'));
        assert_eq!(char_at(&live, 79_998), Some('é'));
        assert_eq!(byte_range(&live, 79_998, 1), Some((79_998, 80_000)));
        h.force_collect();
    }
}

// D7, independent of where malloc puts the next block: freeing a dedicated chunk drops the
// entry stamped with that chunk (without reading its header), and only that one — a live
// oversized string's entry and a bump-chunk string's entry survive and keep hitting.
#[test]
fn freed_dedicated_chunk_drops_only_its_own_entry() {
    use crate::gc::ambient::HeapGuard;
    use crate::gc::arc_heap::ArcMagrGC;
    use crate::gc::heap::MagrGC;
    use crate::object::Value;

    let cached = || CACHE.with(|c| c.borrow().iter().map(|e| (data_ptr(&e.s), e.chunk.is_some())).collect::<Vec<_>>());
    let h = ArcMagrGC::new();
    let _g = HeapGuard::enter(&h);
    let kept = h.alloc_str(&"ü".repeat(40_000)); // 80 000 B ⇒ dedicated chunk, rooted
    let small = h.alloc_str("naïve"); // bump chunk, rooted
    let _r1 = h.pin_root(Value::Str(kept.clone()));
    let _r2 = h.pin_root(Value::Str(small.clone()));
    let doomed = h.alloc_str(&"é".repeat(40_000)); // dedicated chunk, dies
    let doomed_addr = doomed.as_ptr();
    assert_eq!(char_len(&kept), 40_000);
    assert_eq!(char_len(&small), 5);
    assert_eq!(char_len(&doomed), 40_000);
    let before = cached();
    assert!(before.contains(&(kept.as_ptr(), true)), "an oversized string's entry is stamped");
    assert!(before.contains(&(small.as_ptr(), false)), "a bump-chunk string's entry is not");
    assert!(before.contains(&(doomed_addr, true)));

    h.force_collect(); // frees `doomed`'s dedicated chunk
    assert_eq!(char_len(&small), 5);
    let after = cached();
    assert!(!after.iter().any(|&(p, _)| p == doomed_addr), "the freed chunk's entry is dropped");
    assert!(after.contains(&(kept.as_ptr(), true)), "a live dedicated chunk's entry survives");
    assert!(after.contains(&(small.as_ptr(), false)));
    assert_eq!(char_len(&kept), 40_000);
    assert_eq!(char_at(&kept, 39_999), Some('ü'));
}
