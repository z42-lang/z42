//! Package formats, decoded IR, static queries and VM linking state.
//! Static concerns are grouped under format, reader, ir, index and debug.

pub mod image;
pub mod format;
pub mod reader;
pub mod ir;
pub mod index;
pub mod debug;
pub mod tokens;
pub mod superinstr;
pub mod context;
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

#[cfg(test)]
#[path = "constraint_tests.rs"]
mod constraint_tests;


// Re-exports: string pool typed handle (Part 5 P0 Phase A)
// Re-exports: NameIndex (review.md Part 2 C4 / C5 P1)
pub use index::NameIndex;
// Re-exports: per-register static type tag (C2 step 0.2)
pub use ir::reg_type::IrType;

// Re-exports: bytecode IR structures
pub use ir::ExecMode;
pub use ir::{BasicBlock, BranchTargets, ClassDesc, ExceptionEntry, FieldDesc, Function, Instruction, Module, Terminator};
pub use ir::{
    AsCastInsn, BuiltinInsn, CallInsn, CallNativeInsn, FieldGetInsn, FieldSetInsn, IsInstanceInsn,
    LoadFieldAddrInsn, LoadFnInsn, MkClosInsn, ObjNewInsn, StaticGetInsn,
    StaticSetInsn, TypeofInsn, VCallInsn,
};

// Re-exports: package format types and artifact loading
pub use loader::{load_artifact, load_artifact_from_bytes, resolve_namespace, resolve_dependency, extract_import_namespaces, LoadedArtifact};
pub use merge::merge_modules;

// Re-exports: lazy loader (state owned by VmContext, see crate::vm_context)
pub use lazy_loader::{LazyLoader, ZpkgCandidate};

