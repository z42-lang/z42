//! `StrTable` — the VM's string-literal identity: every `ConstStr` operand the VM can
//! execute is a dense id into this table, and once an id has been executed its interned
//! GC string is a lock-free read (P1-2 PR 7).
//!
//! **Id space** — one per VM, append-only, ids never reused:
//! - **Entry module** (`VmCore.module`): ids `0..n` are its `string_pool` indices. Their
//!   slots carry no text; it is read from the module (the table holds a clone of the
//!   module's `Arc`).
//! - **Lazily loaded packages**: the loader appends a package's whole pool in one block
//!   when it registers the package ([`StrTable::append`]) and shifts the package's
//!   `ConstStr` operands by the returned first id (`remap_const_str`). Installing a
//!   loader again (tests only) keeps appending, so an operand of a function the old
//!   loader registered still names its own text.
//! - **Reserved ids** ([`StrTable::reserve`]): a loader numbers its packages from its
//!   `main_pool_len`; on a VM without an entry module (unit tests) the ids below it get
//!   empty slots, which resolve to nothing.
//!
//! **Interned strings**: a slot's `gc` is set once, by the first `ConstStr` of that id on
//! any thread of the VM (`VmContext::const_str`). The decision is made under the `roots`
//! lock, which records the string in the table's root set (scanned by the external root
//! scanner, so an interned string lives as long as the VM). The string is allocated
//! *before* the lock is taken — the allocator may take heap locks, and the scanner takes
//! `roots` from inside a collection — so two threads executing a fresh id at once may
//! both allocate; the loser drops its copy (unreachable garbage) and returns the winner's.
//! No collection can run between the allocation and the publish: both happen inside one
//! instruction, never across a safepoint.
//!
//! GC strings belong to one heap, so the table is per VM (`VmCore.strings`), never in
//! metadata that VMs could share. Threads of one VM see the same interned string for an
//! id; two VMs never share one. String identity is not observable from z42
//! (`ReferenceEquals` is false for strings, strings take no identity hash or weak
//! reference), so sharing across threads changes nothing a program can see.
//!
//! Memory ordering: a slot is published by [`SegVec`]'s Release store of its length;
//! `gc` is a `OnceLock` (Release on set, Acquire on read), so a reader that sees the
//! handle sees the string's bytes.

use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

use super::bytecode::Module;
use super::seg_vec::SegVec;
use super::vstr::Str;

/// One string id.
struct StrSlot {
    /// The literal's text. `None` for entry-module ids (the text is in
    /// `StrTable.entry.string_pool`) and for reserved ids.
    text: Option<Box<str>>,
    /// The interned GC string, set by the first execution.
    gc: OnceLock<Str>,
}

impl StrSlot {
    fn new(text: Option<Box<str>>) -> Self {
        Self { text, gc: OnceLock::new() }
    }
}

/// See the module doc.
pub struct StrTable {
    slots: SegVec<StrSlot>,
    entry: Option<Arc<Module>>,
    /// Every interned string, in publication order — the table's GC roots. Also the
    /// lock under which a slot's `gc` is decided.
    roots: Mutex<Vec<Str>>,
}

impl StrTable {
    /// Build the table with the entry module's pool as ids `0..n`.
    pub fn new(entry: Option<Arc<Module>>) -> Self {
        let slots = SegVec::new();
        if let Some(m) = &entry {
            let first = slots.extend_with(m.string_pool.len(), |_| StrSlot::new(None));
            assert_eq!(first, Some(0), "StrTable: entry module exceeds the string id space");
        }
        Self { slots, entry, roots: Mutex::new(Vec::new()) }
    }

    /// Is `module` this table's entry module (ids `0..n` are its pool indices)?
    #[inline]
    pub fn is_entry(&self, module: &Module) -> bool {
        self.entry.as_deref().is_some_and(|m| std::ptr::eq(m, module))
    }

    /// Number of ids (entry + reserved + appended).
    #[inline]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Grow the id space to at least `n` with empty slots (see the module doc). Called
    /// when a loader is attached — never concurrently with [`Self::append`], which runs
    /// under the same loader's write lock.
    pub fn reserve(&self, n: usize) {
        let len = self.slots.len();
        if n > len {
            let first = self.slots.extend_with(n - len, |_| StrSlot::new(None));
            debug_assert_eq!(first, Some(len), "StrTable::reserve raced an append");
        }
    }

    /// Append a package's string pool; returns the id of its first entry (the offset
    /// its `ConstStr` operands are shifted by). `None` when the id space is full.
    pub fn append(&self, pool: Vec<String>) -> Option<u32> {
        let mut texts = pool.into_iter();
        let first = self.slots.extend_with(texts.len(), |_| {
            StrSlot::new(texts.next().map(String::into_boxed_str))
        })?;
        u32::try_from(first).ok()
    }

    /// The text of `id`, or `None` for an unknown or reserved id.
    pub fn text(&self, id: u32) -> Option<&str> {
        let slot = self.slots.get(id as usize)?;
        self.text_of(id, slot)
    }

    fn text_of<'a>(&'a self, id: u32, slot: &'a StrSlot) -> Option<&'a str> {
        match &slot.text {
            Some(t) => Some(t),
            None => self.entry.as_ref()?.string_pool.get(id as usize).map(String::as_str),
        }
    }

    /// Lock-free: the interned string of `id`, if an execution has made it yet.
    #[inline]
    pub fn get(&self, id: u32) -> Option<Str> {
        self.slots.get(id as usize)?.gc.get().copied()
    }

    /// The interned string of `id`, making it with `alloc` on the first call. `None` for
    /// an unknown or reserved id. A hit takes no lock and hashes nothing.
    #[inline]
    pub fn get_or_intern(&self, id: u32, alloc: impl FnOnce(&str) -> Str) -> Option<Str> {
        let slot = self.slots.get(id as usize)?;
        match slot.gc.get() {
            Some(s) => Some(*s),
            None => self.intern_slow(id, slot, alloc),
        }
    }

    #[cold]
    fn intern_slow(&self, id: u32, slot: &StrSlot, alloc: impl FnOnce(&str) -> Str) -> Option<Str> {
        let fresh = alloc(self.text_of(id, slot)?);
        let mut roots = self.roots.lock();
        if let Some(s) = slot.gc.get() {
            // Another thread published first; `fresh` is unreachable garbage.
            return Some(*s);
        }
        // Cannot fail: `gc` is only ever set under `roots`, and it was empty just now.
        let _ = slot.gc.set(fresh);
        roots.push(fresh);
        Some(fresh)
    }

    /// Visit every interned string (the table's GC roots). Called by the root scanners,
    /// inside a GC pause.
    pub fn scan_roots(&self, mut visit: impl FnMut(Str)) {
        for s in self.roots.lock().iter() {
            visit(*s);
        }
    }

    /// Number of interned strings.
    pub fn interned_count(&self) -> usize {
        self.roots.lock().len()
    }
}

#[cfg(test)]
#[path = "str_table_tests.rs"]
mod str_table_tests;
