//! FieldSlot + 类型标签（TAG_*）与默认值。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;


/// A single field slot in a class layout (runtime representation).
///
/// review.md E2.P2 Step 1 (2026-05-27): `Box<str>` (16 B per field) instead
/// of `String` (24 B; the `cap` word is dead weight — slot fields are
/// immutable after `build_type_registry`). Saves 16 B per FieldSlot
/// (48 B → 32 B). Full E2.P2 target (48 B → 16 B with `name_id: StringId`
/// + `type_id: TypeId` + `offset` + `flags`) waits on StringId Phase B+
/// migration and a zbc minor bump.
#[derive(Debug, Clone)]
pub struct FieldSlot {
    pub name: Box<str>,
    /// Type tag from zbc (e.g. `"int"`, `"long"`, `"bool"`, `"f64"`, `"str"`,
    /// `"Demo.Box"`, …). Used by `ObjNew` to pick a per-type default `Value`
    /// for fields that have no explicit initializer.
    /// 2026-05-02 fix-class-field-default-init.
    pub type_tag: Box<str>,
    /// Member visibility (add-member-visibility, unify P1-b): 0=public /
    /// 1=private / 2=protected. Carried from the TYPE section's per-field
    /// `visibility:u8` so `FieldInfo.IsPublic` can report it via reflection.
    /// Defaults to 0 (public) for synthesized slots (gc / exception / tests).
    pub visibility: u8,
}

/// Returns the default `Value` for a field whose declared type tag is
/// `type_tag`. Mirrors the C# `EmitStaticInit` defaults. Used by `ObjNew`
/// (interp + JIT) to initialise fields without an explicit initializer.
///
/// Reference / unknown types fall back to `Null`. `char` follows the existing
/// "char-as-i64" representation (no separate `Value::Char` variant).
///
/// Three primitive vocabularies reach here: field-slot keywords (`int`), function
/// signature tags (`i32`), and — since fix-type-reflection-names — the FQ wrapper
/// names (`Std.Int32`), which reflective `MakeGenericMethod(typeof(int)).Invoke`
/// threads into `method_type_args` (the resolved Type arg's handle name). All three
/// must yield the same value-type zero so reflective `default(T)` matches a direct
/// `Zero<int>()` call.
pub fn default_value_for(type_tag: &str) -> Value {
    match type_tag {
        "int" | "long" | "short" | "byte" | "sbyte" | "ushort" | "uint" | "ulong"
        | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        | "isize" | "usize"
        | "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte" | "Std.Byte"
        | "Std.UInt16" | "Std.UInt32" | "Std.UInt64" => Value::I64(0),
        "double" | "float" | "f32" | "f64" | "Std.Double" | "Std.Single" => Value::F64(0.0),
        "bool" | "Std.Boolean" => Value::Bool(false),
        "char" | "Std.Char" => Value::Char('\0'),
        _ => Value::Null,
    }
}

// ── zbc TypeTag bytes (mirror of C# Opcodes.TypeTags) ────────────────────────
//
// Single source of truth for the 1-byte type tag carried in instruction
// headers / extra fields. Keep these in sync with
// src/compiler/z42.IR/BinaryFormat/Opcodes.cs `TypeTags`.

pub const TAG_UNKNOWN: u8 = 0x00;
pub const TAG_BOOL:    u8 = 0x01;
pub const TAG_I8:      u8 = 0x02;
pub const TAG_I16:     u8 = 0x03;
pub const TAG_I32:     u8 = 0x04;
pub const TAG_I64:     u8 = 0x05;
pub const TAG_U8:      u8 = 0x06;
pub const TAG_U16:     u8 = 0x07;
pub const TAG_U32:     u8 = 0x08;
pub const TAG_U64:     u8 = 0x09;
pub const TAG_F32:     u8 = 0x0A;
pub const TAG_F64:     u8 = 0x0B;
pub const TAG_CHAR:    u8 = 0x0C;
pub const TAG_STR:     u8 = 0x0D;
pub const TAG_OBJECT:  u8 = 0x20;
pub const TAG_ARRAY:   u8 = 0x21;

/// Returns the default `Value` for a slot whose declared element type tag
/// is `tag`. Mirrors `default_value_for(&str)` but keyed on the wire byte
/// directly (no string lookup). Used by `ArrayNew` (interp + JIT) to
/// initialise array elements without an explicit literal.
///
/// fix-array-default-init, 2026-05-18.
pub fn default_value_for_tag(tag: u8) -> Value {
    match tag {
        TAG_BOOL => Value::Bool(false),
        TAG_I8 | TAG_I16 | TAG_I32 | TAG_I64
      | TAG_U8 | TAG_U16 | TAG_U32 | TAG_U64 => Value::I64(0),
        TAG_F32 | TAG_F64 => Value::F64(0.0),
        TAG_CHAR => Value::Char('\0'),
        _ => Value::Null,
    }
}
