//! Inline Cranelift emit for f64 ops, numeric convert, and constants.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// Emit native I64-source integer convert (Convert opcode fast path).
/// All narrow ints (I8/I16/I32/U8/U16/U32) are stored as Value::I64
/// payload internally, so the conversion is just a sign-trunc or
/// zero-trunc of the i64 bits — output type tag stays TAG_I64.
///
/// Caller must have verified `reg_types[src].is_integer()` (I8..U64, all
/// stored as `Value::I64`) and `to_tag` ∈
/// {T_I8, T_I16, T_I32, T_I64, T_U8, T_U16, T_U32, T_U64}.
pub(super) fn emit_i64_convert(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32, to_tag: u8,
) {
    let si = load_int(builder, cache, promoted, regs_base, src);

    // SEMANTICS: `semantics::convert_value` → `convert_from_i64` integer-narrowing
    // arm — inline Cranelift mirror (ireduce+sextend / band-mask). Tag constants
    // mirror `semantics::T_*` (Cranelift needs compile-time consts; the local copy
    // is deliberate — keep in sync with the single source of truth
    // `crate::metadata::types::TAG_*`, which `semantics::T_*` re-exports).
    const T_I8:  u8 = 0x02;
    const T_I16: u8 = 0x03;
    const T_I32: u8 = 0x04;
    const T_I64: u8 = 0x05;
    const T_U8:  u8 = 0x06;
    const T_U16: u8 = 0x07;
    const T_U32: u8 = 0x08;
    const T_U64: u8 = 0x09;
    let result = match to_tag {
        // I64 / U64: no truncation — pass through.
        T_I64 | T_U64 => si,
        // Signed narrowing: ireduce → sextend back to i64 (sign-extend bits).
        T_I8  => {
            let low = builder.ins().ireduce(types::I8,  si);
            builder.ins().sextend(types::I64, low)
        }
        T_I16 => {
            let low = builder.ins().ireduce(types::I16, si);
            builder.ins().sextend(types::I64, low)
        }
        T_I32 => {
            let low = builder.ins().ireduce(types::I32, si);
            builder.ins().sextend(types::I64, low)
        }
        // Unsigned narrowing: zero-extend low N bits — equivalent to
        // bit-and with the mask.
        T_U8  => {
            let mask = builder.ins().iconst(types::I64, 0xFF);
            builder.ins().band(si, mask)
        }
        T_U16 => {
            let mask = builder.ins().iconst(types::I64, 0xFFFF);
            builder.ins().band(si, mask)
        }
        T_U32 => {
            let mask = builder.ins().iconst(types::I64, 0xFFFFFFFF);
            builder.ins().band(si, mask)
        }
        // Caller's matches!() restricts to_tag — this is unreachable.
        _ => si,
    };

    store_int(builder, cache, promoted, dst, result);
}

/// jit-native-convert-float: emit `frame.regs[dst] = Value::F64(src as f64)` for
/// an integer→float `Convert`. All narrow ints are stored as `Value::I64` with
/// the payload already sign/zero-extended to i64, so a single `fcvt_from_sint`
/// (signed src) / `fcvt_from_uint` (unsigned src) on the i64 payload reproduces
/// interp's `x as f64` / `u as f64` exactly (interp uses full f64 precision even
/// for an `F32` target — no f32 rounding — so this covers both `to_tag`
/// F32/F64). Result discriminant `TAG_F64`.
///
/// Reads `src` straight from memory: the Phase 2C promotion whitelist
/// disqualifies any reg used as a non-int-`Convert` src, so `src` is never a
/// resident Variable here.
///
/// SEMANTICS: `semantics::convert_value` → `convert_from_i64` int→float arm
/// (I2F = full f64 precision, F32 target also f64). Inline Cranelift mirror.
pub(super) fn emit_int_to_f64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32, src_signed: bool,
) {
    let addr_src = reg_addr(builder, regs_base, src);
    let addr_dst = reg_addr(builder, regs_base, dst);
    let si = load_payload_i64(builder, addr_src);
    let f = if src_signed {
        builder.ins().fcvt_from_sint(types::F64, si)
    } else {
        builder.ins().fcvt_from_uint(types::F64, si)
    };
    store_const_tag(builder, addr_dst, TAG_F64, f);
}

/// jit-native-convert-float (float→int): emit `frame.regs[dst] = Value::I64(f as T)`
/// for an F64→integer `Convert`. Rust's `as` (used by interp `convert_from_f64`)
/// is a *saturating* float→int cast with NaN→0; Cranelift `fcvt_to_sint_sat` /
/// `fcvt_to_uint_sat` reproduce that byte-for-byte (same clamp-to-range, same
/// NaN→0). Every narrow int lives as a `Value::I64` payload, so the saturated
/// low-width result is sign/zero-extended back to i64 — matching interp's
/// `(f as i8) as i64` / `(f as u8) as i64` etc. `T_U64` deliberately mirrors
/// interp's `f as i64` (signed saturation to i64 range, NOT an unsigned cast —
/// see `convert_from_f64`). Result discriminant `TAG_I64`.
///
/// Reads `src` (F64) straight from memory: the Phase 2C promotion whitelist
/// disqualifies any F64 reg used as a float→int `Convert` src, so `src` is never
/// a resident Variable. Writes `dst` (integer) via `store_int` (resident
/// Variable / 2B cache / memory), so a float→int result feeding a resident
/// accumulator stays unboxed.
///
/// SEMANTICS: `semantics::convert_value` → `convert_from_f64` float→int arm
/// (F2I = saturate + NaN→0; `T_U64` uses signed i64 saturation). Inline mirror
/// via `fcvt_to_sint_sat` / `fcvt_to_uint_sat`.
pub(super) fn emit_f64_to_int(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32, to_tag: u8,
) {
    const T_I8:  u8 = 0x02;
    const T_I16: u8 = 0x03;
    const T_I32: u8 = 0x04;
    const T_I64: u8 = 0x05;
    const T_U8:  u8 = 0x06;
    const T_U16: u8 = 0x07;
    const T_U32: u8 = 0x08;
    const T_U64: u8 = 0x09;
    let addr_src = reg_addr(builder, regs_base, src);
    let f = load_payload(builder, addr_src, types::F64);
    let result = match to_tag {
        // Signed narrow widths: saturating fcvt to width, sign-extend to i64.
        T_I8  => { let v = builder.ins().fcvt_to_sint_sat(types::I8,  f); builder.ins().sextend(types::I64, v) }
        T_I16 => { let v = builder.ins().fcvt_to_sint_sat(types::I16, f); builder.ins().sextend(types::I64, v) }
        T_I32 => { let v = builder.ins().fcvt_to_sint_sat(types::I32, f); builder.ins().sextend(types::I64, v) }
        // I64 and U64 both use signed saturation to the i64 range (interp uses
        // `f as i64` for T_U64 too).
        T_I64 | T_U64 => builder.ins().fcvt_to_sint_sat(types::I64, f),
        // Unsigned narrow widths: saturating fcvt to width, zero-extend to i64.
        T_U8  => { let v = builder.ins().fcvt_to_uint_sat(types::I8,  f); builder.ins().uextend(types::I64, v) }
        T_U16 => { let v = builder.ins().fcvt_to_uint_sat(types::I16, f); builder.ins().uextend(types::I64, v) }
        T_U32 => { let v = builder.ins().fcvt_to_uint_sat(types::I32, f); builder.ins().uextend(types::I64, v) }
        // Caller's matches!() restricts to_tag to the eight integer widths.
        _ => builder.ins().fcvt_to_sint_sat(types::I64, f),
    };
    store_int(builder, cache, promoted, dst, result);
}

/// Emit native `frame.regs[dst] = frame.regs[src]` for drop-free primitive
/// slots (I64 / F64 / Bool / Char). Copies the 1 B tag at offset 0 plus
/// the 8 B payload at offset 8 — heap-ref payloads keep the helper path
/// because they need Arc::clone. Caller verified `is_drop_free_primitive`
/// on both dst and src so neither side has Drop work (review.md C2 P1
/// follow-up, 2026-05-30).
pub(super) fn emit_primitive_copy(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32,
) {
    let addr_src = reg_addr(builder, regs_base, src);
    let addr_dst = reg_addr(builder, regs_base, dst);
    let tag      = load_tag(builder, addr_src);
    let payload  = load_payload_i64(builder, addr_src);
    store_tagged(builder, addr_dst, tag, payload);
}

/// jit-native-float: emit `frame.regs[dst] = Value::F64(a OP b)` with native
/// Cranelift `fadd`/`fsub`/`fmul`/`fdiv` on the f64 payloads. Caller verified
/// all three regs are `F64`. Result discriminant `TAG_F64`. Matches interp's
/// `int_binop_helper` float arm (plain IEEE f64 arithmetic).
pub(super) fn emit_f64_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    op: F64BinopKind,
) {
    // Operands read via resident F64 Variable (2C) or memory, resolved by load_f64.
    let ai = load_f64(builder, promoted, regs_base, a);
    let bi = load_f64(builder, promoted, regs_base, b);
    let result = match op {
        F64BinopKind::Add => builder.ins().fadd(ai, bi),
        F64BinopKind::Sub => builder.ins().fsub(ai, bi),
        F64BinopKind::Mul => builder.ins().fmul(ai, bi),
        F64BinopKind::Div => builder.ins().fdiv(ai, bi),
    };
    store_f64(builder, promoted, regs_base, dst, result);
}

/// jit-native-float: emit `frame.regs[dst] = Value::Bool(a OP b)` for F64 operands
/// via native `fcmp`. Uses ORDERED comparisons (NaN → false) for
/// Eq/Lt/Le/Gt/Ge and UNORDERED-or-not-equal for Ne (NaN != NaN → true),
/// matching Rust's f64 `==`/`<`/… used by interp `numeric_lt`/`ops::compare`.
///
/// SEMANTICS: `semantics::eval_cmp` (F64 arm; CMP = ordered, `Ne` unordered).
/// The `FloatCC::NotEqual` for Ne mirrors `semantics::eval_cmp`'s `va != vb`
/// on NaN (→ true). Inline Cranelift mirror; byte-identity pinned by diff tests.
pub(super) fn emit_f64_cmp(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    kind: CmpKind,
) {
    use cranelift_codegen::ir::condcodes::FloatCC;
    // F64 operands read via resident Variable (2C) / memory; dst is Bool (never
    // an F64 candidate) → written straight to memory with TAG_BOOL.
    let addr_dst = reg_addr(builder, regs_base, dst);
    let ai = load_f64(builder, promoted, regs_base, a);
    let bi = load_f64(builder, promoted, regs_base, b);
    let cc = match kind {
        CmpKind::Eq => FloatCC::Equal,             // ordered: NaN==NaN → false
        CmpKind::Ne => FloatCC::NotEqual,          // unordered: NaN!=NaN → true
        CmpKind::Lt => FloatCC::LessThan,
        CmpKind::Le => FloatCC::LessThanOrEqual,
        CmpKind::Gt => FloatCC::GreaterThan,
        CmpKind::Ge => FloatCC::GreaterThanOrEqual,
    };
    let result_i8 = builder.ins().fcmp(cc, ai, bi);
    store_const_tag(builder, addr_dst, TAG_BOOL, result_i8);
}

/// jit-native-float: emit `frame.regs[dst] = Value::F64(-src)` via native `fneg`
/// (flips the IEEE sign bit; `-NaN` stays NaN). Caller verified both F64.
pub(super) fn emit_f64_neg(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    promoted: &[bool],
    dst: u32, src: u32,
) {
    let si     = load_f64(builder, promoted, regs_base, src);
    let result = builder.ins().fneg(si);
    store_f64(builder, promoted, regs_base, dst, result);
}

/// Emit native `frame.regs[dst] = Value::I64(val)` — store TAG_I64 + i64
/// payload at known offsets, no helper call. Caller must have verified
/// `reg_types[dst] == I64` (so the old slot value is Null or I64 = Drop-free).
pub(super) fn emit_const_i64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: i64,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().iconst(types::I64, val);
    store_const_tag(builder, addr_dst, TAG_I64, v);
}

/// Emit native `frame.regs[dst] = Value::F64(val)`.
pub(super) fn emit_const_f64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: f64,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().f64const(val);
    store_const_tag(builder, addr_dst, TAG_F64, v);
}

/// Emit native `frame.regs[dst] = Value::Bool(val)`.
pub(super) fn emit_const_bool(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: bool,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().iconst(types::I8, if val { 1 } else { 0 });
    store_const_tag(builder, addr_dst, TAG_BOOL, v);
}

/// Emit native `frame.regs[dst] = Value::Char(val)` — store TAG_CHAR + 4 B
/// codepoint payload. Caller must have verified `reg_types[dst] == Char`
/// (review.md C11 #4, 2026-05-30).
pub(super) fn emit_const_char(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: char,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().iconst(types::I32, val as u32 as i64);
    store_const_tag(builder, addr_dst, TAG_CHAR, v);
}

/// Emit native `frame.regs[dst] = Value::Null` — just stores TAG_NULL.
/// Caller must have verified the previous slot value is Drop-free (any
/// primitive `IrType` — I64/F64/Bool/Char). For Ref/Str/Unknown dst we
/// keep the helper path so the Drop runs (review.md C11 #4, 2026-05-30).
pub(super) fn emit_const_null(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    // Payload slot is left as-is; the discriminant alone defines `Null`.
    store_tag_const(builder, addr_dst, TAG_NULL);
}
