//! `IrType` — runtime type tag for each register value.
//!
//! Mirror of `src/compiler/z42.IR/IrModule.cs::IrType` (C# `enum byte`).
//! Wire format: a single `u8` per register, embedded in the zbc REGT
//! section (introduced  — review.md C2 step 0.2, JIT type
//! specialization foundation).
//!
//! # Why a runtime mirror
//!
//! The C# compiler attaches an `IrType` to every `TypedReg` during
//! codegen. Prior to this work, `ZbcWriter.Instructions.cs::WriteReg`
//! emitted only `(ushort)reg.Id`, dropping the `Type` field at the
//! binary boundary. Without per-register type info on the Rust side,
//! the JIT translator cannot specialize arithmetic / comparison /
//! logical ops on known primitive types — every op routes through a
//! C helper that pays match-dispatch cost.
//!
//! Phase 0 (this commit): expose the enum + a `Function.reg_types`
//! field that's empty for `.zbc` files not carrying REGT. Reader
//! tolerates absence (legacy fixtures keep parsing).
//!
//! Phase 0.3+ (next commits): bump zbc minor; ZbcWriter emits REGT;
//! reader populates the field; JIT translate.rs starts consuming it
//! for native Cranelift emission.
//!
//! # Invariants
//!
//! * Wire encoding: `u8` matching the enum discriminant below
//!   (`Unknown = 0`, then ASCII-order I8/I16/.../Void).
//! * Decode of an unrecognised byte → [`IrType::Unknown`] (forward
//!   compat — adding a new variant on the writer side doesn't break
//!   older readers).
//! * `Function.reg_types[i]` is the static type of register `i`
//!   inside that function; length == `Function.max_reg` (or 0 if no
//!   REGT was decoded).
//!
//! # Why not nest in `bytecode::Reg`
//!
//! `Reg` stays a bare `u32` so every `Instruction` field accessor
//! ( `dst: Reg`, `args: Box<[Reg]>`, etc.) keeps its existing shape.
//! The per-function `reg_types` table is consulted only by code that
//! needs the type — predominantly the JIT translator. Hot interp
//! lookups (`frame.regs[i]`) pay zero overhead vs the pre-IrType
//! world.

/// Runtime type tag for each register value.
///
/// Byte-compatible with the C# `IrType : byte` declared in
/// `src/compiler/z42.IR/IrModule.cs`. Each variant's discriminant
/// must match the C# side so the wire format stays consistent.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IrType {
    #[default]
    Unknown = 0,
    I8      = 1,
    I16     = 2,
    I32     = 3,
    I64     = 4,
    U8      = 5,
    U16     = 6,
    U32     = 7,
    U64     = 8,
    F32     = 9,
    F64     = 10,
    Bool    = 11,
    Char    = 12,
    Str     = 13,
    /// Any heap object (class instance, array, list, dict, null).
    Ref     = 14,
    Void    = 15,
}

impl IrType {
    /// Decode a single byte from the zbc REGT section. Unrecognised
    /// values map to [`IrType::Unknown`] (forward compat — newer
    /// writers can add variants without breaking older readers).
    #[inline]
    pub const fn from_u8(b: u8) -> Self {
        match b {
            1  => IrType::I8,
            2  => IrType::I16,
            3  => IrType::I32,
            4  => IrType::I64,
            5  => IrType::U8,
            6  => IrType::U16,
            7  => IrType::U32,
            8  => IrType::U64,
            9  => IrType::F32,
            10 => IrType::F64,
            11 => IrType::Bool,
            12 => IrType::Char,
            13 => IrType::Str,
            14 => IrType::Ref,
            15 => IrType::Void,
            _  => IrType::Unknown,
        }
    }

    /// `true` for `I64` only — the most common z42 integer type since
    /// the VM stores every narrow integer (I8..U64) as `Value::I64`
    /// internally. JIT specialization consults this to pick the
    /// `iadd`/`isub`/... emit path.
    #[inline]
    pub const fn is_i64(self) -> bool {
        matches!(self, IrType::I64)
    }

    /// `true` for the floating-point variants (`F32`, `F64`).
    #[inline]
    pub const fn is_float(self) -> bool {
        matches!(self, IrType::F32 | IrType::F64)
    }

    /// `true` for any 64-bit integer (signed or unsigned) — Cranelift
    /// `iadd`/`isub` operate the same way regardless of signedness.
    #[inline]
    pub const fn is_integer(self) -> bool {
        matches!(
            self,
            IrType::I8 | IrType::I16 | IrType::I32 | IrType::I64
            | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64
        )
    }
}

#[cfg(test)]
#[path = "reg_type_tests.rs"]
mod ir_type_tests;
