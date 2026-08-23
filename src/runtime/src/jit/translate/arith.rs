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
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_add, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Sub { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Sub);
                    } else if is_f64_typed(self.func, *dst, *a, *b) {
                        emit_f64_binop(self.builder, self.regs_base, self.promoted, *dst, *a, *b, F64BinopKind::Sub);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_sub, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Mul { dst, a, b } => {
                    if is_int_typed(self.func, *dst, *a, *b) {
                        emit_i64_binop(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, BinopKind::Mul);
                    } else if is_f64_typed(self.func, *dst, *a, *b) {
                        emit_f64_binop(self.builder, self.regs_base, self.promoted, *dst, *a, *b, F64BinopKind::Mul);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_mul, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
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

                // Unary arithmetic — review.md C2 P1 follow-up (2026-05-30):
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

                // Bitwise — review.md C2 P1 follow-up (2026-05-30); Phase 2A
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
}
