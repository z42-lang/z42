//! Static type predicates + operator-kind enums + field/array shape probes.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// True iff `reg_types[dst]`, `reg_types[a]`, `reg_types[b]` are all integer
/// types (`I8..U64`). Out-of-range or `Unknown` regs fall back to the slow
/// (helper-call) path.
///
/// jit-unbox-regalloc Phase 2A (2026-08-15): widened from `== I64` to
/// `is_integer()`. Every narrow integer (`I8..U64`) is physically stored as
/// `Value::I64` (payload i64 @off8), and the VM computes **all** integer
/// arithmetic/bitwise ops as signed i64 wrapping regardless of the declared
/// type (`jit_add` fast path + `int_bitop_helper` + interp `exec_value` all
/// operate on the i64 payload). So the native `iadd`/`band`/… path is
/// byte-identical to the helper for any integer type — the old `== I64`
/// predicate was leaving `int`/`uint`/`short`/… arithmetic on the helper path
/// for no reason. Narrowing is handled separately by the explicit `Convert`
/// op (`emit_i64_convert`), not here (z42 has no implicit narrowing →
/// intermediates stay i64).
///
/// **Unsigned note**: the VM (both interp and the JIT helper fallback) treats
/// `U64` uniformly as *signed* i64 for compare/shift (`numeric_lt`: `x < y`;
/// `shr`: `x >> (y & 63)` arithmetic). The native path deliberately matches
/// that (signed `icmp`, `sshr`) so `vm-jit-consistency` stays byte-identical —
/// making `U64` truly unsigned is a separate VM-wide change, not this one.
#[inline]
pub(super) fn is_int_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let get = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown);
    get(dst).is_integer() && get(a).is_integer() && get(b).is_integer()
}

/// Binary op kind passed to `emit_i64_binop`. Mirrors the subset of
/// `Instruction` variants we specialize so far.
///
/// review.md C2 P1 follow-up (2026-05-30): bitwise + shift opcodes added.
/// `Shl` / `Shr` mask the shift amount by 63 to match the helper
/// `jit_shl` / `jit_shr` behavior (`x << (y & 63)`).
#[derive(Clone, Copy)]
pub(super) enum BinopKind { Add, Sub, Mul, BitAnd, BitOr, BitXor, Shl, Shr }

/// F64 binary op kind for `emit_f64_binop` (jit-native-float). `Div` is safe
/// natively: IEEE float divide-by-zero yields ±inf/NaN (no trap), unlike i64
/// `sdiv` which must stay on the helper for the catchable exception.
#[derive(Clone, Copy)]
pub(super) enum F64BinopKind { Add, Sub, Mul, Div }

/// Comparison op kind for `emit_i64_cmp`.
#[derive(Clone, Copy)]
pub(super) enum CmpKind { Eq, Ne, Lt, Le, Gt, Ge }

/// Bool binary op kind for `emit_bool_binop`.
#[derive(Clone, Copy)]
pub(super) enum BoolBinopKind { And, Or }

/// Integer comparison fast-path predicate. Output is always `Bool` regardless
/// of input — we only need both operands to be integer types (`I8..U64`, all
/// stored as `Value::I64`). Phase 2A widened this from `== I64`; the native
/// compare is signed (`icmp`), matching the VM's uniform signed treatment of
/// all integer types incl. `U64` (see `is_int_typed`).
#[inline]
pub(super) fn is_int_cmp(func: &Function, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let get = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown);
    get(a).is_integer() && get(b).is_integer()
}

/// Bool binary-op predicate (And/Or): all three regs are Bool.
#[inline]
pub(super) fn is_bool_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let is_bool = |i: u32| rt.get(i as usize).copied() == Some(IrType::Bool);
    is_bool(dst) && is_bool(a) && is_bool(b)
}

/// Bool unary-op predicate (Not): both regs are Bool.
#[inline]
pub(super) fn is_bool_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_bool = |i: u32| rt.get(i as usize).copied() == Some(IrType::Bool);
    is_bool(dst) && is_bool(src)
}

/// Integer unary-op predicate (BitNot / Neg fast-path): both regs are integer
/// types (`I8..U64`). Phase 2A widened this from `== I64`; native `ineg`/`bnot`
/// on the i64 payload is byte-identical to the helper (`Value::I64(-n)` /
/// `Value::I64(!n)`) for any narrow integer, all stored as `Value::I64`.
#[inline]
pub(super) fn is_int_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_int = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown).is_integer();
    is_int(dst) && is_int(src)
}

/// jit-native-float: `true` iff `dst`, `a`, `b` are all `IrType::F64` (double).
/// Only `F64` — `F32` is stored widened as `Value::F64` and must round to f32
/// precision on write, which the native `fadd`/… path does not do, so `F32`
/// keeps the helper path. Mixed int/float also stays on the helper (which
/// promotes int→f64).
#[inline]
pub(super) fn is_f64_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let is_f64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::F64);
    is_f64(dst) && is_f64(a) && is_f64(b)
}

/// jit-native-float: both compare operands are `F64` (dst is Bool).
#[inline]
pub(super) fn is_f64_cmp(func: &Function, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let is_f64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::F64);
    is_f64(a) && is_f64(b)
}

/// jit-native-float: both unary operands are `F64` (Neg).
#[inline]
pub(super) fn is_f64_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_f64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::F64);
    is_f64(dst) && is_f64(src)
}

/// Predicate: `reg_types[reg]` is `expected`. Used by const-emit fast paths.
#[inline]
pub(super) fn is_typed(func: &Function, reg: u32, expected: IrType) -> bool {
    func.reg_types.get(reg as usize).copied() == Some(expected)
}

/// post-layout JIT perf (P5-B): how to natively load/store a register's static
/// primitive type against `ScriptObject::bytes` — mirroring `decode_prim`/
/// `encode_prim`. `width` = packed byte width, `ext` = how to widen a loaded scalar
/// into the 16B register payload, `reg_tag` = the `Value` discriminant stamped into
/// the register tag byte (int types → `Value::I64` = 0; floats → `Value::F64` = 1),
/// `field_tag` = the `TAG_*` handed to `jit_obj_field_slot` for runtime validation.
/// `None` for types stage 1 does NOT inline (F32 / Bool / Char / Str / Ref / Void /
/// Unknown) → those keep the `jit_field_get`/`jit_field_set` helper. Relies on the
/// invariant that a `FieldGet` dst / `FieldSet` val is typed as the field's declared
/// type (z42 has no implicit narrowing), so this width equals the packed field width.
#[derive(Clone, Copy)]
pub(super) struct FieldPrim {
    pub(super) load_ty: cranelift_codegen::ir::Type,
    pub(super) ext: FieldExt,
    pub(super) reg_tag: i64,
    pub(super) width: u32,
    pub(super) field_tag: u8,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FieldExt { Sext, Uext, Keep, Float }

pub(super) fn field_prim_kind(func: &Function, reg: u32) -> Option<FieldPrim> {
    use crate::metadata::types::{
        TAG_I8, TAG_I16, TAG_I32, TAG_I64, TAG_U8, TAG_U16, TAG_U32, TAG_U64, TAG_F64,
    };
    let (load_ty, ext, reg_tag, width, field_tag) = match func.reg_types.get(reg as usize).copied()? {
        IrType::I8  => (types::I8,  FieldExt::Sext,  0, 1, TAG_I8),
        IrType::I16 => (types::I16, FieldExt::Sext,  0, 2, TAG_I16),
        IrType::I32 => (types::I32, FieldExt::Sext,  0, 4, TAG_I32),
        IrType::I64 => (types::I64, FieldExt::Keep,  0, 8, TAG_I64),
        IrType::U8  => (types::I8,  FieldExt::Uext,  0, 1, TAG_U8),
        IrType::U16 => (types::I16, FieldExt::Uext,  0, 2, TAG_U16),
        IrType::U32 => (types::I32, FieldExt::Uext,  0, 4, TAG_U32),
        IrType::U64 => (types::I64, FieldExt::Keep,  0, 8, TAG_U64),
        IrType::F64 => (types::F64, FieldExt::Float, 1, 8, TAG_F64),
        _ => return None, // F32 / Bool / Char / Str / Ref / Void / Unknown → helper
    };
    Some(FieldPrim { load_ty, ext, reg_tag, width, field_tag })
}

/// Array-element classifier for the JIT inline get/set fast path
/// (jit-inline-i32-arrays). Returns `(val_tag, arr_width)`:
/// - `val_tag`: the `Value` tag written into the 16-byte register (0=I64, 1=F64).
///   `int` is stored as `Value::I64`, so I32 uses tag 0.
/// - `arr_width`: the packed slot width in bytes (4 for I32, 8 for I64/F64).
///
/// Reliable **only** for a register whose IR type equals the array element type
/// — i.e. an ArrayGet `dst` (the compiler types the result as the element type).
/// It is NOT reliable for an ArraySet `val`, which can be wider than the element
/// on a narrowing store; the set path consults the runtime width instead and
/// uses this only as a "worth attempting to inline" gate.
pub(super) fn arr_prim_elem(func: &Function, reg: u32) -> Option<(i64, i64)> {
    match func.reg_types.get(reg as usize).copied() {
        Some(IrType::I64) => Some((0, 8)),
        Some(IrType::F64) => Some((1, 8)),
        Some(IrType::I32) => Some((0, 4)),
        // jit-inline-char-arrays: `char` → `Value::Char` tag (3), width-4 slot.
        // The width-4 load sign-extends, but a valid `char` (≤ 0x10FFFF) has bit
        // 31 clear so sext == zext; the register store writes the codepoint into
        // the low 4 payload bytes + tag 3, mirroring `emit_const_char`.
        Some(IrType::Char) => Some((3, 4)),
        _ => None,
    }
}

/// Index-register gate for the array inline fast path: accept `I32` (`int i`)
/// as well as `I64` (`long i`). Both are stored as a `Value::I64` payload in the
/// register, so reading the index as an i64 is correct regardless.
pub(super) fn idx_int_ok(func: &Function, reg: u32) -> bool {
    is_typed(func, reg, IrType::I64) || is_typed(func, reg, IrType::I32)
}

/// True when `reg_types[reg]` is a primitive (drop-free) type — I64 / F64
/// / Bool / Char. Used by inline `ConstNull` to verify the existing slot
/// value is safe to overwrite without running Drop.
pub(super) fn is_drop_free_primitive(func: &Function, reg: u32) -> bool {
    matches!(
        func.reg_types.get(reg as usize).copied(),
        Some(IrType::I64) | Some(IrType::F64) | Some(IrType::Bool) | Some(IrType::Char)
    )
}
