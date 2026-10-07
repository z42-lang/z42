/// Compiler output format parsing and runtime metadata definitions.
///
/// Submodules:
///   `types`    — runtime value types (Value, ExecMode, ObjectData)
///   `bytecode` — IR data structures (Module, Function, Instruction, Terminator)
///   `formats`  — .zbc / .zpkg magic bytes + the package dependency record
///   `merge`    — multi-module merge algorithm (string pool remap + function concat)
///   `loader`   — format-dispatch entry point: `load_artifact(path)`

pub mod types;
pub mod tokens;
/// unify-object-byte-layout PR-4: `Str` — 8-byte thin ref-counted UTF-8 string handle
/// (replaces `Value::Str(Arc<str>)`'s 16-byte fat pointer; lets `Value` reach 16 B in PR-5).
pub mod vstr;
/// review.md Part 2 C4 / C5 P1 (2026-06-01): linear-scan replacement for
/// `HashMap<String, usize>` used in `TypeDesc.field_index` /
/// `TypeDesc.vtable_index`. For typical class sizes (≤16 entries) a
/// `Vec<(Box<str>, usize)>` scan beats hash + string compare on cache
/// locality + branch prediction, and saves 8 B / entry vs `String`.
pub mod name_index;
/// review.md C2 step 0.2 (2026-05-27): `IrType` enum mirroring the C#
/// `IrType : byte` in `z42.IR/IrModule.cs`. Foundation for JIT type
/// specialization — populated per-register on the `Function` via the
/// upcoming REGT zbc section.
pub mod ir_type;
pub mod bytecode;
pub mod superinstr;
pub mod context;
pub mod formats;
pub mod zbc_reader;
pub mod loader;
pub mod namespace_index;
/// Append-only, lock-free-read segmented tables (`SegVec` / `SparseSegTable`).
pub mod seg_vec;
/// Process-level function identity: `FuncTable` (FnId → Function).
pub mod func_table;
/// Process-level type identity: `TypeTable` (TypeId → TypeDesc) + type-test name keys.
pub mod type_table;
/// VM-level string-literal identity: `StrTable` (ConstStr id → text + interned GC string).
pub mod str_table;
pub mod lazy_loader;
pub mod merge;
pub mod resolver;
pub mod well_known_names;
pub mod test_index;
pub mod build_id;

#[cfg(test)]
#[path = "constraint_tests.rs"]
mod constraint_tests;

#[cfg(test)]
#[path = "sidecar_tests.rs"]
mod sidecar_tests;

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;

// Re-exports: string pool typed handle (Part 5 P0 Phase A, 2026-05-26)
// Re-exports: NameIndex (review.md Part 2 C4 / C5 P1, 2026-06-01)
pub use name_index::NameIndex;
// Re-exports: per-register static type tag (C2 step 0.2, 2026-05-27)
pub use ir_type::IrType;

// Re-exports: runtime value types
pub use types::{default_value_for, ClosureData, ExecMode, FieldSlot, NativeData, PinSourceKind, PinnedViewData, ScriptObject, TypeDesc, Value};
#[allow(deprecated)]
pub use types::ObjectData;

// Re-exports: bytecode IR structures
pub use bytecode::{BasicBlock, BranchTargets, ClassDesc, ExceptionEntry, FieldDesc, Function, Instruction, Module, Terminator};
pub use bytecode::{
    AsCastInsn, BuiltinInsn, CallInsn, CallNativeInsn, FieldGetInsn, FieldSetInsn, IsInstanceInsn,
    LoadFieldAddrInsn, LoadFnInsn, MkClosInsn, ObjNewInsn, StaticGetInsn,
    StaticSetInsn, TypeofInsn, VCallInsn,
};

// Re-exports: package format types and artifact loading
pub use loader::{load_artifact, load_artifact_from_bytes, resolve_namespace, resolve_dependency, extract_import_namespaces, LoadedArtifact};
pub use merge::merge_modules;

// Re-exports: lazy loader (state owned by VmContext, see crate::vm_context)
pub use lazy_loader::{LazyLoader, ZpkgCandidate};

// Re-exports: test metadata (R1 add-test-metadata-section)
pub use test_index::{
    read_test_index, TestCase, TestEntry, TestEntryKind, TestFlags,
    TEST_INDEX_MAGIC, TEST_INDEX_VERSION,
};
