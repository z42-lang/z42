//! `NameIndex` — linear-scan replacement for the `HashMap<String, usize>`
//! used by `TypeDesc.field_index` + `TypeDesc.vtable_index`.
//!
//! review.md C4 / C5 P1 : the FieldIC / VCallIC monomorphic +
//! polymorphic caches catch the hot path, but IC miss still goes through
//! `HashMap.get(&str)` which costs a `SipHash` hash + at least one string
//! compare. For the typical z42 class (≤16 fields / vtable slots), a
//! straight linear scan of `Vec<(Box<str>, usize)>` is faster than the
//! HashMap probe: cache-locality friendly, no hash compute, and branch
//! prediction loves the small loop.
//!
//! Memory: `Box<str>` saves 8 B / entry vs `String` (no `capacity` word).
//! For a class with 8 fields that's 64 B / class — meaningful with stdlib's
//! ~80 classes.
//!
//! # API
//!
//! Mirrors the `HashMap<String, usize>` subset that `TypeDesc` callers
//! actually use, so swapping the type is a no-op at most call sites:
//!
//! - `get(&str) -> Option<&usize>` — returns a reference for compat with
//!   `.copied()` chains throughout the interp / JIT helpers.
//! - `insert(String, usize) -> Option<usize>` — takes owned `String` like
//!   `HashMap::insert`; internally converts to `Box<str>`.
//! - `iter()` — yields `(&str, usize)` pairs (slightly diverges from
//!   HashMap's `(&K, &V)` — production code never reads slot by ref).
//! - `FromIterator<(String, usize)>` — for `.collect()` in loader.
//!
//! # When NOT to use
//!
//! Linear scan is O(n). If a class ever ships with ≥64 fields / methods
//! the scan starts to lose to HashMap. We don't degrade gracefully; if
//! that pattern emerges, switch the implementation to a hybrid (linear
//! for n ≤ K, hashmap for n > K) without changing the public API.
//!
//! `Send` / `Sync` are auto-derived (Vec<(Box<str>, usize)> is both).
//! Required because `TypeDesc` is shared across threads via `Arc`.

use std::fmt;

/// Linear-scan name → slot index map. See module docs for design rationale.
#[derive(Default, Clone)]
pub struct NameIndex {
    entries: Vec<(Box<str>, usize)>,
}

impl NameIndex {
    /// Empty index.
    #[inline]
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Pre-allocate capacity. Useful in loaders that know the size up front
    /// (e.g. iterating a class's flattened field list).
    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Self { entries: Vec::with_capacity(cap) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Linear-scan lookup. Returns a reference to the stored slot for compat
    /// with `field_index.get(name).copied()` chains.
    #[inline]
    pub fn get(&self, name: &str) -> Option<&usize> {
        // Manual loop instead of `iter().find(...)` — tighter codegen and
        // avoids the `(&Box<str>, &usize)` borrow pair when we only need
        // to compare the key.
        for (k, v) in &self.entries {
            if k.as_ref() == name {
                return Some(v);
            }
        }
        None
    }

    /// True if `name` exists.
    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// Insert or update. Returns the previous slot if `name` already
    /// existed, mirroring `HashMap::insert` semantics.
    pub fn insert(&mut self, name: String, slot: usize) -> Option<usize> {
        // Update-in-place if name already present.
        for entry in &mut self.entries {
            if entry.0.as_ref() == name {
                return Some(std::mem::replace(&mut entry.1, slot));
            }
        }
        self.entries.push((name.into_boxed_str(), slot));
        None
    }

    /// Iterate `(name, slot)` pairs in insertion order (HashMap iteration
    /// is unordered; NameIndex iteration is stable, which is incidentally
    /// useful for debug printing).
    pub fn iter(&self) -> impl Iterator<Item = (&str, usize)> + '_ {
        self.entries.iter().map(|(k, v)| (k.as_ref(), *v))
    }

    /// Iterate names only.
    pub fn keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries.iter().map(|(k, _)| k.as_ref())
    }
}

impl fmt::Debug for NameIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Render like a map: {"name": slot, ...}
        f.debug_map()
            .entries(self.iter().map(|(k, v)| (k, v)))
            .finish()
    }
}

impl FromIterator<(String, usize)> for NameIndex {
    fn from_iter<I: IntoIterator<Item = (String, usize)>>(iter: I) -> Self {
        let mut idx = NameIndex::new();
        for (name, slot) in iter {
            idx.insert(name, slot);
        }
        idx
    }
}

#[cfg(test)]
#[path = "name_tests.rs"]
mod name_index_tests;
