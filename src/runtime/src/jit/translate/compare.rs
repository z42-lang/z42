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
            self.emit_cmp_guarded(kind, dst, a, b);
        }
        Ok(())
    }

    /// Dynamic-site guard inlining: `reg_types` can't type the operands, so
    /// guard on the runtime register tags and answer the common shapes
    /// natively — both `I64` (any compare, signed payload `icmp`), both `Str`
    /// (Eq/Ne content compare via the narrow `jit_eq_str`), and "exactly one
    /// side `Null`" (Eq/Ne folds to a constant: cross-variant equality is
    /// always false because `Null` joins no numeric-widening arm of
    /// `numeric_eq` — `p == null` on a reference lives exactly here).
    /// Everything else takes the fat helper unchanged; each guard-hit path is
    /// byte-equivalent to the corresponding helper arm. Same block-splitting
    /// pattern (and cache-coherence argument) as `emit_char_cmp`.
    fn emit_cmp_guarded(&mut self, kind: CmpKind, dst: u32, a: u32, b: u32) {
        let a_addr = reg_addr(self.builder, self.regs_base, a);
        let b_addr = reg_addr(self.builder, self.regs_base, b);
        let d_addr = reg_addr(self.builder, self.regs_base, dst);
        let ta = load_tag(self.builder, a_addr);
        let tb = load_tag(self.builder, b_addr);
        let done_blk = self.builder.create_block();

        // Guard 1: both I64 — the `jit_eq`/`jit_lt` fast path, inline.
        let fast_i64 = self.builder.create_block();
        let next_blk = self.builder.create_block();
        let a_i64 = self.builder.ins().icmp_imm_s(IntCC::Equal, ta, TAG_I64 as i64);
        let b_i64 = self.builder.ins().icmp_imm_s(IntCC::Equal, tb, TAG_I64 as i64);
        let both_i64 = self.builder.ins().band(a_i64, b_i64);
        self.builder.ins().brif(both_i64, fast_i64, &[], next_blk, &[]);
        self.builder.switch_to_block(fast_i64);
        let ai = load_payload_i64(self.builder, a_addr);
        let bi = load_payload_i64(self.builder, b_addr);
        let r = self.builder.ins().icmp(int_cc_for_cmp(kind), ai, bi);
        store_const_tag(self.builder, d_addr, TAG_BOOL, r);
        self.builder.ins().jump(done_blk, &[]);

        self.builder.switch_to_block(next_blk);
        let slow_blk = if matches!(kind, CmpKind::Eq | CmpKind::Ne) {
            // Guard 2: both Str — content compare, minus the fat helper's
            // frame/ctx/dst/a/b marshalling and tag dispatch.
            let fast_str = self.builder.create_block();
            let next2_blk = self.builder.create_block();
            let a_str = self.builder.ins().icmp_imm_s(IntCC::Equal, ta, TAG_STR as i64);
            let b_str = self.builder.ins().icmp_imm_s(IntCC::Equal, tb, TAG_STR as i64);
            let both_str = self.builder.ins().band(a_str, b_str);
            self.builder.ins().brif(both_str, fast_str, &[], next2_blk, &[]);
            self.builder.switch_to_block(fast_str);
            let inst = self.builder.ins().call(self.hr_eq_str, &[a_addr, b_addr]);
            let mut r = self.builder.inst_results(inst)[0];
            if kind == CmpKind::Ne {
                let one = self.builder.ins().iconst(types::I8, 1);
                r = self.builder.ins().bxor(r, one);
            }
            store_const_tag(self.builder, d_addr, TAG_BOOL, r);
            self.builder.ins().jump(done_blk, &[]);

            // Guard 3: exactly one side Null → Eq is false, Ne is true.
            self.builder.switch_to_block(next2_blk);
            let fast_null = self.builder.create_block();
            let slow_blk = self.builder.create_block();
            let a_null = self.builder.ins().icmp_imm_s(IntCC::Equal, ta, TAG_NULL as i64);
            let b_null = self.builder.ins().icmp_imm_s(IntCC::Equal, tb, TAG_NULL as i64);
            let cross_null = self.builder.ins().bxor(a_null, b_null);
            self.builder.ins().brif(cross_null, fast_null, &[], slow_blk, &[]);
            self.builder.switch_to_block(fast_null);
            let v = self.builder.ins().iconst(types::I8, if kind == CmpKind::Eq { 0 } else { 1 });
            store_const_tag(self.builder, d_addr, TAG_BOOL, v);
            self.builder.ins().jump(done_blk, &[]);
            slow_blk
        } else {
            // Ordering compares have no further shape to fold — this block
            // is the slow path.
            next_blk
        };

        self.builder.switch_to_block(slow_blk);
        self.emit_cmp_helper(kind, dst, a, b);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(done_blk);
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
