//! Comparison (Eq/Ne/Lt/Le/Gt/Ge) translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_compare(&mut self, instr: &Instruction) -> Result<()> {
        let (kind, dst, a, b) = match *instr {
            Instruction::Eq { dst, a, b } => (CmpKind::Eq, dst, a, b),
            Instruction::Ne { dst, a, b } => (CmpKind::Ne, dst, a, b),
            Instruction::Lt { dst, a, b } => (CmpKind::Lt, dst, a, b),
            Instruction::Le { dst, a, b } => (CmpKind::Le, dst, a, b),
            Instruction::Gt { dst, a, b } => (CmpKind::Gt, dst, a, b),
            Instruction::Ge { dst, a, b } => (CmpKind::Ge, dst, a, b),
            _ => anyhow::bail!("tr_compare: unexpected opcode in category dispatch"),
        };
        if is_int_cmp(self.func, a, b) {
            emit_i64_cmp(self.builder, self.regs_base, self.cache, self.promoted, dst, a, b, kind);
        } else if is_f64_cmp(self.func, a, b) {
            emit_f64_cmp(self.builder, self.regs_base, self.promoted, dst, a, b, kind);
        } else if is_typed(self.func, a, IrType::Char) && is_typed(self.func, b, IrType::Char) {
            self.emit_char_cmp(kind, dst, a, b);
        } else {
            self.emit_cmp_helper(kind, dst, a, b);
        }
        Ok(())
    }

    /// The comparison helper (`jit_eq` … `jit_ge`). `Eq` / `Ne` cannot fail; the
    /// ordering ones can (type mismatch) and are followed by the exception check.
    fn emit_cmp_helper(&mut self, kind: CmpKind, dst: u32, a: u32, b: u32) {
        let (hr, can_throw) = match kind {
            CmpKind::Eq => (self.hr_eq, false),
            CmpKind::Ne => (self.hr_ne, false),
            CmpKind::Lt => (self.hr_lt, true),
            CmpKind::Le => (self.hr_le, true),
            CmpKind::Gt => (self.hr_gt, true),
            CmpKind::Ge => (self.hr_ge, true),
        };
        let (d, av, bv) = (self.ri(dst), self.ri(a), self.ri(b));
        let inst = self.builder.ins().call(hr, &[self.frame_val, self.ctx_val, d, av, bv]);
        if can_throw {
            let ret = self.builder.inst_results(inst)[0];
            self.check(ret);
        }
    }

    /// perf-str-jit-intrinsics: both operands statically `char` (`s[i] == '7'`,
    /// `c >= '0'`) — compare the code points natively, which is what
    /// `semantics::numeric_{eq,lt}` do for two `Value::Char`. Guarded by both
    /// runtime tags being `Char`; anything else takes the helper unchanged.
    fn emit_char_cmp(&mut self, kind: CmpKind, dst: u32, a: u32, b: u32) {
        let slow_blk = self.builder.create_block();
        let fast_blk = self.builder.create_block();
        let done_blk = self.builder.create_block();
        let a_addr = reg_addr(self.builder, self.regs_base, a);
        let b_addr = reg_addr(self.builder, self.regs_base, b);
        let ta = load_tag(self.builder, a_addr);
        let tb = load_tag(self.builder, b_addr);
        let a_ok = self.builder.ins().icmp_imm_s(IntCC::Equal, ta, TAG_CHAR as i64);
        let b_ok = self.builder.ins().icmp_imm_s(IntCC::Equal, tb, TAG_CHAR as i64);
        let both = self.builder.ins().band(a_ok, b_ok);
        self.builder.ins().brif(both, fast_blk, &[], slow_blk, &[]);

        self.builder.switch_to_block(fast_blk);
        // `Value::Char` payload: the 4-byte code point at the payload offset.
        let ca = load_payload(self.builder, a_addr, types::I32);
        let cb = load_payload(self.builder, b_addr, types::I32);
        let cc = match kind {
            CmpKind::Eq => IntCC::Equal,
            CmpKind::Ne => IntCC::NotEqual,
            CmpKind::Lt => IntCC::UnsignedLessThan,
            CmpKind::Le => IntCC::UnsignedLessThanOrEqual,
            CmpKind::Gt => IntCC::UnsignedGreaterThan,
            CmpKind::Ge => IntCC::UnsignedGreaterThanOrEqual,
        };
        let r = self.builder.ins().icmp(cc, ca, cb);
        let d_addr = reg_addr(self.builder, self.regs_base, dst);
        store_const_tag(self.builder, d_addr, TAG_BOOL, r);
        self.builder.ins().jump(done_blk, &[]);

        self.builder.switch_to_block(slow_blk);
        self.emit_cmp_helper(kind, dst, a, b);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(done_blk);
    }
}
