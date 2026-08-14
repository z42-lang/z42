//! Per-string character-metadata cache (perf-str-char-index, 2026-06-20).
//!
//! `Std.String.Length` (Unicode scalar count) and `CharAt(i)` are O(n) / O(i)
//! on a UTF-8 string — counting / walking chars from the start. The z42c lexer
//! scans its source with `while (pos < src.Length) { ... src.CharAt(pos) ... }`,
//! which turns lexing into **O(n²)** (a `Length` and a `CharAt` per character).
//! We can't change the (byte-identical) compiler source, so the VM must make
//! these O(1).
//!
//! Design: a small thread-local cache keyed by `Arc<str>` **identity** (data
//! pointer). For each distinct string we compute, once:
//!   - `char_len`  — the Unicode scalar count (what `Length` returns);
//!   - `ascii`     — whether every byte is < 0x80 (then char index == byte index);
//!   - `offsets`   — for non-ASCII strings, the byte offset of each char, so
//!                   `CharAt(i)` is an O(1) table lookup instead of O(i).
//!
//! Soundness: each cache entry *holds an `Arc<str>` clone*, so while the entry
//! exists the string stays alive and its address cannot be reused by a
//! different allocation — pointer identity is therefore a valid key. On
//! eviction we drop our clone; a later allocation reusing the address has no
//! cache entry (we removed it), so it recomputes. Immutable strings + identity
//! key ⇒ the cached metadata always matches the queried string's content.
//!
//! The cache only changes *speed*, never the returned value — byte-identical
//! output is preserved.

use std::cell::RefCell;
use crate::metadata::vstr::Str;

struct StrMeta {
    /// Keeps the string alive (so its identity pointer stays a valid key) and provides
    /// the bytes for `CharAt`. unify-object-byte-layout PR-4: `Str` (8B thin) not `Arc<str>`.
    s:        Str,
    char_len: usize,
    ascii:    bool,
    /// `Some` only for non-ASCII strings: `offsets[i]` is the byte offset of
    /// the i-th char. ASCII strings index bytes directly (offset == index).
    offsets:  Option<Box<[u32]>>,
}

// Small N-way cache. The lexer hammers a single source string (→ ~100% hit on
// one slot); a handful of slots covers brief interleaving with other strings
// (identifiers, etc.) without unbounded memory. LRU via move-to-front.
const CACHE_CAP: usize = 8;

thread_local! {
    static CACHE: RefCell<Vec<StrMeta>> = const { RefCell::new(Vec::new()) };
}

#[inline]
fn data_ptr(s: &Str) -> *const u8 {
    s.as_ptr()
}

fn compute(s: &Str) -> StrMeta {
    let bytes = s.as_bytes();
    if bytes.is_ascii() {
        StrMeta { s: s.clone(), char_len: bytes.len(), ascii: true, offsets: None }
    } else {
        let offsets: Vec<u32> = s.char_indices().map(|(b, _)| b as u32).collect();
        let char_len = offsets.len();
        StrMeta { s: s.clone(), char_len, ascii: false, offsets: Some(offsets.into_boxed_slice()) }
    }
}

/// Run `f` against `s`'s cached metadata, computing + caching on first sight.
fn with_meta<R>(s: &Str, f: impl FnOnce(&StrMeta) -> R) -> R {
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let key = data_ptr(s);
        if let Some(pos) = cache.iter().position(|e| data_ptr(&e.s) == key) {
            if pos != 0 {
                let e = cache.remove(pos);
                cache.insert(0, e);
            }
            return f(&cache[0]);
        }
        let meta = compute(s);
        cache.insert(0, meta);
        if cache.len() > CACHE_CAP {
            cache.pop();
        }
        f(&cache[0])
    })
}

/// O(1) (amortised) Unicode scalar count — backs `Std.String.Length`.
pub fn char_len(s: &Str) -> usize {
    with_meta(s, |m| m.char_len)
}

/// O(1) (amortised) char at scalar index `i` — backs `Std.String.CharAt`.
/// Returns `None` when `i >= char_len`.
pub fn char_at(s: &Str, i: usize) -> Option<char> {
    with_meta(s, |m| {
        if m.ascii {
            m.s.as_bytes().get(i).map(|&b| b as char)
        } else {
            let offs = m.offsets.as_ref().expect("non-ascii entry has offsets");
            let start = *offs.get(i)? as usize;
            // SAFETY of indexing: `start` is a char boundary by construction.
            m.s[start..].chars().next()
        }
    })
}

#[cfg(test)]
#[path = "str_meta_tests.rs"]
mod str_meta_tests;
