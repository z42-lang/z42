//! Instruction / Terminator / BranchTargets 枚举 + impl Instruction。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
use crate::metadata::bytecode_serde::{typed_reg_serde, typed_reg_vec_serde, typed_reg_opt_serde};
use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;
use std::sync::Arc;

impl Instruction {
    /// The register this instruction writes (its `dst`), or `None` for the
    /// store-only instructions (`ArraySet` / `FieldSet` / `StaticSet` /
    /// `StructFieldSetPrim` / `UnpinPtr`). Used by `Function::reg_file_len`
    /// to find the highest written register when sizing an activation frame
    /// (shared by the interp frame pre-sizing and the JIT's `max_reg`).
    pub fn written_reg(&self) -> Option<u32> {
        match self {
            Instruction::ConstStr  { dst, .. } => Some(*dst),
            Instruction::ConstI32  { dst, .. } => Some(*dst),
            Instruction::ConstI64  { dst, .. } => Some(*dst),
            Instruction::ConstF64  { dst, .. } => Some(*dst),
            Instruction::ConstBool { dst, .. } => Some(*dst),
            Instruction::ConstChar { dst, .. } => Some(*dst),
            Instruction::ConstNull { dst }      => Some(*dst),
            Instruction::Copy      { dst, .. }  => Some(*dst),
            Instruction::Add       { dst, .. }  => Some(*dst),
            Instruction::Sub       { dst, .. }  => Some(*dst),
            Instruction::Mul       { dst, .. }  => Some(*dst),
            Instruction::Div       { dst, .. }  => Some(*dst),
            Instruction::Rem       { dst, .. }  => Some(*dst),
            Instruction::Eq        { dst, .. }  => Some(*dst),
            Instruction::Ne        { dst, .. }  => Some(*dst),
            Instruction::Lt        { dst, .. }  => Some(*dst),
            Instruction::Le        { dst, .. }  => Some(*dst),
            Instruction::Gt        { dst, .. }  => Some(*dst),
            Instruction::Ge        { dst, .. }  => Some(*dst),
            Instruction::And       { dst, .. }  => Some(*dst),
            Instruction::Or        { dst, .. }  => Some(*dst),
            Instruction::Not       { dst, .. }  => Some(*dst),
            Instruction::Neg       { dst, .. }  => Some(*dst),
            Instruction::BitAnd    { dst, .. }  => Some(*dst),
            Instruction::BitOr     { dst, .. }  => Some(*dst),
            Instruction::BitXor    { dst, .. }  => Some(*dst),
            Instruction::BitNot    { dst, .. }  => Some(*dst),
            Instruction::Shl       { dst, .. }  => Some(*dst),
            Instruction::Shr       { dst, .. }  => Some(*dst),
            Instruction::StrConcat { dst, .. }  => Some(*dst),
            Instruction::ToStr     { dst, .. }  => Some(*dst),
            Instruction::Call(insn)              => Some(insn.dst),
            Instruction::LoadLocalAddr { dst, .. } => Some(*dst),
            Instruction::LoadElemAddr  { dst, .. } => Some(*dst),
            Instruction::LoadFieldAddr(insn)       => Some(insn.dst),
            Instruction::DefaultOf     { dst, .. } => Some(*dst),
            Instruction::MethodTypeArg { dst, .. } => Some(*dst),
            Instruction::MethodDefault { dst, .. } => Some(*dst),
            Instruction::Builtin(insn)          => Some(insn.dst),
            Instruction::ArrayNew(insn)          => Some(insn.dst),
            Instruction::ArrayNewLit(insn)       => Some(insn.dst),
            Instruction::ArrayGet    { dst, .. } => Some(*dst),
            Instruction::ArraySet    { .. }      => None,
            Instruction::ArrayLen    { dst, .. } => Some(*dst),
            Instruction::ObjNew(insn)           => Some(insn.dst),
            Instruction::Typeof(insn)           => Some(insn.dst),
            Instruction::FieldGet(insn)         => Some(insn.dst),
            Instruction::FieldSet(_)            => None,
            Instruction::VCall(insn)            => Some(insn.dst),
            Instruction::IsInstance(insn)       => Some(insn.dst),
            Instruction::AsCast(insn)           => Some(insn.dst),
            Instruction::StaticGet(insn)        => Some(insn.dst),
            Instruction::StaticSet(_)           => None,
            Instruction::CallNative(insn)             => Some(insn.dst),
            Instruction::CallNativeVtable { dst, .. } => Some(*dst),
            Instruction::PinPtr           { dst, .. } => Some(*dst),
            Instruction::UnpinPtr         { .. }      => None,
            Instruction::LoadFn(insn)             => Some(insn.dst),
            Instruction::LoadFnCached(insn)       => Some(insn.dst),
            Instruction::CallIndirect { dst, .. } => Some(*dst),
            Instruction::MkClos(insn)             => Some(insn.dst),
            Instruction::Convert      { dst, .. } => Some(*dst),
            // add-struct-value-semantics Phase A: blob value type instructions.
            Instruction::StructAlloc(insn)              => Some(insn.dst),
            Instruction::StructCopy { dst, .. }         => Some(*dst),
            Instruction::StructFieldGetPrim { dst, .. } => Some(*dst),
            Instruction::StructFieldSetPrim { .. }      => None,
        }
    }
}
/// SSA instructions.
/// JSON wire format: {"op": "<snake_case_name>", <named fields...>}
///
/// Register fields accept both plain integers (`42`) and TypedReg objects
/// (`{"id": 42, "type": "i32"}`) during JSON deserialization for backward
/// compatibility with both old and new compiler output.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Instruction {
    // Constants
    ConstStr  { #[serde(with = "typed_reg_serde")] dst: Reg, idx: u32 },
    ConstI32  { #[serde(with = "typed_reg_serde")] dst: Reg, val: i32 },
    ConstI64  { #[serde(with = "typed_reg_serde")] dst: Reg, val: i64 },
    ConstF64  { #[serde(with = "typed_reg_serde")] dst: Reg, val: f64 },
    ConstBool { #[serde(with = "typed_reg_serde")] dst: Reg, val: bool },
    ConstChar { #[serde(with = "typed_reg_serde")] dst: Reg, val: char },
    ConstNull { #[serde(with = "typed_reg_serde")] dst: Reg },
    Copy {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Arithmetic
    Add {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Sub {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Mul {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Div {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Rem {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    // Comparison
    Eq {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Ne {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Lt {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Le {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Gt {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Ge {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    // Logical
    And {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Or {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Not {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Unary arithmetic
    Neg {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Bitwise
    BitAnd {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    BitOr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    BitXor {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    BitNot {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    Shl {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Shr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    // String
    StrConcat {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    ToStr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Spec impl-ref-out-in-runtime: Address-load instructions producing
    // Value::Ref values. Caller emits these for `ref`/`out`/`in` arguments
    // before the Call; the Ref is passed through Call's args; callee's
    // frame.get/set transparently derefs (single dispatch point).
    LoadLocalAddr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        /// Slot in the *current* frame to point at. Codegen guarantees this
        /// is a real local register (not virtual). At runtime produces
        /// `Value::Ref { kind: RefKind::Stack { frame_idx: depth-1, slot } }`.
        #[serde(with = "typed_reg_serde")] slot: Reg,
    },
    LoadElemAddr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        /// Reg holding the array (must be `Value::Array(GcRef<...>)`).
        #[serde(with = "typed_reg_serde")] arr: Reg,
        /// Reg holding the index (must be `Value::I64`).
        #[serde(with = "typed_reg_serde")] idx: Reg,
    },
    LoadFieldAddr(Box<LoadFieldAddrInsn>),
    /// 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): runtime
    /// resolution of `default(T)` where T is a generic type-parameter of the
    /// receiver class. Reads `frame.regs[0]` (this) → `Object → type_desc.type_args[param_index]`,
    /// looks up the resolved type via `default_value_for(tag)`, writes Value to dst.
    /// Non-Object reg 0 / OOB index → graceful-degrade to `Value::Null`.
    DefaultOf {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        param_index: u8,
    },
    /// add-generic-methods: materialize a **method-level** type parameter into a
    /// concrete `Std.Type`, reading `frame.method_type_args[param_index]` (set at
    /// call time from `CallInsn::method_type_args`). Feeds `typeof(T)` (result
    /// directly) and `new T()` (via `__activator_create`). OOB/empty → placeholder
    /// constructed type (graceful, mirrors class-level Typeof placeholder).
    MethodTypeArg {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        param_index: u8,
    },
    /// add-generic-methods: method-level `default(T)` zero value — mirrors
    /// `DefaultOf` but reads `frame.method_type_args[param_index]` instead of the
    /// receiver's instance type_args. OOB/empty → `Value::Null`.
    MethodDefault {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        param_index: u8,
    },
    /// spec fix-numeric-cast-lowering (2026-05-13): explicit numeric type
    /// conversion. Target type comes from `dst`'s static type tag; source
    /// type is resolved at runtime from `src`'s `Value` variant.
    ///
    /// Covered:
    ///   - f64 → i*/u* (saturating, Rust `as` semantics; NaN → 0)
    ///   - i64 → f32/f64 (widening)
    ///   - i64 → i8/i16/i32 (low-bits + sign extend)
    ///   - i64 → u8/u16/u32 (low-bits + zero extend)
    ///   - char ↔ i32/i64 (Unicode scalar; invalid → error)
    /// Identity casts (fromIr == toIr) are not emitted — codegen returns the
    /// source register directly.
    Convert {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
        /// Target type tag (TypeTags constants — I8/I16/.../F64/Char etc.).
        /// Source type is determined at runtime from `src`'s Value variant.
        to_tag: u8,
    },
    // Calls
    Call(Box<CallInsn>),
    Builtin(Box<BuiltinInsn>),
    /// Push a function-reference value onto a register. The runtime resolves
    /// `func` at call site (current usage: L2 no-capture lambda lifted as a
    /// module-level function). See docs/design/language/closure.md §6.
    LoadFn(Box<LoadFnInsn>),
    /// 2026-05-02 add-method-group-conversion (D1b): cached method group
    /// conversion. First execution stores `Value::FuncRef(func)` into VmContext
    /// `func_ref_slots[slot_id]`; subsequent hits read from slot. Same fully-
    /// qualified `func` shares a `slot_id` across all call sites in a module.
    LoadFnCached(Box<LoadFnCachedInsn>),
    /// Indirect call via a register holding a `FuncRef` value. See
    /// docs/design/language/closure.md §6.
    CallIndirect {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] callee: Reg,
        #[serde(with = "typed_reg_vec_serde")] args: Box<[Reg]>,
    },
    /// L3 closure tier-C: allocate an env from `captures`, build a closure
    /// value and write it to `dst`. See docs/design/language/closure.md §6.
    /// `stack_alloc=true` (impl-closure-l3-escape-stack): VM 走 frame-local
    /// arena → `Value::StackClosure`；否则 heap → `Value::Closure`。
    MkClos(Box<MkClosInsn>),
    // Arrays
    /// Allocate a zero-initialised array of `size` elements. Each slot is
    /// filled with the per-type default value derived from `elem_tag`
    /// (zbc `TypeTags::*` byte): `Value::I64(0)` for numeric tags,
    /// `Value::Bool(false)` for bool, `Value::Char('\0')` for char,
    /// `Value::F64(0.0)` for float/double, `Value::Null` for ref/string/unknown.
    /// fix-array-default-init, 2026-05-18.
    /// add-reflection-array-element-type (zbc 1.16): boxed because the
    /// `element_type` String would blow the 32 B slim-instruction invariant.
    ArrayNew(Box<ArrayNewInsn>),
    /// Allocate an array from a literal list of element registers.
    ArrayNewLit(Box<ArrayNewLitInsn>),
    /// Load element at `idx` from array `arr` into `dst`. Panics on out-of-bounds.
    ArrayGet {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] arr: Reg,
        #[serde(with = "typed_reg_serde")] idx: Reg,
    },
    /// Store `val` into array `arr` at `idx`. Panics on out-of-bounds.
    ArraySet {
        #[serde(with = "typed_reg_serde")] arr: Reg,
        #[serde(with = "typed_reg_serde")] idx: Reg,
        #[serde(with = "typed_reg_serde")] val: Reg,
    },
    /// Load the length of array `arr` as i32 into `dst`.
    ArrayLen {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] arr: Reg,
    },
    // Objects
    /// Allocate a new object of `class_name`, calling overload-resolved
    /// ctor `ctor_name` (FQ, 含 `$N` suffix 如有) with `args`. VM 不再做
    /// `${class}.${simple}` 名字推断 — 直查 `func_index[ctor_name]`.
    ObjNew(Box<ObjNewInsn>),
    /// `typeof(T)` → a reflection `Std.Type` into `dst`, carrying structured
    /// generic instantiation args. add-reflection-generic-type-definition.
    Typeof(Box<TypeofInsn>),
    /// Load field `field_name` of object `obj` into `dst`.
    FieldGet(Box<FieldGetInsn>),
    /// Store `val` into field `field_name` of object `obj`.
    FieldSet(Box<FieldSetInsn>),
    /// Virtual dispatch: invoke `method` on runtime class of `obj`, walking base classes.
    VCall(Box<VCallInsn>),
    /// `expr is ClassName` — dst = true if obj's runtime type is class_name or a subclass.
    IsInstance(Box<IsInstanceInsn>),
    /// `expr as ClassName` — dst = obj if it is an instance of class_name (or subclass), else null.
    AsCast(Box<AsCastInsn>),
    /// Load the module-level static field `field` into `dst`.
    StaticGet(Box<StaticGetInsn>),
    /// Store `val` into the module-level static field `field`.
    StaticSet(Box<StaticSetInsn>),

    // Native interop (C1 scaffold; semantics by C2/C4/C5)
    /// Direct native symbol call. Resolved at load time; runtime behaviour
    /// arrives in spec C2.
    CallNative(Box<CallNativeInsn>),
    /// Native-type vtable indirect call. `vtable_slot` is filled by the C5
    /// source generator at compile time so no name lookup happens at runtime.
    CallNativeVtable {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] recv: Reg,
        vtable_slot: u16,
        #[serde(with = "typed_reg_vec_serde")] args: Box<[Reg]>,
    },
    /// Pin a String/Array buffer for FFI borrow. Pinned-view layout and
    /// lifetime semantics land in spec C4.
    PinPtr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    /// Release a pinned view created by `PinPtr`.
    UnpinPtr {
        #[serde(with = "typed_reg_serde")] pinned: Reg,
    },

    // ── blob value types (add-struct-value-semantics Phase A) ────────────────
    /// Allocate a `size`-byte blob in the per-context struct arena
    /// (zero-initialized); `dst` = `Value::StructRef` handle.
    StructAlloc(Box<StructAllocInsn>),
    /// Copy a struct blob (`size` bytes) from `src` to `dst`. Pure-primitive
    /// blobs memcpy; blobs with reference leaves clone per the type's ref-bitmap.
    StructCopy {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
        size: u32,
    },
    /// Read the primitive leaf at `byte_off` of struct blob `base` into `dst`.
    StructFieldGetPrim {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] base: Reg,
        byte_off: u32,
        kind: u8,
    },
    /// Write primitive `val` into struct blob `base` at `byte_off` (in-place lvalue).
    StructFieldSetPrim {
        #[serde(with = "typed_reg_serde")] base: Reg,
        byte_off: u32,
        kind: u8,
        #[serde(with = "typed_reg_serde")] val: Reg,
    },
}

/// Block terminator.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Terminator {
    Ret {
        #[serde(with = "typed_reg_opt_serde")]
        reg: Option<Reg>,
    },
    Br { label: String },
    BrCond {
        #[serde(with = "typed_reg_serde")] cond: Reg,
        true_label: String,
        false_label: String,
    },
    /// Throw the value in `reg` as an exception.
    Throw {
        #[serde(with = "typed_reg_serde")] reg: Reg,
    },
}

/// perf-vm-iteration: a block terminator's branch target(s) pre-resolved to
/// block **indices** at load time, so `Br`/`BrCond` become direct integer jumps
/// instead of a per-branch `HashMap<String,usize>` SipHash lookup on the label.
/// Profiling showed ~25% of interp loop time was SipHashing block labels on
/// every back-edge. Populated by `loader::build_block_indices`; runtime falls
/// back to the label `HashMap` when absent (e.g. hand-built test functions).
#[derive(Debug, Clone, Copy)]
pub enum BranchTargets {
    /// Ret / Throw — not a branch (this block's terminator never indexes here).
    NoBranch,
    /// `Br` → resolved target block index.
    Br(usize),
    /// `BrCond` → (true_label idx, false_label idx).
    BrCond(usize, usize),
}
