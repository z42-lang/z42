//! Package formats, decoded IR, static queries and interpreter fusion.
//! Static concerns are grouped under format, reader, ir, index and debug.

pub mod image;
pub mod format;
pub mod reader;
pub mod ir;
pub mod index;
pub mod debug;
pub mod superinstr;

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
