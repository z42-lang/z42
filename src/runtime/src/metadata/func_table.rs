//! `FuncTable` — the VM's process-level function identity: every function the
//! VM can run gets one dense [`FnId`], and `get(id)` is a lock-free read.
//!
//! - **Entry module** (`VmCore.module`): ids `0..n` equal the
//!   `Module.functions` index, registered in one block when the `VmCore` is
//!   built. Their slots borrow from the module (`own = None`); the table holds
//!   a clone of the module's `Arc`, so the borrow is sound on its own.
//! - **Lazily loaded packages**: each function is appended when its package
//!   registers with the `LazyLoader` (`LazyLoader::insert_function`), in
//!   registration order; the slot owns an `Arc<Function>`.
//! - Ids are never reused and slots never change, so a cached id stays valid
//!   for the table's lifetime.
//!
//! **The table is also the loader's function registry**: `by_name` (lazy
//! functions only) is the one name → function map for lazily loaded code —
//! `LazyLoader` keeps no map of its own and answers its probes from here
//! ([`FuncTable::lazy_fn`]). Duplicate lazy names are first-wins (no new id).
//! Entry names are not copied into `by_name`, so boot adds no hashing; a lazy
//! function whose name is also an entry-module name still gets its own id
//! (registered as a separate function), and [`FuncTable::id_of`] answers with
//! the entry one — the same precedence call resolution uses.
//!
//! Installing a lazy loader again (tests only) starts a fresh name space with
//! [`FuncTable::reset_lazy_names`]: the new loader sees none of the old one's
//! functions, as before the two maps were merged. The old slots stay (ids are
//! never reused), so ids cached against them keep pointing at the old functions.
//!
//! Consumers: `Call` site tokens (`ResolvedTokens.method_tokens` hold `FnId`s,
//! see `metadata::resolver`), the JIT slot table (`JitModuleCtx`, indexed by
//! `FnId`), and the `VCall` PIC payload (`metadata::resolver::ic`) — all three
//! read a lazily loaded callee back through [`FuncTable::get`].
//!
//! Memory ordering: a slot is published by [`SegVec`]'s Release store of its
//! length; `Function.id` is set before that, inside the append. `FnSlot.func`
//! is loaded with Acquire. Keep both Acquire: the site caches that store `FnId`s
//! (`Call` tokens, the `VCall` PIC) are read `Relaxed` and rely on this pairing
//! to order the slot's contents — an id seen before its registration is visible
//! reads as `None` here, never as a half-built slot.

use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;
use rustc_hash::FxHashMap;

use super::bytecode::{Function, Module};
use super::seg_vec::SegVec;
use super::tokens::FnId;

/// One registered function.
struct FnSlot {
    /// Never null. Points into `FuncTable.entry` (entry functions) or at `own`.
    func: AtomicPtr<Function>,
    /// Keeps a lazily loaded function alive; `None` for entry-module functions.
    own: Option<Arc<Function>>,
}

/// See the module doc.
pub struct FuncTable {
    slots:   SegVec<FnSlot>,
    entry:   Option<Arc<Module>>,
    /// Lazy functions only: FQ name → id (first-wins).
    by_name: RwLock<FxHashMap<Box<str>, FnId>>,
}

impl FuncTable {
    /// Build the table and register every entry-module function as `0..n`
    /// (one lock acquisition, one store of `Function.id` per function).
    pub fn new(entry: Option<Arc<Module>>) -> Self {
        let slots = SegVec::new();
        if let Some(m) = &entry {
            let first = slots.extend_with(m.functions.len(), |i| {
                let f = &m.functions[i];
                f.id.set(FnId(i as u32));
                FnSlot { func: AtomicPtr::new(f as *const Function as *mut Function), own: None }
            });
            assert_eq!(first, Some(0), "FuncTable: entry module exceeds the FnId space");
        }
        Self { slots, entry, by_name: RwLock::new(FxHashMap::default()) }
    }

    /// Number of registered functions (entry + lazy).
    #[inline]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Number of entry-module functions (ids `0..entry_len`).
    pub fn entry_len(&self) -> usize {
        self.entry.as_ref().map_or(0, |m| m.functions.len())
    }

    /// Lock-free: the function registered under `id`, or `None` if `id` has not
    /// been published (yet).
    #[inline]
    pub fn get(&self, id: FnId) -> Option<&Function> {
        let slot = self.slots.get(id.0 as usize)?;
        let p = slot.func.load(Ordering::Acquire);
        // SAFETY: `func` is never null; it points into `self.entry` (whose `Arc` this
        // table holds and whose `functions` are never mutated after the module is
        // shared) or at `slot.own`. Slots are never removed while `self` lives.
        Some(unsafe { &*p })
    }

    /// Is `module` this table's entry module (ids `0..entry_len` are its indices)?
    #[inline]
    pub fn is_entry(&self, module: &Module) -> bool {
        self.entry.as_deref().is_some_and(|m| std::ptr::eq(m, module))
    }

    /// Cold path: the id a call by `name` would resolve to (entry module first).
    pub fn id_of(&self, name: &str) -> Option<FnId> {
        if let Some(&i) = self.entry.as_ref().and_then(|m| m.func_index.get(name)) {
            return Some(FnId(i as u32));
        }
        self.lazy_id(name)
    }

    /// The registered lazily loaded function named `name` (no entry lookup).
    pub fn lazy_id(&self, name: &str) -> Option<FnId> {
        self.by_name.read().get(name).copied()
    }

    /// The registered lazily loaded function named `name`, as the owning `Arc`
    /// (the loader's probes hand it out).
    pub fn lazy_fn(&self, name: &str) -> Option<Arc<Function>> {
        let id = self.lazy_id(name)?;
        self.slots.get(id.0 as usize)?.own.clone()
    }

    /// Number of registered lazy names (the loader's registry size).
    pub fn lazy_name_count(&self) -> usize {
        self.by_name.read().len()
    }

    /// Register a lazily loaded function and return its id, or `None` when a
    /// lazy function with the same name is already registered (first-wins: `f`
    /// stays unregistered). Sets `f.id` before the slot is published.
    pub fn register_lazy(&self, f: &Arc<Function>) -> Option<FnId> {
        let mut names = self.by_name.write();
        if names.contains_key(f.name.as_str()) {
            return None;
        }
        let pushed = self.slots.push_with(|i| {
            f.id.set(FnId(i as u32));
            FnSlot { func: AtomicPtr::new(Arc::as_ptr(f) as *mut Function), own: Some(Arc::clone(f)) }
        });
        let Some(idx) = pushed else { panic!("FuncTable: FnId space exhausted") };
        let id = FnId(idx as u32);
        names.insert(f.name.as_str().into(), id);
        Some(id)
    }

    /// A new lazy loader is being installed: forget the previous loader's names
    /// (see the module doc). Slots and ids are untouched.
    pub fn reset_lazy_names(&self) {
        self.by_name.write().clear();
    }
}

#[cfg(test)]
#[path = "func_table_tests.rs"]
mod func_table_tests;
