//! Per-string character-metadata cache (perf-str-char-index).
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
//! Freed-chunk soundness (D7): `is_live` reads the cached block's header, which is only
//! sound while that memory is still the region's. Blocks in bump chunks qualify — a bump
//! chunk is pooled when it empties, never handed back while the region lives. An oversized
//! block (> 64 KB, `Z42_GC_LOH_BYTES`) does not: it sits alone in a dedicated chunk that the
//! collection `dealloc`s the moment the block dies, and whatever malloc puts at that address
//! next (another oversized block, the first block of a fresh bump chunk) is born at
//! `FIRST_GEN` — the dead string's generation — so the stale entry would pass both checks and
//! return the dead string's char count and offsets (wrong `Length`, `CharAt` slicing at a
//! non-boundary). So an entry whose block may be in a dedicated chunk records that chunk's `(chunk, epoch)` stamp — the identity
//! weak references use; the epoch moves when the chunk is freed — and every lookup compares the
//! process-wide dedicated-free count (one load). Both come from the heap layer through
//! [`crate::gc::ambient::cache_scope`], the one ambient entry this cache uses. When the
//! count moved since the last lookup, each stamped entry is re-checked against its chunk under
//! the region lock and dropped if the chunk was freed — before anything reads its header.
//! Bump-chunk entries are never re-checked, and a free elsewhere costs a dedicated entry one
//! lock, not a recompute.
//!
//! The cache only changes *speed*, never the returned value — byte-identical
//! output is preserved.
//!
//! Strings marked all-ASCII when their block was written (`Str::is_known_ascii`, a
//! header bit set by the heap's string allocators) skip the cache entirely: char
//! index == byte index and the char count is the byte length. The cache is only
//! consulted for non-ASCII strings and for strings created by a path that does not
//! set the bit.

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
    /// D7: `Some((chunk, epoch))` when `s` may sit in a dedicated chunk, whose memory goes back
    /// to the allocator when `s` dies — see the module's freed-chunk note. `None` for a block in
    /// a bump chunk (or in no chunk at all), whose header stays readable.
    chunk:    Option<(u32, u32)>,
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
    /// Paired with **D7**'s dedicated-chunk free count (`CacheScope::dedicated_frees`) at
    /// the last lookup: when that moves, every entry stamped with a chunk is re-checked before
    /// reuse. One cell, so the lookup pays one TLS access for both.
    static LAST_EPOCH: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

#[inline]
fn data_ptr(s: &Str) -> *const u8 {
    s.as_ptr()
}

/// `chunk`: D7's stamp for `s` (`CacheScope::dedicated_chunk_stamp`).
fn compute(s: &Str, chunk: Option<(u32, u32)>) -> StrMeta {
    let bytes = s.as_bytes();
    if bytes.is_ascii() {
        StrMeta { s: s.clone(), char_len: bytes.len(), ascii: true, offsets: None, chunk }
    } else {
        let offsets: Vec<u32> = s.char_indices().map(|(b, _)| b as u32).collect();
        let char_len = offsets.len();
        StrMeta { s: s.clone(), char_len, ascii: false, offsets: Some(offsets.into_boxed_slice()), chunk }
    }
}

/// D7: drop every entry whose dedicated chunk was freed since it was stamped — its header
/// address may now be unmapped, or hold a different block at the same generation. Called only
/// when the dedicated-free count moved; reads no header.
fn drop_entries_in_freed_chunks(cache: &mut Vec<StrMeta>, unchanged: impl Fn(u32, u32) -> bool) {
    cache.retain(|e| e.chunk.is_none_or(|(chunk, epoch)| unchanged(chunk, epoch)));
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
    let scope = crate::gc::ambient::cache_scope();
    let (epoch, frees) = (scope.epoch, scope.dedicated_frees);
    CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        let (last_epoch, last_frees) = LAST_EPOCH.with(|le| le.replace((epoch, frees)));
        if last_epoch != epoch {
            cache.clear();
        } else if last_frees != frees {
            // D7: a dedicated chunk was freed somewhere since the last lookup — drop the entries
            // whose chunk it was, before `is_live` below could read their (freed) headers.
            drop_entries_in_freed_chunks(&mut cache, |chunk, ep| scope.chunk_unchanged(chunk, ep));
        }
        let key = data_ptr(s);
        // Hit = same allocation address AND the cached handle is still live (its
        // block wasn't swept + its slot reused by a different string) — see the
        // module soundness notes. A stale (dead-slot) entry fails `is_live` and is
        // skipped, so we recompute for the current `s`. Every entry left here has a
        // mapped header: bump-chunk blocks always, dedicated ones by the check above.
        if let Some(pos) = cache.iter().position(|e| data_ptr(&e.s) == key && e.s.var_ref().is_live()) {
            if pos != 0 {
                let e = cache.remove(pos);
                cache.insert(0, e);
            }
            return f(&cache[0]);
        }
        // D7: stamp a block that may live in a dedicated chunk (`s` is live here, so the stamp
        // names its chunk). One region lock, only past the large-object threshold — whose offsets
        // table is a ≥ 64 KB walk anyway.
        let meta = compute(s, scope.dedicated_chunk_stamp(s));
        cache.insert(0, meta);
        if cache.len() > CACHE_CAP {
            cache.pop();
        }
        f(&cache[0])
    })
}

/// O(1) (amortised) Unicode scalar count — backs `Std.String.Length`.
pub fn char_len(s: &Str) -> usize {
    if s.is_known_ascii() { return s.len(); }
    with_meta(s, |m| m.char_len)
}

/// O(1) (amortised) char at scalar index `i` — backs `Std.String.CharAt`.
/// Returns `None` when `i >= char_len`.
/// Byte range `[b0, b1)` of the character range `[start, start+len)`; `None` when the
/// range is out of bounds. ASCII strings map 1:1; non-ASCII use the cached offsets table.
/// perf-stdlib-hot-paths: backs `Std.String.Substring` (one slice copy instead of a
/// per-character `CharAt` loop).
pub fn byte_range(s: &Str, start: usize, len: usize) -> Option<(usize, usize)> {
    if s.is_known_ascii() {
        let end = start.checked_add(len)?;
        return (end <= s.len()).then_some((start, end));
    }
    with_meta(s, |m| {
        let end = start.checked_add(len)?;
        if end > m.char_len { return None; }
        if m.ascii { return Some((start, end)); }
        let offs = m.offsets.as_ref()?;
        let b0 = offs[start] as usize;
        let b1 = if end == m.char_len { m.s.len() } else { offs[end] as usize };
        Some((b0, b1))
    })
}

pub fn char_at(s: &Str, i: usize) -> Option<char> {
    if s.is_known_ascii() { return s.as_bytes().get(i).map(|&b| b as char); }
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
