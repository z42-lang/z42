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

/// fix-generic-typeparam-field-zero: per-instantiation zero values for **type-parameter
/// fields**.
///
/// `alloc_object` zero-initialises from the composed layout, which is right for every
/// concretely-typed field. But a field declared as a type parameter (`class GBox<T> {
/// public T V; }`) is classified as a **reference** slot — the layout is computed from
/// the *declaration*, where `T` is not a primitive — so its zero is `Value::Null`.
/// `GBox<int>().V` then reads `Null` instead of `0`, which breaks the
/// `enforce-value-type-non-null` invariant "a value-type slot never holds `Value::Null`"
/// (and read out through `==` it even disagreed between interp and JIT).
///
/// The instance knows its arguments (`ObjNew` carries them; see `set_type_args`), so the
/// slot's real zero is recoverable at the allocation point: map the field's declared
/// `type_tag` onto `TypeDesc::type_params()` by name, then take
/// `default_value_for(type_args[i])`.
///
/// **Deliberately narrow — only PRIMITIVE value arguments produce an override**, the same
/// line `ArrayNew` already draws (`interp/exec_array.rs`): a resolved *struct* argument
/// must NOT be forced through struct backing here, because generic containers store
/// structs by reference (`struct_generic_container`: `VCall: expected object, got
/// StructRefHeap`); and a reference argument's zero is `Null` already, so there is
/// nothing to write. Both cases return no override and keep the pre-change path exactly.
///
/// Returns `(slot_index, zero)` pairs for the caller to write via `set_field_value`.
/// Empty for non-generic types, for missing/short `type_args`, and whenever the argument
/// is not a primitive value type — so callers can apply it unconditionally.
/// complete-generic-class-identity P1: a constructed instantiation's **name is its type
/// argument list** — `Demo.Box<int>` carries `["int"]`, `Demo.Pair<int,Demo.P2>` carries
/// `["int", "Demo.P2"]`. Nested `<…>` stay inside their argument.
///
/// Parsing them here makes the name the single source of truth. The compiler's `ObjNew`
/// deliberately ships **no** separate `type_args` list once the class name carries them
/// (see `CallEmitter`: passing both renders `Demo.Box<int><int>`), so reflection
/// (`Type.GetGenericArguments`) and generic field zero-init would otherwise see nothing.
///
/// Returns an empty list for a non-instantiated name, which is the common case.
pub fn type_args_from_name(name: &str) -> Box<[String]> {
    let Some(lt) = name.find('<') else { return Box::new([]) };
    let inner = match name.strip_suffix('>') {
        Some(s) => &s[lt + 1..],
        None => return Box::new([]),
    };
    let mut out: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(inner[start..i].trim().to_string());
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out.into_boxed_slice()
}

pub fn generic_field_zero_overrides(td: &TypeDesc, type_args: &[String]) -> Vec<(usize, Value)> {
    if type_args.is_empty() {
        return Vec::new();
    }
    let params = td.type_params();
    if params.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (slot, f) in td.fields.iter().enumerate() {
        // The field's declared type is a type parameter exactly when its tag is one of
        // the declaring type's parameter names (`"T"`), not a type name.
        let Some(pi) = params.iter().position(|p| p.as_str() == &*f.type_tag) else {
            continue;
        };
        // Short `type_args` happens for partially-applied / erased call sites; leaving
        // those alone is the pre-change behaviour.
        let Some(concrete) = type_args.get(pi) else { continue };
        let zero = default_value_for(concrete);
        if !matches!(zero, Value::Null) {
            out.push((slot, zero));
        }
    }
    out
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
