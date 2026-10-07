//! `TypeTable` — the VM's type identity: `TypeId` → descriptor, lock-free reads.
//!
//! `TypeId`s are **process-global** (drawn from `tokens::alloc_type_id_block` when a
//! module's type registry is built), so one VM sees a sparse subset of the id space;
//! the table is a [`SparseSegTable`] indexed by the bare id. Registration:
//!
//! - **Entry module**: every descriptor in `module.type_registry`, when the `VmCore`
//!   is built ([`TypeTable::new`]).
//! - **Lazily loaded packages**: the loader publishes its whole type registry after
//!   each step that changes it — a package load (after the inheritance fixup has
//!   run), a type-registry seed, a base-chain fixup ([`TypeTable::publish`] via
//!   `LazyLoader::publish_types`). A first publish happens after the package's own
//!   fixup pass, so a fresh descriptor is still uniquely owned there and the fixup
//!   mutates it in place.
//!
//! A slot holds the **latest version** of its type. `try_fixup_inheritance` may
//! replace a registry entry with a merged copy (`Arc::make_mut` clone-on-write) that
//! keeps the same id and name; publishing it repoints the slot. Every published
//! version stays alive in `own` until the table drops, so a `&TypeDesc` from
//! [`TypeTable::get`] never dangles. A descriptor with a *different* name under a
//! taken id is refused (first-wins, logged) — ids are unique per type, so that only
//! happens if the allocator invariant is broken.
//!
//! Not registered: transient fallback descriptors (`make_fallback_type_desc`, built per
//! allocation, `id == UNRESOLVED`), the corelib native-handle singletons (process
//! statics with `UNRESOLVED` ids), and types of modules held only by a load context
//! (`metadata::context`). Name lookups still go through `Module.type_registry` and the
//! loader's registry; this table carries ids only.
//!
//! **Target name keys** ([`TypeTable::name_key`]): a type test whose target names no
//! registered type (an erased generic `Demo.GBox`, an arity-mangled `GBox$1`, an
//! interface whose package is not loaded) still needs a key in the `TypeId` space for
//! `isa_cache`. Such a name gets an id from the same allocator, reserved for that name
//! (first-wins, never reused, never given to a descriptor). See `tokens::TypeKeyCell`.
//!
//! Memory ordering: the slot pointer is stored with Release after the `Arc` is pushed
//! onto `own`, and loaded with Acquire, so a reader that sees the pointer sees the
//! descriptor's contents.

use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;

use super::bytecode::Module;
use super::seg_vec::SparseSegTable;
use super::tokens::{alloc_type_id_block, TypeId};
use super::types::TypeDesc;

/// One id's slot: null until the id is registered in this table.
#[derive(Default)]
struct TypeSlot {
    desc: AtomicPtr<TypeDesc>,
}

/// See the module doc.
#[derive(Default)]
pub struct TypeTable {
    slots: SparseSegTable<TypeSlot>,
    /// Every published descriptor version (keeps the slot pointers valid).
    own:   Mutex<Vec<Arc<TypeDesc>>>,
    /// Number of registered ids.
    count: AtomicUsize,
    /// Target names that named no registered type when first keyed → reserved id.
    names: RwLock<FxHashMap<Box<str>, u32>>,
}

impl TypeTable {
    /// Build the table and register the entry module's type registry.
    pub fn new(entry: Option<&Module>) -> Self {
        let t = Self::default();
        if let Some(m) = entry {
            for td in m.type_registry.values() {
                t.publish(td);
            }
        }
        t
    }

    /// Number of registered ids.
    #[inline]
    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lock-free: the latest registered version of type `id`, or `None` when this
    /// VM has not registered it (another VM's type, a reserved name key, a transient
    /// descriptor's `UNRESOLVED`).
    #[inline]
    pub fn get(&self, id: TypeId) -> Option<&TypeDesc> {
        let p = self.slots.get(id.0 as usize)?.desc.load(Ordering::Acquire);
        // SAFETY: a non-null slot pointer comes from an `Arc` held in `self.own`,
        // which only grows while `self` lives.
        (!p.is_null()).then(|| unsafe { &*p })
    }

    /// Register `td` under its id, or repoint the slot at this newer version of the
    /// same type. Returns `true` when the slot changed. No-op for `UNRESOLVED` ids and
    /// when the slot already points at `td`; refuses a different type under a taken id.
    pub fn publish(&self, td: &Arc<TypeDesc>) -> bool {
        if !td.id.is_resolved() || td.id.is_import() {
            return false;
        }
        let Some(slot) = self.slots.get_or_init(td.id.0 as usize) else { return false };
        let new = Arc::as_ptr(td) as *mut TypeDesc;
        let mut own = self.own.lock();
        // Under `own`'s lock: publishers are serialized, so load → store cannot race.
        let cur = slot.desc.load(Ordering::Relaxed);
        if cur == new {
            return false;
        }
        if !cur.is_null() {
            // SAFETY: as in `get`.
            let prev = unsafe { &*cur };
            if prev.name != td.name {
                tracing::error!(
                    "TypeId {} is registered as `{}`; refusing `{}` under the same id \
                     (TypeIds must be process-unique, see tokens::alloc_type_id_block)",
                    td.id.0, prev.name, td.name
                );
                return false;
            }
        } else {
            self.count.fetch_add(1, Ordering::Relaxed);
        }
        own.push(Arc::clone(td));
        slot.desc.store(new, Ordering::Release);
        true
    }

    /// The key reserved for type-test target `name` (allocated on first ask). Cold
    /// path: callers cache the result per site (`tokens::TypeKeyCell`).
    pub fn name_key(&self, name: &str) -> u32 {
        if let Some(&k) = self.names.read().get(name) {
            return k;
        }
        *self.names.write().entry(name.into()).or_insert_with(|| alloc_type_id_block(1))
    }
}

#[cfg(test)]
#[path = "type_table_tests.rs"]
mod type_table_tests;
