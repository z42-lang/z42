//! `Str` — an 8-byte thin handle to an **immutable UTF-8 string held in the GC
//! heap** (unify-gc-heap PR-4).
//!
//! # History
//!
//! - unify-object-byte-layout PR-4 made `Str` a hand-rolled thin, atomically
//!   ref-counted pointer (`StrHeader{strong,len}` + inline bytes) so `Value`
//!   could shrink to 16 B — replacing the fat `Arc<str>`.
//! - unify-gc-heap PR-4 (this) moves the string bytes **into the single GC heap**:
//!   `Str` is now a thin [`VarGcRef`] to a `BlockType::Str` variable-length block
//!   (`{GcBlockHeader, inline UTF-8 bytes}`). The atomic refcount is **gone** — the
//!   GC manages the string's lifetime (mark/sweep), exactly like every other
//!   managed object. This is the last variable-length payload to leave the "GC
//!   outside" world, completing the single-heap model (design **A'** / **D3**).
//!
//! # Layout
//!
//! One GC block per string (a single [`VarRegion`](crate::gc::var_region) alloc):
//! ```text
//!   ┌────────────────────────────┬──────────────────────────────┐
//!   │ GcBlockHeader (16 B, align8)│ data: [u8; len] (UTF-8)       │
//!   │  generation/size/marked/    │                               │
//!   │  alive/type_tag=Str/…       │                               │
//!   └────────────────────────────┴──────────────────────────────┘
//! ```
//! The byte length is `header.size` (no separate `len` field). The handle is still
//! a single machine word (8 B) — `Option<Str>` stays 8 B via the `NonNull` niche —
//! so `Value` stays 16 B.
//!
//! # Allocation & the ambient heap
//!
//! `Str::new`/`From<&str>` carry no `&heap`, so they allocate from the **ambient
//! heap** ([`crate::gc::ambient::current_heap`], scoped per frame). In the rare
//! heap-less context (unit tests without a VM), they fall back to a standalone
//! **leaked** block — never taken on a production hot path (see [`Self::new`]).
//!
//! # Lifetime, clone, drop
//!
//! - **Clone** = copy the 8-byte handle (no refcount bump) — cheaper than the old
//!   atomic increment. Both handles name the same immutable block; observationally
//!   identical to the old Arc share (strings are never mutated).
//! - **Drop** = no-op. The GC frees the block when it becomes unreachable; a
//!   `Value::Str` in a frame register is a root (the external root scanner walks
//!   frame regs), so live strings stay marked. Interned pool strings are kept alive
//!   by the per-context intern cache (a scanned root); see `interp::exec_value`.
//! - **Reachability, not refcount**: a `Str` handle sitting in a *Rust* local (not
//!   a GC root) does not by itself keep the block alive — but the GC only runs at
//!   safepoints (loop back-edges / calls) and on `ForceCollect`, never mid-operation,
//!   so a transient `Str` is safe until it lands in a register. This is the same
//!   invariant that already protects `Value::Object`/`Array` temporaries.

use std::hash::{Hash, Hasher};

use crate::gc::var_region::{BlockType, VarGcRef};

/// 8-byte thin handle to an immutable UTF-8 string in the GC heap. See the module
/// docs. `Copy` because the handle is a plain pointer with no owned resource (the
/// GC owns the block); cloning is a bitwise copy.
#[derive(Clone, Copy)]
pub struct Str {
    /// Handle to the `BlockType::Str` GC block (`{GcBlockHeader, UTF-8 bytes}`).
    block: VarGcRef,
}

// PR-4 invariant: the handle is exactly one machine word (what lets `Value` stay
// 16 B). `Option<Str>` stays 8 B via the `VarGcRef` → `NonNull` niche.
const _: () = assert!(std::mem::size_of::<Str>() == std::mem::size_of::<usize>());
const _: () = assert!(std::mem::size_of::<Option<Str>>() == std::mem::size_of::<usize>());

// `Send`/`Sync` come for free: `VarGcRef` is `Send + Sync` (the block memory is
// immutable + GC-managed behind the heap mutex), so `Str` inherits them — no
// hand-written `unsafe impl` needed (unlike the old refcounted version).

impl Str {
    /// Allocate a fresh GC string copying `s`'s UTF-8 bytes inline.
    ///
    /// Uses the ambient heap ([`crate::gc::ambient::current_heap`]) so the ~189
    /// `.into()` / `From<&str>` call sites need no `&heap`. Falls back to a
    /// standalone leaked block only when no frame is executing (unit tests without
    /// a VM); production execution always has an active ambient heap.
    pub fn new(s: &str) -> Self {
        match crate::gc::ambient::current_heap() {
            Some(heap) => heap.alloc_str(s),
            None => Self::new_leaked(s),
        }
    }

    /// Wrap an already-allocated `BlockType::Str` block handle. Used by the heap's
    /// `alloc_str` (after writing the bytes) and by GC read-back paths.
    #[inline]
    pub fn from_var_ref(block: VarGcRef) -> Self {
        Str { block }
    }

    /// The backing GC block handle — for the GC mark phase and allocation-identity.
    #[inline]
    pub fn var_ref(&self) -> VarGcRef {
        self.block
    }

    /// Mark this string's GC block during the mark phase. Returns `true` if this
    /// call won the mark CAS. Strings are leaves (no outgoing references), so
    /// nothing more is traced. Called from `arc_heap`'s `Value::Str` mark arm.
    #[inline]
    pub fn mark(&self) -> bool {
        self.block.mark()
    }

    /// Allocate a **standalone leaked** GC string block (no ambient heap: unit
    /// tests / mock heaps). Never GC-managed, never freed — acceptable there (see
    /// [`VarGcRef::alloc_leaked`]).
    pub fn new_leaked(s: &str) -> Self {
        let block = VarGcRef::alloc_leaked(s.len(), BlockType::Str);
        // SAFETY: fresh block sized for exactly `s.len()` bytes; write the UTF-8
        // bytes into the (zeroed) payload before any read.
        unsafe {
            let dst = block.payload_as_ptr::<u8>();
            std::ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
        }
        Str { block }
    }

    /// Byte length in bytes (O(1) — the block header's `size`).
    #[inline]
    pub fn len(&self) -> usize {
        self.block.payload_size()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The inline UTF-8 bytes.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        let len = self.len();
        // SAFETY: the block payload holds `len` initialized UTF-8 bytes (written at
        // construction) and is immutable for the block's lifetime; the payload
        // pointer is derived from the raw block header (whole-allocation provenance,
        // D8). The block stays live while any `Str` handle to it is reachable.
        unsafe {
            let data = self.block.payload_as_ptr::<u8>() as *const u8;
            std::slice::from_raw_parts(data, len)
        }
    }

    /// Borrow as `&str`.
    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: the bytes came from a valid `&str` (UTF-8) and are never mutated.
        unsafe { std::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Stable allocation-identity pointer (the GC block header address). Unique per
    /// backing block for its lifetime — an O(1) identity key (e.g. the char-metadata
    /// cache) like the old `Arc::as_ptr`. Two `Str`s compare equal here iff they
    /// share the same block (one cloned from the other), NOT iff equal content.
    ///
    /// ⚠️ Under GC the slot can be reclaimed + reused after the block dies; a cache
    /// keyed on this pointer must additionally guard freshness via
    /// [`VarGcRef::is_live`] (generation check) — see `corelib::str_meta`.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.block.addr() as *const u8
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

// Value-equality (content), not pointer-equality — matches the old `Arc<str>`
// `PartialEq` and the language's string `==` semantics.
impl PartialEq for Str {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for Str {}
// NB: deliberately NO `PartialEq<str>` / `PartialEq<&str>` — matching `Arc<str>`,
// which only impls `PartialEq<Arc<str>>`. Extra target impls make
// `str_val == "lit".into()` ambiguous. Compare a literal via `s.as_str() == "lit"`
// or `s == Str::from("lit")` / `"lit".into()`.

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

    // These tests run without a VM → `Str::new` takes the leaked-block fallback
    // (no ambient heap). They validate content/identity semantics, not GC lifetime
    // (that is covered by `gc::var_region` + e2e). The Miri gate is `gc::var_region`,
    // so the intentional leaks here are not leak-checked.

    #[test]
    fn round_trips_content_and_len() {
        let s = Str::new("hello 世界");
        assert_eq!(s.as_str(), "hello 世界");
        assert_eq!(s.len(), "hello 世界".len());
        assert_eq!(&*s, "hello 世界");
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
    fn clone_shares_same_block() {
        let a = Str::new("shared");
        let b = a.clone();
        // Clone copies the handle → same backing block (allocation identity).
        assert_eq!(a.as_ptr(), b.as_ptr());
        assert_eq!(a.as_str(), b.as_str());
    }

    #[test]
    fn distinct_allocs_have_distinct_identity() {
        let a = Str::new("dup");
        let b = Str::new("dup");
        // Equal content, but separate allocations → distinct identity pointers.
        assert_eq!(a, b);
        assert_ne!(a.as_ptr(), b.as_ptr());
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
