//! 指令载荷结构（*Insn）与 Reg。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
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
// See docs/internals/src/formats/ir.md (hot/cold boxing strategy).

/// Payload for [`Instruction::Call`].
#[derive(Debug)]
pub struct CallInsn {
    pub dst: Reg,
    pub func: String,
    pub args: Box<[Reg]>,
    /// add-generic-methods: resolved FQ type-argument names for a generic method
    /// call `Foo<A,B>()`. Empty for non-generic calls. Copied into the callee's
    /// `Frame.method_type_args` at frame construction; read by `MethodTypeArg` /
    /// `MethodDefault` in the callee body.
    pub method_type_args: Box<[String]>,
}

/// Payload for [`Instruction::ArrayNew`] (add-reflection-array-element-type).
#[derive(Debug)]
pub struct ArrayNewInsn {
    pub dst: Reg,
    pub size: Reg,
    pub elem_tag: u8,
    /// Element type's FQ name (e.g. "int" / "geometry.Point"), resolved from the
    /// string pool at decode and **interned** there ([`ElemType`](crate::metadata::types::ElemType)), so creating the
    /// array copies an 8 B handle instead of allocating a name. Stored on the array's
    /// `ArrayObj` so `arr.GetType().GetElementType()` is non-erased. Empty = absent (legacy).
    pub element_type: crate::metadata::types::ElemType,
    /// add-escape-analysis-stack-alloc (zbc 1.29): escape analysis proved this
    /// array does not escape its creating frame → interp allocates it in the
    /// frame arena (GC-skipped). JIT ignores this flag (heap-allocates) in v1.
    pub stack_alloc: bool,
    /// fix-generic-array-value-zero-init (zbc 1.37, 方案 C): when the element is a
    /// generic type parameter, `type_param_kind` is 1 (method-level) or 2 (class-level)
    /// and `type_param_index` is its param index; the VM resolves it to a concrete type
    /// at runtime (frame.method_type_args / receiver.type_args) so value-type slots get
    /// the type's zero, not Null. `kind == 0` / `index == -1` for non-generic elements.
    pub type_param_kind: u8,
    pub type_param_index: i32,
}

/// Payload for [`Instruction::ArrayNewLit`] (add-reflection-array-element-type).
#[derive(Debug)]
pub struct ArrayNewLitInsn {
    pub dst: Reg,
    pub elems: Box<[Reg]>,
    /// Interned element type FQ name (see [`ArrayNewInsn::element_type`]).
    pub element_type: crate::metadata::types::ElemType,
    /// add-escape-analysis-stack-alloc (zbc 1.29): non-escaping → frame arena (interp).
    pub stack_alloc: bool,
}

/// Payload for [`Instruction::Builtin`].
#[derive(Debug)]
pub struct BuiltinInsn {
    pub dst: Reg,
    pub name: String,
    pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::LoadFn`].
#[derive(Debug)]
pub struct LoadFnInsn {
    pub dst: Reg,
    pub func: String,
}

/// Payload for [`Instruction::MkClos`].
#[derive(Debug)]
pub struct MkClosInsn {
    pub dst: Reg,
    pub fn_name: String,
    pub captures: Box<[Reg]>,
}

/// Payload for [`Instruction::ObjNew`].
#[derive(Debug)]
pub struct ObjNewInsn {
    pub dst: Reg,
    pub class_name: String,
    pub ctor_name: String,
    pub args: Box<[Reg]>,
    /// Resolved generic type-arguments for this allocation (e.g. `["int"]` for
    /// `new Foo<int>()`); empty for non-generic. `Box<[String]>` (immutable IR).
    pub type_args: Box<[String]>,
    /// add-escape-analysis-stack-alloc (zbc 1.29): escape analysis proved this
    /// object does not escape AND its ctor does not leak `this` → interp allocates
    /// it in the frame arena (GC-skipped). JIT ignores this flag (heap) in v1.
    pub stack_alloc: bool,
    /// encode-ctorless-objnew (zbc 1.39): **positive** marker — the compiler saw
    /// `ctor_name` among (every function this package emitted) ∪ `DependencyIndex`
    /// when it assembled the package. Lets the runtime tell "the constructor should
    /// be here but resolves nowhere" (dependency version skew) apart from "this
    /// class simply has no constructor". Absence is the conservative state: an
    /// unset bit reproduces the pre-1.39 behaviour exactly.
    pub ctor_known: bool,
}

/// Payload for [`Instruction::Typeof`].
///
/// `typeof(T)` reflection. `type_name` is the FQ definition name
/// (`make_type_from_name`-resolvable); `type_args` are the FQ names of the
/// instantiation type arguments (`typeof(Box<int>)` → `["int"]`; empty for
/// non-generic / open). A non-empty list marks a *constructed* generic type.
/// add-reflection-generic-type-definition (zbc 1.18).
#[derive(Debug)]
pub struct TypeofInsn {
    pub dst: Reg,
    pub type_name: String,
    pub type_args: Box<[String]>,
}

/// Payload for [`Instruction::FieldGet`].
#[derive(Debug)]
pub struct FieldGetInsn {
    pub dst: Reg,
    pub obj: Reg,
    pub field_name: String,
}

/// Payload for [`Instruction::FieldSet`].
#[derive(Debug)]
pub struct FieldSetInsn {
    pub obj: Reg,
    pub field_name: String,
    pub val: Reg,
}

/// Payload for [`Instruction::VCall`].
#[derive(Debug)]
pub struct VCallInsn {
    pub dst: Reg,
    pub obj: Reg,
    pub method: String,
    pub args: Box<[Reg]>,
    /// add-generic-methods: resolved FQ type-argument names for a generic instance
    /// method call. Empty for non-generic. See `CallInsn::method_type_args`.
    pub method_type_args: Box<[String]>,
}

/// Payload for [`Instruction::IsInstance`].
#[derive(Debug)]
pub struct IsInstanceInsn {
    pub dst: Reg,
    pub obj: Reg,
    pub class_name: String,
    /// Runtime-only: `class_name`'s type-test key, resolved on first use
    /// (see [`TypeKeyCell`](crate::metadata::tokens::TypeKeyCell)).
    pub target: crate::metadata::tokens::TypeKeyCell,
}

/// Payload for [`Instruction::AsCast`].
#[derive(Debug)]
pub struct AsCastInsn {
    pub dst: Reg,
    pub obj: Reg,
    pub class_name: String,
    /// Runtime-only: `class_name`'s type-test key, resolved on first use
    /// (see [`TypeKeyCell`](crate::metadata::tokens::TypeKeyCell)).
    pub target: crate::metadata::tokens::TypeKeyCell,
}

/// Payload for [`Instruction::StaticGet`].
#[derive(Debug)]
pub struct StaticGetInsn {
    pub dst: Reg,
    pub field: String,
}

/// Payload for [`Instruction::StaticSet`].
#[derive(Debug)]
pub struct StaticSetInsn {
    pub field: String,
    pub val: Reg,
}

/// Payload for [`Instruction::CallNative`].
#[derive(Debug)]
pub struct CallNativeInsn {
    pub dst: Reg,
    pub module: String,
    pub type_name: String,
    pub symbol: String,
    pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::StructAlloc`] (add-struct-value-semantics Phase A).
#[derive(Debug)]
pub struct StructAllocInsn {
    pub dst: Reg,
    /// FQ value-type name — the arena records it so GC can scan blob reference
    /// leaves by the type's ref-bitmap, and boxing can recover the precise type.
    pub type_name: String,
    /// Blob size in bytes (StructLayout.size).
    pub size: u32,
}

/// Payload for [`Instruction::StructFieldGetPrim`] (symbolic-struct-field-access P2, 方案 A).
///
/// ⚠️ **`root_type` 不只是「所属类型」，它同时是编号空间的判别器。**
/// 这条指令此前携带一个**烘焙好的字节偏移**，而那个偏移活在**两个互不相容的编号空间**里
/// —— struct blob 相对（`StructRef`/`BoxedStruct`/`StructRefHeap`）或 composed 对象相对
/// （`Object`/`StackObject`）—— 而**指令里一个字都没记是哪个**：正确性靠编译器
/// （`_isInlineStructFieldRoot`）与运行时（按 `Value` 变体分派）各自独立地同意。
/// 那是结构审计 R2「判据复制」在 struct 路径上的实例，也是 P2 真正要消掉的东西
/// （符号化的**性能**是中性的 —— 字段访问在 JIT 里恒是 helper 调用，偏移只是个 `iconst` 实参）。
///
/// `root_type` 解析成 **class** ⇒ 第一级索引进 `composed_object_layout().field_offsets`；
/// 解析成 **struct** ⇒ 进 `struct_layout().fields`。名字本身就回答了「是哪个空间」。
///
/// `path` 是**字段序号**路径（长度 >= 1）。用序号而非字段名，是因为**序号实例化不变、
/// 偏移才变**（`Pair<A,B>` 的 First/Second 永远 0/1）⇒ 解析是 O(1) 下标，无哈希、无字符串、
/// 不需要 IC。嵌套链（`line.a.x`）此前被编译器**求和展平**成一个立即数，现在保留为路径。
/// 实测分布：深度 1 占 85.1%，最大深度 4（见提案）。
#[derive(Debug)]
pub struct StructFieldGetInsn {
    pub dst: Reg,
    pub base: Reg,
    pub root_type: String,
    pub path: Box<[u16]>,
    pub kind: u8,
}

/// Payload for [`Instruction::StructFieldSetPrim`] — 写侧镜像，语义见
/// [`StructFieldGetInsn`]。
#[derive(Debug)]
pub struct StructFieldSetInsn {
    pub base: Reg,
    pub root_type: String,
    pub path: Box<[u16]>,
    pub kind: u8,
    pub val: Reg,
}

/// Payload for [`Instruction::LoadFieldAddr`].
#[derive(Debug)]
pub struct LoadFieldAddrInsn {
    pub dst: Reg,
    /// Reg holding the object (must be `Value::Object(GcRef<...>)`).
    pub obj: Reg,
    pub field_name: String,
}
