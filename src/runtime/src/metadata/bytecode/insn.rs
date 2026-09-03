//! 指令载荷结构（*Insn）与 Reg。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
use crate::metadata::bytecode_serde::{typed_reg_serde, typed_reg_vec_serde, typed_reg_opt_serde};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Register index.
pub type Reg = u32;

// ── Boxed instruction payloads (slim-instruction-enum, 2026-06-11) ───────────
// Variants carrying a `String` (name-bearing, cold) keep their payload behind a
// `Box<XxxInsn>` so the `Instruction` enum stays ≤32 B (was ~120 B). Hot
// register/scalar variants remain inline. JSON wire format is unchanged: an
// internally-tagged (`tag = "op"`) newtype variant whose inner type is a struct
// merges the tag into the struct's fields, so `Call(Box<CallInsn>)` serializes
// to the same `{"op":"call", dst, func, args}` as the old struct variant.
// See docs/design/runtime/ir.md (hot/cold boxing strategy).

/// Payload for [`Instruction::Call`].
#[derive(Debug, Serialize, Deserialize)]
pub struct CallInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub func: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
    /// add-generic-methods: resolved FQ type-argument names for a generic method
    /// call `Foo<A,B>()`. Empty for non-generic calls. Copied into the callee's
    /// `Frame.method_type_args` at frame construction; read by `MethodTypeArg` /
    /// `MethodDefault` in the callee body.
    #[serde(default)] pub method_type_args: Box<[String]>,
}

/// Payload for [`Instruction::ArrayNew`] (add-reflection-array-element-type).
#[derive(Debug, Serialize, Deserialize)]
pub struct ArrayNewInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub size: Reg,
    #[serde(default)] pub elem_tag: u8,
    /// Element type's FQ name (e.g. "int" / "geometry.Point"), resolved from the
    /// string pool at decode. Stored on the array's `ArrayObj` so
    /// `arr.GetType().GetElementType()` is non-erased. Empty = absent (legacy).
    #[serde(default)] pub element_type: String,
    /// add-escape-analysis-stack-alloc (zbc 1.29): escape analysis proved this
    /// array does not escape its creating frame → interp allocates it in the
    /// frame arena (GC-skipped). JIT ignores this flag (heap-allocates) in v1.
    #[serde(default)] pub stack_alloc: bool,
    /// fix-generic-array-value-zero-init (zbc 1.37, 方案 C): when the element is a
    /// generic type parameter, `type_param_kind` is 1 (method-level) or 2 (class-level)
    /// and `type_param_index` is its param index; the VM resolves it to a concrete type
    /// at runtime (frame.method_type_args / receiver.type_args) so value-type slots get
    /// the type's zero, not Null. `kind == 0` / `index == -1` for non-generic elements.
    #[serde(default)] pub type_param_kind: u8,
    #[serde(default = "neg_one_i32")] pub type_param_index: i32,
}

fn neg_one_i32() -> i32 { -1 }

/// Payload for [`Instruction::ArrayNewLit`] (add-reflection-array-element-type).
#[derive(Debug, Serialize, Deserialize)]
pub struct ArrayNewLitInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_vec_serde")] pub elems: Box<[Reg]>,
    #[serde(default)] pub element_type: String,
    /// add-escape-analysis-stack-alloc (zbc 1.29): non-escaping → frame arena (interp).
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::Builtin`].
#[derive(Debug, Serialize, Deserialize)]
pub struct BuiltinInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub name: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::LoadFn`].
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadFnInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub func: String,
}

/// Payload for [`Instruction::LoadFnCached`].
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadFnCachedInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub func: String,
    pub slot_id: u32,
}

/// Payload for [`Instruction::MkClos`].
#[derive(Debug, Serialize, Deserialize)]
pub struct MkClosInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub fn_name: String,
    #[serde(with = "typed_reg_vec_serde")] pub captures: Box<[Reg]>,
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::ObjNew`].
#[derive(Debug, Serialize, Deserialize)]
pub struct ObjNewInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub class_name: String,
    pub ctor_name: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
    /// Resolved generic type-arguments for this allocation (e.g. `["int"]` for
    /// `new Foo<int>()`); empty for non-generic. `Box<[String]>` (immutable IR).
    #[serde(default)] pub type_args: Box<[String]>,
    /// add-escape-analysis-stack-alloc (zbc 1.29): escape analysis proved this
    /// object does not escape AND its ctor does not leak `this` → interp allocates
    /// it in the frame arena (GC-skipped). JIT ignores this flag (heap) in v1.
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::Typeof`].
///
/// `typeof(T)` reflection. `type_name` is the FQ definition name
/// (`make_type_from_name`-resolvable); `type_args` are the FQ names of the
/// instantiation type arguments (`typeof(Box<int>)` → `["int"]`; empty for
/// non-generic / open). A non-empty list marks a *constructed* generic type.
/// add-reflection-generic-type-definition (zbc 1.18).
#[derive(Debug, Serialize, Deserialize)]
pub struct TypeofInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub type_name: String,
    #[serde(default)] pub type_args: Box<[String]>,
}

/// Payload for [`Instruction::FieldGet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FieldGetInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub field_name: String,
}

/// Payload for [`Instruction::FieldSet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FieldSetInsn {
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub field_name: String,
    #[serde(with = "typed_reg_serde")] pub val: Reg,
}

/// Payload for [`Instruction::VCall`].
#[derive(Debug, Serialize, Deserialize)]
pub struct VCallInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub method: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
    /// add-generic-methods: resolved FQ type-argument names for a generic instance
    /// method call. Empty for non-generic. See `CallInsn::method_type_args`.
    #[serde(default)] pub method_type_args: Box<[String]>,
}

/// Payload for [`Instruction::IsInstance`].
#[derive(Debug, Serialize, Deserialize)]
pub struct IsInstanceInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub class_name: String,
}

/// Payload for [`Instruction::AsCast`].
#[derive(Debug, Serialize, Deserialize)]
pub struct AsCastInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub class_name: String,
}

/// Payload for [`Instruction::StaticGet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct StaticGetInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub field: String,
}

/// Payload for [`Instruction::StaticSet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct StaticSetInsn {
    pub field: String,
    #[serde(with = "typed_reg_serde")] pub val: Reg,
}

/// Payload for [`Instruction::CallNative`].
#[derive(Debug, Serialize, Deserialize)]
pub struct CallNativeInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub module: String,
    pub type_name: String,
    pub symbol: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::StructAlloc`] (add-struct-value-semantics Phase A).
#[derive(Debug, Serialize, Deserialize)]
pub struct StructAllocInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    /// FQ value-type name — the arena records it so GC can scan blob reference
    /// leaves by the type's ref-bitmap, and boxing can recover the precise type.
    pub type_name: String,
    /// Blob size in bytes (StructLayout.size).
    pub size: u32,
}

/// Payload for [`Instruction::LoadFieldAddr`].
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadFieldAddrInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    /// Reg holding the object (must be `Value::Object(GcRef<...>)`).
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub field_name: String,
}
