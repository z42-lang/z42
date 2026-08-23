//! Comparison (Eq/Ne/Lt/Le/Gt/Ge) translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_compare(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::Eq { dst, a, b } => {
                    if is_int_cmp(self.func, *a, *b) {
                        emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, CmpKind::Eq);
                    } else if is_f64_cmp(self.func, *a, *b) {
                        emit_f64_cmp(self.builder, self.regs_base, self.promoted, *dst, *a, *b, CmpKind::Eq);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        self.builder.ins().call(self.hr_eq, &[self.frame_val, self.ctx_val, d, av, bv]);
                    }
                }
                Instruction::Ne { dst, a, b } => {
                    if is_int_cmp(self.func, *a, *b) {
                        emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, CmpKind::Ne);
                    } else if is_f64_cmp(self.func, *a, *b) {
                        emit_f64_cmp(self.builder, self.regs_base, self.promoted, *dst, *a, *b, CmpKind::Ne);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        self.builder.ins().call(self.hr_ne, &[self.frame_val, self.ctx_val, d, av, bv]);
                    }
                }
                Instruction::Lt { dst, a, b } => {
                    if is_int_cmp(self.func, *a, *b) {
                        emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, CmpKind::Lt);
                    } else if is_f64_cmp(self.func, *a, *b) {
                        emit_f64_cmp(self.builder, self.regs_base, self.promoted, *dst, *a, *b, CmpKind::Lt);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_lt, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Le { dst, a, b } => {
                    if is_int_cmp(self.func, *a, *b) {
                        emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, CmpKind::Le);
                    } else if is_f64_cmp(self.func, *a, *b) {
                        emit_f64_cmp(self.builder, self.regs_base, self.promoted, *dst, *a, *b, CmpKind::Le);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_le, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Gt { dst, a, b } => {
                    if is_int_cmp(self.func, *a, *b) {
                        emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, CmpKind::Gt);
                    } else if is_f64_cmp(self.func, *a, *b) {
                        emit_f64_cmp(self.builder, self.regs_base, self.promoted, *dst, *a, *b, CmpKind::Gt);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_gt, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::Ge { dst, a, b } => {
                    if is_int_cmp(self.func, *a, *b) {
                        emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, *dst, *a, *b, CmpKind::Ge);
                    } else if is_f64_cmp(self.func, *a, *b) {
                        emit_f64_cmp(self.builder, self.regs_base, self.promoted, *dst, *a, *b, CmpKind::Ge);
                    } else {
                        let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                        let inst = self.builder.ins().call(self.hr_ge, &[self.frame_val, self.ctx_val, d, av, bv]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }

                // Logical — C2 P1: Bool-typed operands emit Cranelift
                // `band`/`bor`/`bnot` directly on the i8 payload.
            _ => anyhow::bail!("tr_compare: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
