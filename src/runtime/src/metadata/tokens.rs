//! Runtime tokens for dispatch hot path.
//!
//! Each token is a `u32` newtype identifying a runtime entity (method / type /
//! builtin / field / static-field / vtable-slot). Tokens are **stable within
//! one VmContext lifetime, NOT persisted to zbc / cross-process**. The current
//! lifecycle:
//!
//!   1. Module load → `metadata::resolver::resolve_module` walks every IR
//!      instruction and resolves available `String` references to tokens
//!      (intra-module hits resolved eagerly; cross-zpkg lazy loaded targets
//!      stay `UNRESOLVED` and are filled on first dispatch).
//!   2. Interp / JIT dispatch hot path checks the cached token; on hit it
//!      indexes a flat `Vec<Function>` / `Vec<Value>` / etc directly,
//!      replacing per-call `HashMap<&str, _>::get()`.
//!
//! Scope differs per kind: `MethodId` / `FieldId` / `VTableSlot` index into one
//! module / type and are only meaningful there, whereas **`TypeId` is
//! process-global** — the dispatch inline caches compare receiver types by bare
//! `u32`, so a per-module id would make two zpkgs' classes indistinguishable.
//! See [`alloc_type_id_block`].
//!
//! See `docs/spec/changes/introduce-method-token/` (or its archived form) for the
//! full design rationale, including the Decision 6 flip that brought
//! Field/Static into Phase 1 alongside method dispatch.

use serde::{Deserialize, Serialize};

/// Sentinel value indicating an unresolved cache slot. Encoded as `u32::MAX`
/// because legitimate IDs are bounded by metadata size (≪ 2^32 entries in
/// practice); a sentinel cannot collide with a real id without overflowing.
pub const UNRESOLVED: u32 = u32::MAX;

/// Phase 3 (`tokenize-ir-and-zbc-bump`, 2026-05-09): bit threshold splitting
/// the `u32` token space into intra-module IDs and import-table indices.
///
/// ```text
/// intra-module:    [0,             0x7FFF_FFFE]   (~2.1B capacity)
/// IMPORT_BASE:     0x8000_0000
/// import indices:  [0x8000_0000,   0xFFFF_FFFE]   (idx = token - IMPORT_BASE)
/// UNRESOLVED:      0xFFFF_FFFF
/// ```
///
/// Applies to `MethodId` / `TypeId` / `StaticFieldId` / `BuiltinId` —
/// kinds that can be cross-zpkg-imported. `FieldId` / `VTableSlot` are
/// always intra-type slot indices and never carry import semantics
/// (the methods on those types still compile but always return false /
/// trip on `import_idx`; the constraint is upheld by the caller, not
/// the type).
pub const IMPORT_BASE: u32 = 0x8000_0000;

/// refactor-vcall-ic-primitives (2026-05-17): synthetic TypeId base for
/// primitive receivers (`Value::I64` / `F64` / `Bool` / `Char` / `Str` / `Array`).
/// These don't appear in `Module.type_registry` — primitives are built-in
/// runtime values, not user-defined classes — but the `VCallIC` machinery
/// keys on a `u32` TypeId. Using a fixed high range avoids collision with
/// real IDs (real IDs grow from 0 and are bounded by metadata size ≪ 2^32).
///
/// Layout: `PRIM_TYPE_BASE + variant_offset`. Range reserves 16 slots
/// (`0xFFFE_0000..0xFFFE_000F`) — far below `UNRESOLVED` (`0xFFFF_FFFF`)
/// and `IMPORT_BASE` (`0x8000_0000`), no collision possible.
pub const PRIM_TYPE_BASE:   u32 = 0xFFFE_0000;
pub const PRIM_TYPE_I64:    u32 = 0xFFFE_0001;
pub const PRIM_TYPE_F64:    u32 = 0xFFFE_0002;
pub const PRIM_TYPE_BOOL:   u32 = 0xFFFE_0003;
pub const PRIM_TYPE_CHAR:   u32 = 0xFFFE_0004;
pub const PRIM_TYPE_STR:    u32 = 0xFFFE_0005;
pub const PRIM_TYPE_ARRAY:  u32 = 0xFFFE_0006;

macro_rules! define_token {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl $name {
            pub const UNRESOLVED: Self = Self(UNRESOLVED);

            #[inline]
            pub fn is_resolved(self) -> bool {
                self.0 != UNRESOLVED
            }

            /// True iff `self` points into the per-module import_table
            /// (cross-zpkg lazy reference). False for both intra-module
            /// IDs and the UNRESOLVED sentinel.
            #[inline]
            pub fn is_import(self) -> bool {
                self.0 >= IMPORT_BASE && self.0 != UNRESOLVED
            }

            /// Index into the import_table. Caller must check `is_import()`
            /// first. Panics if called on a non-import token (debug build).
            #[inline]
            pub fn import_idx(self) -> u32 {
                debug_assert!(self.is_import(), "import_idx() on non-import token");
                self.0 - IMPORT_BASE
            }
        }
    };
}

define_token!(
    /// Identifies one `Function` in `Module.functions: Vec<Function>` (per module).
    /// Resolved at load by `module.func_index[name]`.
    MethodId
);

define_token!(
    /// Identifies one `ClassDef` / `TypeDesc`. Resolved at load by
    /// `module.type_registry[name]`.
    ///
    /// **Invariant: globally unique within the process** — allocated by
    /// [`alloc_type_id_block`], never per-module from 0. See that function for
    /// why; violating this silently dispatches to another zpkg's method.
    TypeId
);

define_token!(
    /// Identifies one builtin function in the global `BUILTINS` static table
    /// (cross-module / per-process). Resolved at load by
    /// `corelib::dispatch_table::builtin_id_of(name)`. Resolution is mandatory
    /// (closed set); a miss is a bug (unknown builtin name).
    BuiltinId
);

define_token!(
    /// Identifies one field slot in a specific `TypeDesc.fields: Vec<FieldSlot>`
    /// (per type). Stored inside `FieldIC` as the cached slot for a
    /// `FieldGet` / `FieldSet` site after the receiver-type IC fires.
    FieldId
);

define_token!(
    /// Identifies one static field slot in `VmContext.static_fields: Vec<Value>`
    /// (cross-module / per-VmContext). Allocated lazily via
    /// `VmContext::resolve_static_field_id(name)` so cross-zpkg static fields
    /// can be encountered in arbitrary load order.
    StaticFieldId
);

define_token!(
    /// Identifies one vtable slot in a specific `TypeDesc.vtable: Vec<...>`
    /// (per type). Stored inside `VCallIC` after the receiver-type IC fires.
    VTableSlot
);

/// fix-crosspkg-typeid-collision (2026-09-08): allocate `n` consecutive
/// **globally unique** `TypeId`s and return the first.
///
/// `TypeId` used to be handed out per module starting at 0 ("per module" was
/// even its documented contract). That is unsound, because `VCallIC` /
/// `FieldIC` — the two dispatch inline caches — key on the bare `u32`:
///
/// ```text
/// vcall_ic_lookup(ic, recv_type) → first entry whose type_id == recv_type
/// ```
///
/// Nothing re-numbers a `TypeDesc` when it crosses a zpkg boundary
/// (`VmContext::try_lookup_type` hands back the foreign `Arc` as-is), so two
/// classes from two zpkgs routinely share an id. A call site that sees both —
/// e.g. `ParallelFor.Run`'s `body.Run(i)`, reached with `CompileCuTask`
/// (z42c.semantics) *and* `SrcReadHashTask` (z42c.driver) — then dispatches the
/// second receiver into the first receiver's method. Observed for real: removing
/// one unrelated class from z42.core lined the ids up and the bootstrap chain
/// died in `File.ReadAllText(<CompilationUnit>)`. `FieldIC` has the same hazard
/// and is worse: it returns the wrong field slot, silently.
///
/// Allocating in blocks keeps a module's ids consecutive (readable in dumps)
/// while costing one atomic per module rather than one per class.
///
/// Ids live in the low band `[0, IMPORT_BASE)`; running past it is a hard error
/// rather than a wrap, because a wrap would reintroduce exactly this bug.
pub fn alloc_type_id_block(n: u32) -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT_TYPE_ID: AtomicU32 = AtomicU32::new(0);
    let first = NEXT_TYPE_ID.fetch_add(n, Ordering::Relaxed);
    assert!(
        u64::from(first) + u64::from(n) <= u64::from(IMPORT_BASE),
        "TypeId space exhausted: tried to allocate {n} ids starting at {first} \
         (limit {IMPORT_BASE:#x}); ids must stay below IMPORT_BASE"
    );
    first
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
