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
//! Soundness (unify-gc-heap PR-4): strings are GC blocks now, so a cache entry's
//! held `Str` clone is **not** a GC root — the block can be swept (its slot bumped
//! + reused by another string) while the entry lingers. A stale entry would share
//! the reused slot's address and falsely "hit". So a hit requires **both** the
//! identity pointer match **and** [`VarGcRef::is_live`] (the cached handle's
//! generation still matching the slot): a swept-and-reused slot fails the
//! generation check ⇒ treated as a miss ⇒ recomputed for the current string.
//! Immutable strings + generation-guarded identity ⇒ the cached metadata always
//! matches the queried string's content. (Leaked test/no-heap strings have a fixed
//! generation and are never swept, so they always pass the check.)
//!
//! Cross-heap soundness (fix-wasm-string-ops): the generation guard above only
//! distinguishes slot reuse **within one heap** (the region bumps the generation at
//! every tombstone). It does **not** cover reuse **across heaps**: this cache is a
//! plain `thread_local`, so it outlives any single [`VmContext`]/heap. When a VM is
//! torn down its [`VarRegion`] returns its chunk memory to the system allocator, and
//! the next VM on the same thread can re-allocate a block at the exact same address
//! with generation **0** (a fresh bump-allocated slot). A stale gen-0 entry from the
//! dead heap would then pass both `data_ptr == key` and `is_live` — a false hit that
//! returns the WRONG string's metadata (observed on wasm32, where linear-memory
//! addresses recycle densely and deterministically: e.g. `"".Length == 13`). The fix
//! is to scope the cache to a single heap's address space via the per-heap monotonic
//! epoch ([`crate::gc::ambient::current_heap_epoch`]): entries are computed under one
//! epoch, and the whole cache is dropped the moment execution switches to a different
//! epoch. Because the epoch is never reused, a recycled address in a *new* heap can
//! never be mistaken for a live entry from the *old* one.
//!
//! The cache only changes *speed*, never the returned value — byte-identical
//! output is preserved.

use std::cell::{Cell, RefCell};
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
    /// **fix-wasm-string-ops**: the heap epoch under which `CACHE`'s entries were computed. On
    /// a mismatch with the current epoch (a heap switch — a new VM on this thread), the whole
    /// cache is dropped, because a torn-down heap's block addresses get recycled and a fresh
    /// gen-0 block at a recycled address would otherwise false-hit a stale gen-0 entry.
    static LAST_EPOCH: Cell<u64> = const { Cell::new(0) };
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
    // fix-wasm-string-ops: scope the cache to a single heap's address space. The entries are
    // keyed by GC block ADDRESS (+ generation), but a torn-down heap's addresses get recycled
    // by the next VM's allocator, and a fresh block's generation resets to 0 — so a gen-0 entry
    // from a dead heap could false-hit a gen-0 block at the same recycled address (the `is_live`
    // generation guard only distinguishes reuse WITHIN one region). The per-heap monotonic epoch
    // (`ambient::current_heap_epoch`, never reused) changes on every heap switch; drop the whole
    // cache when it does. Within one heap the epoch is constant ⇒ no clearing, no perf loss.
    let epoch = crate::gc::ambient::current_heap_epoch();
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if LAST_EPOCH.with(|le| le.replace(epoch)) != epoch {
            cache.clear();
        }
        let key = data_ptr(s);
        // Hit = same allocation address AND the cached handle is still live (its
        // block wasn't swept + its slot reused by a different string) — see the
        // module soundness note. A stale (dead-slot) entry fails `is_live` and is
        // skipped, so we recompute for the current `s`.
        if let Some(pos) = cache.iter().position(|e| data_ptr(&e.s) == key && e.s.var_ref().is_live()) {
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
