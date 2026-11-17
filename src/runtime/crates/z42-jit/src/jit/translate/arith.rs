//! Integer/float arithmetic, bitwise, shift, logical, unary translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_arith(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::Add { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Add);
                    } else if is_f64_typed(self.func, *dst, *a, *b) {
                        emit_f64_binop(self.builder, self.regs_base, self.promoted, *dst, *a, *b, F64BinopKind::Add);
                    } else {
                        self.emit_arith_guarded_binop(*dst, *a, *b, self.hr_add, BinopKind::Add, true);
                    }
                }
                Instruction::Sub { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Sub);
                    } else if is_f64_typed(self.func, *dst, *a, *b) {
                        emit_f64_binop(self.builder, self.regs_base, self.promoted, *dst, *a, *b, F64BinopKind::Sub);
                    } else {
                        self.emit_arith_guarded_binop(*dst, *a, *b, self.hr_sub, BinopKind::Sub, false);
                    }
                }
                Instruction::Mul { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Mul);
                    } else if is_f64_typed(self.func, *dst, *a, *b) {
                        emit_f64_binop(self.builder, self.regs_base, self.promoted, *dst, *a, *b, F64BinopKind::Mul);
                    } else {
                        self.emit_arith_guarded_binop(*dst, *a, *b, self.hr_mul, BinopKind::Mul, false);
                    }
                }
                Instruction::Div { dst, a, b } => {
                    // jit-native-float: F64 divide is native `fdiv` — IEEE /0 →
                    // ±inf/NaN (no trap, no exception), matching interp.
                    // jit-native-int-divrem: integer divide is native `sdiv`
                    // with a cold guard for `b ∈ {0, -1}` (see `emit_int_divrem`).
                    if is_f64_typed(self.func, *dst, *a, *b) {
                        emit_f64_binop(self.builder, self.regs_base, self.promoted, *dst, *a, *b, F64BinopKind::Div);
                    } else if is_int_typed(self.func, *dst, *a, *b) {
                        self.emit_int_divrem(*dst, *a, *b, self.hr_div, true);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_div, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Rem { dst, a, b } => {
                    // jit-native-int-divrem: integer remainder is native `srem`
                    // with the same cold guard as Div. Float `%` (an f64 modulo,
                    // no single Cranelift op) is not int-typed → stays on the
                    // helper's `int_binop_helper` f64 path.
                    if is_int_typed(self.func, *dst, *a, *b) {
                        self.emit_int_divrem(*dst, *a, *b, self.hr_rem, false);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_rem, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }

                // Comparison — C2 P1; Phase 2A widened to all integer types:
                // integer-typed operands (I8..U64) emit Cranelift `icmp <pred>`
                // directly (signed, matching the VM's uniform signed compare);
                // Bool result stored back inline.
                Instruction::And { dst, a, b } => {
                    if is_bool_typed(self.func, *dst, *a, *b) {
                        emit_bool_binop(self.builder, self.regs_base, *dst, *a, *b, BoolBinopKind::And);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_and, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Or { dst, a, b } => {
                    if is_bool_typed(self.func, *dst, *a, *b) {
                        emit_bool_binop(self.builder, self.regs_base, *dst, *a, *b, BoolBinopKind::Or);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_or, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Not { dst, src } => {
                    if is_bool_typed_unary(self.func, *dst, *src) {
                        emit_bool_not(self.builder, self.regs_base, *dst, *src);
                    } else {
                        let d = self.ri(*dst); let s = self.ri(*src);
                        let inst = self.builder.ins().call(self.hr_not, &[self.frame_val, self.ctx_val, d, s]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }

                // Unary arithmetic — review.md C2 P1 follow-up :
                // I64-typed Neg emits native Cranelift `ineg` (wrapping,
                // matches helper's `Value::I64(-n)`).
                Instruction::Neg { dst, src } => {
                    if is_int_typed_unary(self.func, *dst, *src) {
                        emit_i64_neg(self.builder, self.regs_base, self.cache, self.promoted, *dst, *src);
                    } else if is_f64_typed_unary(self.func, *dst, *src) {
                        emit_f64_neg(self.builder, self.regs_base, self.promoted, *dst, *src);
                    } else {
                        let d = self.ri(*dst); let s = self.ri(*src);
                        let inst = self.builder.ins().call(self.hr_neg, &[self.frame_val, self.ctx_val, d, s]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }

                // Bitwise — review.md C2 P1 follow-up ; Phase 2A
                // widened to all integer types: inline native Cranelift
                // band/bor/bxor/bnot/ishl/sshr when reg_types confirm integer
                // operands (I8..U64). Same payload load/store layout as arith;
                // shift amount masked to low 6 bits; `sshr` (arithmetic) matches
                // the VM's uniform signed `>>` on all integer types.
                Instruction::BitAnd { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::BitAnd);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_bit_and, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::BitOr { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::BitOr);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_bit_or, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::BitXor { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::BitXor);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_bit_xor, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::BitNot { dst, src } => {
                    if is_int_typed_unary(self.func, *dst, *src) {
                        emit_i64_bit_not(self.builder, self.regs_base, self.cache, self.promoted, *dst, *src);
                    } else {
                        let d = self.ri(*dst); let s = self.ri(*src);
                        let inst = self.builder.ins().call(self.hr_bit_not, &[self.frame_val, self.ctx_val, d, s]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Shl { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Shl);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_shl, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Shr { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Shr);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_shr, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }

                // String
            _ => anyhow::bail!("tr_arith: unexpected opcode in category dispatch"),
        }
        Ok(())
    }

    /// Dynamic-site guard inlining for Add/Sub/Mul: guard both operands'
    /// runtime tags on `I64` and compute the wrapping op natively; `Add`
    /// additionally guards both `Str` and concats via the narrow
    /// `jit_add_str` (string building is the dominant dynamic-Add shape).
    /// Everything else takes the fat helper unchanged; each guard-hit path
    /// is byte-equivalent to the helper's arm (wrapping `iadd`/`isub`/
    /// `imul`, `alloc_str_concat2`). Same block-splitting pattern (and
    /// cache-coherence argument) as `emit_char_cmp`.
    fn emit_arith_guarded_binop(
        &mut self, dst: u32, a: u32, b: u32,
        hr: cranelift_codegen::ir::FuncRef, op: BinopKind, with_str: bool,
    ) {
        let a_addr = reg_addr(self.builder, self.regs_base, a);
        let b_addr = reg_addr(self.builder, self.regs_base, b);
        let d_addr = reg_addr(self.builder, self.regs_base, dst);
        let ta = load_tag(self.builder, a_addr);
        let tb = load_tag(self.builder, b_addr);
        let done_blk = self.builder.create_block();

        // Guard 1: both I64 — the helper's fast path, inline (wrapping).
        let fast_i64 = self.builder.create_block();
        let next_blk = self.builder.create_block();
        let a_i64 = self.builder.ins().icmp_imm_s(IntCC::Equal, ta, TAG_I64 as i64);
        let b_i64 = self.builder.ins().icmp_imm_s(IntCC::Equal, tb, TAG_I64 as i64);
        let both_i64 = self.builder.ins().band(a_i64, b_i64);
        self.builder.ins().brif(both_i64, fast_i64, &[], next_blk, &[]);
        self.builder.switch_to_block(fast_i64);
        let ai = load_payload_i64(self.builder, a_addr);
        let bi = load_payload_i64(self.builder, b_addr);
        let r = match op {
            BinopKind::Add => self.builder.ins().iadd(ai, bi),
            BinopKind::Sub => self.builder.ins().isub(ai, bi),
            BinopKind::Mul => self.builder.ins().imul(ai, bi),
            _ => unreachable!("guarded binop is Add/Sub/Mul only"),
        };
        store_const_tag(self.builder, d_addr, TAG_I64, r);
        self.builder.ins().jump(done_blk, &[]);

        self.builder.switch_to_block(next_blk);
        let slow_blk = if with_str {
            // Guard 2: both Str — fused concat via the narrow helper.
            let fast_str = self.builder.create_block();
            let slow_blk = self.builder.create_block();
            let a_str = self.builder.ins().icmp_imm_s(IntCC::Equal, ta, TAG_STR as i64);
            let b_str = self.builder.ins().icmp_imm_s(IntCC::Equal, tb, TAG_STR as i64);
            let both_str = self.builder.ins().band(a_str, b_str);
            self.builder.ins().brif(both_str, fast_str, &[], slow_blk, &[]);
            self.builder.switch_to_block(fast_str);
            let inst = self.builder.ins().call(self.hr_add_str, &[self.ctx_val, a_addr, b_addr, d_addr]);
            let ret = self.builder.inst_results(inst)[0];
            self.check(ret);
            self.builder.ins().jump(done_blk, &[]);
            slow_blk
        } else {
            // No further shape to fold — this block is the slow path.
            next_blk
        };

        self.builder.switch_to_block(slow_blk);
        let (d, av, bv) = (self.ri(dst), self.ri(a), self.ri(b));
        let inst = self.builder.ins().call(hr, &[self.frame_val, self.ctx_val, d, av, bv]);
        let ret = self.builder.inst_results(inst)[0];
        self.check(ret);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(done_blk);
    }
}

#[cfg(test)]
#[path = "arith_guard_tests.rs"]
mod arith_guard_tests;
