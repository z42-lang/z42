//! ClassDesc / FieldDesc / 布局描述 / 约束束 / CLASS_FLAG_* · METHOD_FLAG_*。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
use crate::metadata::bytecode_serde::{typed_reg_serde, typed_reg_vec_serde, typed_reg_opt_serde};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Class descriptor — field layout for object allocation.
///
/// review.md E5 follow-up (2026-05-27): three immutable-after-construction
/// fields stored as `Box<[T]>` (16 B) instead of `Vec<T>` (24 B) — saves
/// 8 B/field/ClassDesc. TypeDesc still owns growable `Vec`s because the
/// cross-zpkg fixup pass rebuilds them.
/// add-reflection-type-flags (zbc 1.12): bit layout for the TYPE-section class
/// flags byte (`ClassDesc::class_flags` / `TypeDesc::class_flags`). Must match
/// ZbcWriter.BuildTypeSection (1=abstract, 2=sealed, 4=struct, 8=record).
pub const CLASS_FLAG_ABSTRACT: u8 = 1 << 0;
pub const CLASS_FLAG_SEALED: u8 = 1 << 1;
pub const CLASS_FLAG_STRUCT: u8 = 1 << 2;
pub const CLASS_FLAG_RECORD: u8 = 1 << 3;
/// add-reflection-interface-class-predicates (zbc 1.19): set on the minimal
/// TYPE entry emitted for an `interface`. Backs `Type.IsInterface`; excluded
/// from `Type.IsClass`.
pub const CLASS_FLAG_INTERFACE: u8 = 1 << 4;
/// add-enum-type-metadata (zbc 1.22): set on the TYPE entry emitted for an
/// `enum`. Backs `Type.IsEnum`; when set, the class record carries a trailing
/// enum-member block (member_count:u16 + (name_idx:u32, value:i64)×n).
pub const CLASS_FLAG_ENUM: u8 = 1 << 5;

/// add-delegate-metadata (unify P1-e, zbc 1.26): the class record describes a
/// `delegate` (delegate-as-class: TYPE entry + synthesized `<FQ>.Invoke` dead
/// stub carries the signature). Backs `Type.IsDelegate`. No extra payload.
pub const CLASS_FLAG_DELEGATE: u8 = 1 << 6;
/// add-struct-heap-inline (P3b): the class declares ≥1 **inline value-struct field**
/// (`class C { Point pt; }`). Gates the zbc 1.32 TYPE-section inline-layout block
/// (same shape as the struct block) carrying the object's composed inline byte region
/// size + reference bitmap. `TypeDescCold::inline_layout`; consumed by `ScriptObject`
/// alloc + inline field access. Last free `class_flags` bit.
pub const CLASS_FLAG_HAS_INLINE_STRUCT: u8 = 1 << 7;

/// SIGS `method_flags` bits (add-method-modifiers, unify P1-c). Backs
/// `MethodInfo.IsVirtual` (authoritative) / `IsAbstract`. `static` is NOT here
/// — it stays in the dedicated `is_static` byte (single source of truth).
pub const METHOD_FLAG_VIRTUAL: u8 = 1 << 0;
pub const METHOD_FLAG_ABSTRACT: u8 = 1 << 1;
/// impl-sealed-semantics-devirt (zbc 1.30): a `sealed override` (or the shorthand
/// `sealed`) method — no further override permitted. Backs `MethodInfo.IsSealed`;
/// consumed by the compiler for `sealed`-receiver devirtualization. A sealed method
/// is always virtual, so `METHOD_FLAG_VIRTUAL` is set alongside this bit.
pub const METHOD_FLAG_SEALED: u8 = 1 << 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassDesc {
    pub name: String,
    #[serde(default)]
    pub base_class: Option<String>,
    pub fields: Box<[FieldDesc]>,
    /// Generic type parameter names: ["T"], ["K", "V"]. Empty for non-generic classes.
    #[serde(default)]
    pub type_params: Box<[String]>,
    /// L3-G3a: constraint bundle per type parameter. When non-empty must align with
    /// `type_params` by index. Absent entries in old zbc deserialise as empty box.
    #[serde(default)]
    pub type_param_constraints: Box<[ConstraintBundle]>,
    /// C3 add-attribute-reflection: user attributes applied to this class.
    /// Each points at a synthesized factory function the runtime calls (lazily,
    /// cached) to build the attribute instance for `Type.GetCustomAttributes()`.
    #[serde(default)]
    pub attributes: Box<[AttributeRef]>,
    /// add-reflection-type-flags (zbc 1.12): class-shape flags (see
    /// CLASS_FLAG_* above). Threaded into `TypeDesc::class_flags` for
    /// `Type.IsAbstract` / `Type.IsSealed` reflection.
    #[serde(default)]
    pub class_flags: u8,
    /// complete-class-access-control (zbc 1.33 visibility byte): class-declaration
    /// visibility (0=public / 1=private / 2=protected / 3=internal). Threaded into
    /// `TypeDesc::visibility` for `Type.IsPublic` / `IsNestedPrivate` etc. reflection.
    #[serde(default)]
    pub visibility: u8,
    /// add-reflection-static-fields (zbc 1.13): the class's static fields
    /// (separate from `fields`, which is the instance layout). Threaded into
    /// `TypeDescCold::static_fields`; surfaced by `Type.GetFields()` with
    /// `FieldInfo.IsStatic = true`.
    #[serde(default)]
    pub static_fields: Box<[FieldDesc]>,
    /// add-reflection-get-interfaces (zbc 1.17): the interface names this class
    /// directly declares (bare; e.g. "IFoo"). Threaded into
    /// `TypeDescCold::interfaces`; surfaced by `Type.GetInterfaces()` (which
    /// base-walks for inherited interfaces).
    #[serde(default)]
    pub interfaces: Box<[String]>,
    /// add-enum-type-metadata (zbc 1.22): enum member (name, i64 value) pairs,
    /// present only when `class_flags & CLASS_FLAG_ENUM`. Threaded into
    /// `TypeDesc::enum_members`; surfaced by `Type.IsEnum` / `Enum.GetNames` /
    /// `Enum.GetValues` / `Enum.GetName`. Empty for non-enum classes.
    #[serde(default)]
    pub enum_members: Box<[(String, i64)]>,
    /// add-interface-member-reflection (surfaces the zbc 1.28 interface method
    /// block, previously parsed-and-discarded): the interface's directly-declared
    /// method signatures, present only when `class_flags & CLASS_FLAG_INTERFACE`.
    /// Threaded into `TypeDesc::iface_methods`; surfaced by `Type.GetMethods()`.
    /// Empty for non-interface classes.
    #[serde(default)]
    pub iface_methods: Box<[IfaceMethodSig]>,
    /// add-struct-value-semantics (A-use): the value-struct byte + reference
    /// layout, present only when `class_flags & CLASS_FLAG_STRUCT` (parsed from
    /// the zbc TYPE-section struct block). Threaded into
    /// `TypeDescCold::struct_layout`; consumed by `StructAlloc` to size + scan
    /// blobs. `None` for non-struct classes and old zbc without the block.
    #[serde(default)]
    pub struct_layout: Option<StructLayoutDesc>,
    /// add-struct-heap-inline (P3b): the class's **composed inline-struct layout**
    /// (object-relative byte region size + reference bitmap of all inline struct
    /// fields), present only when `class_flags & CLASS_FLAG_HAS_INLINE_STRUCT` (zbc
    /// 1.32 inline block). Threaded into `TypeDescCold::inline_layout`; consumed by
    /// `ScriptObject` alloc + inline field access. `None` for classes with no inline
    /// struct fields. Reuses `StructLayoutDesc` (identical byte-blob + ref-bitmap shape).
    #[serde(default)]
    pub inline_layout: Option<StructLayoutDesc>,
    /// unify-object-byte-layout (PR-1): the class's **full object field layout** —
    /// every direct field's (byte offset, size, kind) at 8-byte reference width (the
    /// C#-equivalent endpoint), plus the flattened 8B reference bitmap (including
    /// inline-struct interior ref leaves). Present for normal reference classes (not
    /// struct / interface / enum / delegate — the zbc 1.34 object block). **Dormant in
    /// PR-1**: threaded into `TypeDescCold::object_layout` but not consumed (runtime
    /// still uses `slots`); PR-2 switches field storage to this byte layout. `None` for
    /// value/interface/enum/delegate types and old zbc without the block.
    #[serde(default)]
    pub object_layout: Option<ObjectLayoutDesc>,
}

/// unify-object-byte-layout (PR-1): serialized full-object field layout carried in
/// `ClassDesc` (parsed from the zbc 1.34 TYPE-section object block). `size` = total
/// object byte-region size (8B references); `field_*` = each **direct** field's byte
/// offset / size / kind (`STRUCT_REF_*` for refs, else prim/struct), parallel arrays in
/// declaration order; `ref_offsets` / `ref_kinds` = flattened 8B reference bitmap
/// (includes inline-struct interior ref leaves), parallel arrays. Own fields only —
/// inheritance base-offset composition happens at consume time (PR-2), mirroring
/// `fields = base.fields ++ own_fields`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ObjectLayoutDesc {
    pub size: u32,
    #[serde(default)]
    pub field_offsets: Box<[u32]>,
    #[serde(default)]
    pub field_sizes: Box<[u32]>,
    #[serde(default)]
    pub field_kinds: Box<[u8]>,
    #[serde(default)]
    pub ref_offsets: Box<[u32]>,
    #[serde(default)]
    pub ref_kinds: Box<[u8]>,
}

/// add-struct-value-semantics (A-use): serialized value-struct layout carried in
/// `ClassDesc` (parsed from the zbc TYPE-section struct block). `size` = byte-blob
/// size; `ref_offsets` / `ref_kinds` = each reference leaf's byte offset + kind
/// (`STRUCT_REF_*`), parallel arrays. Pure-primitive structs have empty ref arrays.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructLayoutDesc {
    pub size: u32,
    #[serde(default)]
    pub ref_offsets: Box<[u32]>,
    #[serde(default)]
    pub ref_kinds: Box<[u8]>,
}

/// add-interface-member-reflection: one interface-declared method signature,
/// recovered from the zbc 1.28 interface method block. Interface methods have no
/// backing `Function` (no body), so reflection builds their `MethodInfo` straight
/// from this signature (name / return type / parameter types).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfaceMethodSig {
    /// Source-level method name (may carry a `$N$types` dispatch-mangle suffix;
    /// reflection strips it for the user-facing `MethodInfo.Name`).
    pub name: String,
    /// Return type name (e.g. "double" / "void" / "geometry.Point").
    pub ret_type: String,
    /// Parameter type names, in declaration order (no `this`).
    pub param_types: Box<[String]>,
}

/// C3 add-attribute-reflection: one applied attribute — the attribute class's
/// qualified name plus the qualified name of the compiler-synthesized
/// `() => new T(args)` factory function (resolved against the func index).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeRef {
    pub type_name: String,
    pub factory_func: String,
}

/// Resolved constraint bundle for one generic type parameter. (L3-G3a, L3-G2.5 bare-tp)
/// Mirrors the C# `GenericConstraintBundle` on the semantic layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConstraintBundle {
    #[serde(default)]
    pub requires_class: bool,
    #[serde(default)]
    pub requires_struct: bool,
    #[serde(default)]
    pub base_class: Option<String>,
    #[serde(default)]
    pub interfaces: Vec<String>,
    /// L3-G2.5 bare-typeparam: name of another type parameter in the same decl
    /// that this parameter must be a subtype of. None when no such constraint.
    #[serde(default)]
    pub type_param_constraint: Option<String>,
    /// L3-G2.5 ctor: `where T: new()` — type arg must have a no-arg constructor.
    #[serde(default)]
    pub requires_constructor: bool,
    /// L3-G2.5 enum: `where T: enum` — type arg must be an enum type.
    #[serde(default)]
    pub requires_enum: bool,
    /// add-generic-func-constraint (2026-05-11): function-type signature.
    /// `params` are IR type-name strings (e.g. "int", "string", "Cat"); `ret` is
    /// likewise a type name ("void" / "int" / etc.). None when no func constraint.
    #[serde(default)]
    pub func_signature: Option<FuncSigDescriptor>,
}

/// add-generic-func-constraint (2026-05-11): per-tp function signature spelled
/// as type-name strings (so zbc serialization is uniform with other constraint
/// fields that hold class/interface names).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FuncSigDescriptor {
    pub params: Vec<String>,
    pub ret: String,
}

impl ConstraintBundle {
    pub fn is_empty(&self) -> bool {
        !self.requires_class && !self.requires_struct
            && self.base_class.is_none() && self.interfaces.is_empty()
            && self.type_param_constraint.is_none()
            && !self.requires_constructor
            && !self.requires_enum
            && self.func_signature.is_none()
    }
}

/// A single field in a class descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDesc {
    pub name: String,
    #[serde(rename = "type")]
    pub type_tag: String,
    /// add-field-attribute-reflection (zbc 1.14): user attributes applied to
    /// this field. Surfaced by `FieldInfo.GetCustomAttributes()` (the loader
    /// indexes these into `TypeDescCold::field_attributes`).
    #[serde(default)]
    pub attributes: Box<[AttributeRef]>,
    /// add-member-visibility (zbc 1.23): 0=public / 1=private / 2=protected.
    /// Surfaced by `FieldInfo.IsPublic` / `IsPrivate`. Default 0 (public).
    #[serde(default)]
    pub visibility: u8,
}
