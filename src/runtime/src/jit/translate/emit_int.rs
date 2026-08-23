//! Inline Cranelift emit for statically-typed integer & bool ops.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// Emit Cranelift native code for `frame.regs[dst] = Value::I64(op(a, b))`,
/// loading both operands' i64 payloads via raw pointer arithmetic against
/// the cached `regs_base` and storing back with the I64 discriminant byte.
///
/// Layout assumption (pinned by `value_size_observed` +
/// `value_*_payload_at_offset_8` tests):
///   * Value stride 16 B, alignment 8
///   * u8 discriminant at offset 0 (TAG_I64 = 0)
///   * i64 payload at offset 8
///
/// Safety: caller must have verified `reg_types[dst] == I64` so the
/// pre-existing slot value is either `Null` (initial) or `I64`, both of
/// which have no Drop work — raw bit-copy is sound.
///
/// SEMANTICS: `semantics::int_binop` (I64 arm) — inline Cranelift mirror.
/// `iadd`/`isub`/`imul` are wrapping (INT_OVERFLOW_WRAPS); Shl/Shr `band 63`
/// mirrors `semantics::SHIFT_MASK`. Byte-identity pinned by
/// `translate/semantics_jit_diff_tests.rs`.
pub(super) fn emit_i64_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    op: BinopKind,
) {
    // Load payload i64s — resident Variable (2C) / cached SSA value (2B) /
    // memory load, resolved by `load_int`.
    let ai = load_int(builder, cache, promoted, regs_base, a);
    let bi = load_int(builder, cache, promoted, regs_base, b);

    // Compute (Cranelift `iadd`/`isub`/`imul` are wrapping by default —
    // matches z42's `vm-wrapping-int-arith` semantics).
    let result = match op {
        BinopKind::Add    => builder.ins().iadd(ai, bi),
        BinopKind::Sub    => builder.ins().isub(ai, bi),
        BinopKind::Mul    => builder.ins().imul(ai, bi),
        BinopKind::BitAnd => builder.ins().band(ai, bi),
        BinopKind::BitOr  => builder.ins().bor(ai, bi),
        BinopKind::BitXor => builder.ins().bxor(ai, bi),
        BinopKind::Shl    => {
            // Match `jit_shl` / `jit_shr`: shift amount masked to low 6 bits.
            let mask = builder.ins().iconst(types::I64, 63);
            let masked_bi = builder.ins().band(bi, mask);
            builder.ins().ishl(ai, masked_bi)
        }
        BinopKind::Shr    => {
            // Arithmetic shift (sign-extending) matches Rust's `i64 >>`.
            let mask = builder.ins().iconst(types::I64, 63);
            let masked_bi = builder.ins().band(bi, mask);
            builder.ins().sshr(ai, masked_bi)
        }
    };

    // Store to the resident Variable (2C) or the cache (2B), via `store_int`.
    store_int(builder, cache, promoted, dst, result);
}

/// Emit native `frame.regs[dst] = Value::I64(-src)` — integer negate
/// via Cranelift `ineg` (wrapping; `ineg(i64::MIN) == i64::MIN` matching
/// the helper's release-mode `-n` semantics). Caller must have verified
/// `reg_types[dst] == reg_types[src] == I64`.
pub(super) fn emit_i64_neg(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32,
) {
    let si     = load_int(builder, cache, promoted, regs_base, src);
    let result = builder.ins().ineg(si);
    store_int(builder, cache, promoted, dst, result);
}

/// Emit native `frame.regs[dst] = Value::I64(!src)` — bitwise NOT on i64
/// via Cranelift `bnot`. Caller must have verified `reg_types[dst] ==
/// reg_types[src] == I64` (review.md C2 P1 follow-up, 2026-05-30).
pub(super) fn emit_i64_bit_not(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32,
) {
    let si     = load_int(builder, cache, promoted, regs_base, src);
    let result = builder.ins().bnot(si);
    store_int(builder, cache, promoted, dst, result);
}

/// Emit Cranelift native `icmp <pred>` for `frame.regs[dst] = Value::Bool(a OP b)`
/// when both `a` and `b` are statically I64. Result discriminant is `TAG_BOOL`,
/// payload is the i8 comparison result.
///
/// SEMANTICS: `semantics::eval_cmp` (I64 arm; CMP = signed ordered) — inline
/// Cranelift mirror via signed `IntCC`. Byte-identity pinned by diff tests.
pub(super) fn emit_i64_cmp(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    kind: CmpKind,
) {
    use cranelift_codegen::ir::condcodes::IntCC;

    // Operands read via resident Variable (2C) / cache (2B) / memory.
    let ai = load_int(builder, cache, promoted, regs_base, a);
    let bi = load_int(builder, cache, promoted, regs_base, b);

    // Cranelift `icmp` returns an i8 (boolean: 0 or 1) — directly the
    // payload byte we need to write back. Signed compares since z42's
    // `<` / `<=` etc. are signed on all narrow integer types (i8..i64).
    let cc = match kind {
        CmpKind::Eq => IntCC::Equal,
        CmpKind::Ne => IntCC::NotEqual,
        CmpKind::Lt => IntCC::SignedLessThan,
        CmpKind::Le => IntCC::SignedLessThanOrEqual,
        CmpKind::Gt => IntCC::SignedGreaterThan,
        CmpKind::Ge => IntCC::SignedGreaterThanOrEqual,
    };
    let result_i8 = builder.ins().icmp(cc, ai, bi);

    // The dst is a Bool, not an integer — write it straight to memory (with
    // TAG_BOOL) and drop any stale integer cache entry for it. The consumer
    // (a `BrCond`) reads it from memory after the block-end flush.
    let addr_dst = reg_addr(builder, regs_base, dst);
    store_const_tag(builder, addr_dst, TAG_BOOL, result_i8);
    cache.invalidate(dst);
}

/// Emit Cranelift native `band`/`bor` on Bool operands.
/// `frame.regs[dst] = Value::Bool(a OP b)` for And/Or, statically Bool inputs.
pub(super) fn emit_bool_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, a: u32, b: u32,
    kind: BoolBinopKind,
) {
    let addr_a   = reg_addr(builder, regs_base, a);
    let addr_b   = reg_addr(builder, regs_base, b);
    let addr_dst = reg_addr(builder, regs_base, dst);

    // Bool payload is a single u8 at offset 8.
    let ai = load_payload(builder, addr_a, types::I8);
    let bi = load_payload(builder, addr_b, types::I8);

    let result = match kind {
        BoolBinopKind::And => builder.ins().band(ai, bi),
        BoolBinopKind::Or  => builder.ins().bor(ai, bi),
    };

    store_const_tag(builder, addr_dst, TAG_BOOL, result);
}

/// Emit Cranelift native `bnot` (xor 1) for `Value::Bool(!a)`. The src
/// payload is a single u8 (0 or 1); `xor 1` flips it. Avoids the
/// `band/bor` constant-fold subtlety of Cranelift's `bnot` on i8 (which
/// would flip ALL bits, producing 0xfe from 0x01 — wrong for a Bool slot).
pub(super) fn emit_bool_not(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32,
) {
    let addr_src = reg_addr(builder, regs_base, src);
    let addr_dst = reg_addr(builder, regs_base, dst);

    let si = load_payload(builder, addr_src, types::I8);
    let one = builder.ins().iconst(types::I8, 1);
    let result = builder.ins().bxor(si, one);

    store_const_tag(builder, addr_dst, TAG_BOOL, result);
}
