//! unify-object-byte-layout PR-4: `Str` — an 8-byte thin, atomically ref-counted,
//! immutable UTF-8 string handle.
//!
//! `Value::Str` was `Arc<str>` — a **fat** pointer (data ptr + length = 16 B), which made
//! `Value` 24 B (its largest payload). `Str` stores the length **inline in the heap header**
//! instead of in the pointer, so the handle is a single **thin** `NonNull` (8 B). That is
//! what lets `Value` shrink to 16 B (PR-5).
//!
//! Layout of one allocation (`alloc(header ++ bytes)`, one allocation total):
//! ```text
//!   ┌───────────────┬──────────┬──────────────── … ─────────┐
//!   │ strong: usize │ len:usize│ data: [u8; len] (UTF-8)     │
//!   └───────────────┴──────────┴──────────────── … ─────────┘
//!     └── StrHeader (Sized, 16 B) ──┘└── inline bytes follow ──┘
//! ```
//! The bytes follow the `StrHeader` in the same allocation (accessed by pointer
//! arithmetic), so there is exactly one allocation per string and the handle carries no
//! length metadata. This mirrors `triomphe::ThinArc` / the std `Arc<str>` internals, but
//! hand-rolled (no new dependency; tunable for the string-heavy z42c self-compile).
//!
//! **Refcount, not GC** (design, User-confirmed 2026-08-15): the GC region allocator is
//! fixed-size-typed (`RegionEntry<T>`) and can't inline variable-length string bytes, so a
//! GC-managed string would need a second (out-of-line) allocation or a variable-size
//! allocator rework. Strings are also immutable **leaves** (never reference other heap
//! objects, never form cycles), so tracing GC would add mark/sweep cost for zero
//! cycle-collection benefit. `Arc`-style atomic refcount (thread-safe: `Value: Send+Sync`)
//! with one inline allocation is the better fit.

use std::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use std::hash::{Hash, Hasher};
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering};

/// Heap header preceding a string's inline UTF-8 bytes. `#[repr(C)]` pins the field order
/// so the `data` bytes always start at `size_of::<StrHeader>()`.
#[repr(C)]
struct StrHeader {
    /// Atomic strong reference count (starts at 1; `Str::clone` increments, `drop`
    /// decrements + frees at 0). Atomic because `Value: Send + Sync`.
    strong: AtomicUsize,
    /// Byte length of the inline UTF-8 `data` that follows this header.
    len: usize,
}

/// 8-byte thin, atomically ref-counted, immutable UTF-8 string. See the module docs.
pub struct Str {
    /// Points at the `StrHeader`; the UTF-8 bytes follow it in the same allocation. Never
    /// null (a live handle always points at a valid header).
    ptr: NonNull<StrHeader>,
}

// PR-4 invariant: the handle is exactly one machine word (this is the whole point — it is
// what lets `Value` reach 16 B in PR-5). `Option<Str>` stays 8 B via the `NonNull` niche.
const _: () = assert!(std::mem::size_of::<Str>() == std::mem::size_of::<usize>());
const _: () = assert!(std::mem::size_of::<Option<Str>>() == std::mem::size_of::<usize>());

// SAFETY: the pointed-to bytes are immutable for the allocation's lifetime and the
// refcount is atomic, so a `Str` is safe to send/share across threads exactly like
// `Arc<str>`.
unsafe impl Send for Str {}
unsafe impl Sync for Str {}

impl Str {
    /// Byte offset of the inline data within the allocation = the (padded) header size.
    /// `StrHeader` is `{usize, usize}` (align 8), `u8` has align 1, so no extra padding.
    const DATA_OFFSET: usize = std::mem::size_of::<StrHeader>();

    /// Compute the allocation `Layout` for a header + `len` inline UTF-8 bytes.
    #[inline]
    fn layout_for(len: usize) -> Layout {
        // header (16 B, align 8) followed by `len` u8; align stays 8, size = 16 + len.
        Layout::from_size_align(Self::DATA_OFFSET + len, std::mem::align_of::<StrHeader>())
            .expect("string length overflows isize")
    }

    /// Allocate a fresh `Str` copying `s`'s UTF-8 bytes inline (one allocation).
    pub fn new(s: &str) -> Self {
        let len = s.len();
        let layout = Self::layout_for(len);
        // SAFETY: layout has non-zero size (header is 16 B > 0). On OOM we abort via
        // `handle_alloc_error` rather than returning a dangling pointer.
        let raw = unsafe { alloc(layout) };
        let Some(header_ptr) = NonNull::new(raw as *mut StrHeader) else {
            handle_alloc_error(layout);
        };
        // SAFETY: `header_ptr` points at freshly-allocated, correctly-aligned space for a
        // `StrHeader`; write the initial refcount + length, then copy the bytes into the
        // trailing region `[DATA_OFFSET .. DATA_OFFSET+len)`.
        unsafe {
            header_ptr.as_ptr().write(StrHeader {
                strong: AtomicUsize::new(1),
                len,
            });
            let data = (raw as *mut u8).add(Self::DATA_OFFSET);
            std::ptr::copy_nonoverlapping(s.as_ptr(), data, len);
        }
        Str { ptr: header_ptr }
    }

    #[inline]
    fn header(&self) -> &StrHeader {
        // SAFETY: the header stays valid while this `Str` holds a strong reference.
        unsafe { self.ptr.as_ref() }
    }

    /// Length in bytes (O(1) — read from the inline header, no deref of the data).
    #[inline]
    pub fn len(&self) -> usize {
        self.header().len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The inline UTF-8 bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        let len = self.len();
        // SAFETY: the `len` bytes at `DATA_OFFSET` were initialized at construction from a
        // valid `&str` and are immutable for the allocation's lifetime.
        unsafe {
            let data = (self.ptr.as_ptr() as *const u8).add(Self::DATA_OFFSET);
            std::slice::from_raw_parts(data, len)
        }
    }

    /// Borrow as `&str`.
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: the bytes came from a valid `&str` (UTF-8) and are never mutated.
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Stable allocation-identity pointer (the header address). Unique per backing
    /// allocation for its lifetime — usable as an O(1) identity key (e.g. the char-metadata
    /// cache) exactly like `Arc::as_ptr`. Two `Str`s compare equal here iff they share the
    /// same allocation (one was cloned from the other), NOT iff they have equal content.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr() as *const u8
    }

    /// Current strong reference count (for tests / diagnostics).
    #[cfg(test)]
    fn strong_count(&self) -> usize {
        self.header().strong.load(Ordering::Acquire)
    }
}

impl Clone for Str {
    #[inline]
    fn clone(&self) -> Self {
        // Relaxed is sufficient for a refcount increment (mirrors `std::sync::Arc`): the
        // existing reference we clone from already establishes the needed happens-before.
        let old = self.header().strong.fetch_add(1, Ordering::Relaxed);
        // Guard against a pathological refcount overflow (same backstop as std Arc).
        debug_assert!(old < usize::MAX, "Str strong count overflow");
        Str { ptr: self.ptr }
    }
}

impl Drop for Str {
    #[inline]
    fn drop(&mut self) {
        // Release so all prior reads/writes of this thread are visible to whoever performs
        // the final decrement (mirrors `std::sync::Arc::drop`).
        if self.header().strong.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }
        // We are the last owner. Acquire fence so the deallocation happens-after every
        // other thread's use of the data.
        fence(Ordering::Acquire);
        let layout = Self::layout_for(self.len());
        // SAFETY: refcount reached 0 → no other handle exists; free the whole allocation
        // (header + inline bytes) with the same layout it was allocated with.
        unsafe {
            dealloc(self.ptr.as_ptr() as *mut u8, layout);
        }
    }
}

impl std::ops::Deref for Str {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for Str {
    #[inline]
    fn from(s: &str) -> Self {
        Str::new(s)
    }
}

impl From<String> for Str {
    #[inline]
    fn from(s: String) -> Self {
        Str::new(&s)
    }
}

impl From<&String> for Str {
    #[inline]
    fn from(s: &String) -> Self {
        Str::new(s)
    }
}

impl From<Box<str>> for Str {
    #[inline]
    fn from(s: Box<str>) -> Self {
        Str::new(&s)
    }
}

impl From<std::borrow::Cow<'_, str>> for Str {
    #[inline]
    fn from(s: std::borrow::Cow<'_, str>) -> Self {
        Str::new(&s)
    }
}

impl std::borrow::Borrow<str> for Str {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for Str {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// Value-equality (content), not pointer-equality — matches `Arc<str>`'s `PartialEq` and the
// language's string `==` semantics.
impl PartialEq for Str {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for Str {}
// NB: deliberately NO `PartialEq<str>` / `PartialEq<&str>` — matching `Arc<str>`, which only
// impls `PartialEq<Arc<str>>`. Extra target impls make `str_val == "lit".into()` ambiguous
// (the `.into()` can't pick between `Str` / `str` / `&str`). Compare against a literal via
// `s.as_str() == "lit"` or `s == Str::from("lit")` / `"lit".into()`.

impl PartialOrd for Str {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Str {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for Str {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash by content so `Str` and `&str`/`String` hash identically (borrow-as-str).
        self.as_str().hash(state);
    }
}

impl std::fmt::Debug for Str {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self.as_str(), f)
    }
}

impl std::fmt::Display for Str {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_content_and_len() {
        let s = Str::new("hello 世界");
        assert_eq!(s.as_str(), "hello 世界");
        assert_eq!(s.len(), "hello 世界".len());
        assert_eq!(&*s, "hello 世界");
        assert_eq!(s.as_str(), "hello 世界");
        assert_eq!(s, Str::from("hello 世界"));
    }

    #[test]
    fn empty_string_is_valid() {
        let s = Str::new("");
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert_eq!(s.as_str(), "");
    }

    #[test]
    fn clone_shares_and_refcounts() {
        let a = Str::new("shared");
        assert_eq!(a.strong_count(), 1);
        let b = a.clone();
        assert_eq!(a.strong_count(), 2);
        // Same backing allocation (thin pointer identity), same content.
        assert_eq!(a.ptr, b.ptr);
        assert_eq!(a.as_str(), b.as_str());
        drop(b);
        assert_eq!(a.strong_count(), 1);
        // `a` frees at end of scope — Miri validates no leak / no double-free.
    }

    #[test]
    fn equality_and_ordering_by_content() {
        assert_eq!(Str::new("abc"), Str::new("abc"));
        assert_ne!(Str::new("abc"), Str::new("abd"));
        assert!(Str::new("abc") < Str::new("abd"));
    }

    #[test]
    fn hashes_like_str() {
        use std::collections::HashMap;
        let mut m: HashMap<Str, i32> = HashMap::new();
        m.insert(Str::new("key"), 42);
        // Borrow<str> lets `&str` look up a `Str` key.
        assert_eq!(m.get("key"), Some(&42));
    }
}
