use super::*;
use crate::metadata::vstr::Str;

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
