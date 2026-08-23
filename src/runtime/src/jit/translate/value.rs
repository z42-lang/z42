//! Const*/Copy/StrConcat/ToStr instruction translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_value(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::ConstI32 { dst, val } => {
                    if self.promoted.get(*dst as usize).copied().unwrap_or(false) {
                        // 2C: define the resident Variable, no memory store.
                        let v = self.builder.ins().iconst(types::I64, *val as i64);
                        self.builder.def_var(Variable::from_u32(*dst), v);
                    } else if is_typed(self.func, *dst, IrType::I64) {
                        emit_const_i64(self.builder, self.regs_base, *dst, *val as i64);
                    } else {
                        let d = self.ri(*dst); let v = self.builder.ins().iconst(types::I32, *val as i64);
                        self.builder.ins().call(self.hr_const_i32, &[self.frame_val, self.ctx_val, d, v]);
                    }
                }
                Instruction::ConstI64 { dst, val } => {
                    if self.promoted.get(*dst as usize).copied().unwrap_or(false) {
                        let v = self.builder.ins().iconst(types::I64, *val);
                        self.builder.def_var(Variable::from_u32(*dst), v);
                    } else if is_typed(self.func, *dst, IrType::I64) {
                        emit_const_i64(self.builder, self.regs_base, *dst, *val);
                    } else {
                        let d = self.ri(*dst); let v = self.builder.ins().iconst(types::I64, *val);
                        self.builder.ins().call(self.hr_const_i64, &[self.frame_val, self.ctx_val, d, v]);
                    }
                }
                Instruction::ConstF64 { dst, val } => {
                    if self.promoted.get(*dst as usize).copied().unwrap_or(false) {
                        // 2C (F64 residency): define the resident Variable, no memory store.
                        let v = self.builder.ins().f64const(*val);
                        self.builder.def_var(Variable::from_u32(*dst), v);
                    } else if is_typed(self.func, *dst, IrType::F64) {
                        emit_const_f64(self.builder, self.regs_base, *dst, *val);
                    } else {
                        let d = self.ri(*dst); let v = self.builder.ins().f64const(*val);
                        self.builder.ins().call(self.hr_const_f64, &[self.frame_val, self.ctx_val, d, v]);
                    }
                }
                Instruction::ConstBool { dst, val } => {
                    if is_typed(self.func, *dst, IrType::Bool) {
                        emit_const_bool(self.builder, self.regs_base, *dst, *val);
                    } else {
                        let d = self.ri(*dst); let v = self.builder.ins().iconst(types::I8, if *val { 1 } else { 0 });
                        self.builder.ins().call(self.hr_const_bool, &[self.frame_val, self.ctx_val, d, v]);
                    }
                }
                Instruction::ConstChar { dst, val } => {
                    if is_typed(self.func, *dst, IrType::Char) {
                        emit_const_char(self.builder, self.regs_base, *dst, *val);
                    } else {
                        let d = self.ri(*dst); let v = self.builder.ins().iconst(types::I32, *val as i32 as i64);
                        self.builder.ins().call(self.hr_const_char, &[self.frame_val, self.ctx_val, d, v]);
                    }
                }
                Instruction::ConstNull { dst } => {
                    if is_drop_free_primitive(self.func, *dst) {
                        emit_const_null(self.builder, self.regs_base, *dst);
                    } else {
                        let d = self.ri(*dst);
                        self.builder.ins().call(self.hr_const_null, &[self.frame_val, self.ctx_val, d]);
                    }
                }
                Instruction::ConstStr { dst, idx } => {
                    let d = self.ri(*dst); let i = self.ri(*idx);
                    let inst = self.builder.ins().call(self.hr_const_str, &[self.frame_val, self.ctx_val, d, i]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::Copy { dst, src } => {
                    // review.md C2 P1 follow-up (2026-05-30): inline when src
                    // and dst are both drop-free primitives (I64 / F64 / Bool
                    // / Char). 16 B Value = 1 B tag at offset 0 + 8 B payload
                    // at offset 8. Heap-ref payload requires Arc::clone so
                    // those keep the helper.
                    if is_drop_free_primitive(self.func, *dst)
                        && is_drop_free_primitive(self.func, *src)
                    {
                        emit_primitive_copy(self.builder, self.regs_base, *dst, *src);
                    } else {
                        let d = self.ri(*dst); let s = self.ri(*src);
                        self.builder.ins().call(self.hr_copy, &[self.frame_val, self.ctx_val, d, s]);
                    }
                }

                // Arithmetic — review.md C2 P1 (2026-05-28); widened by
                // jit-unbox-regalloc Phase 2A (2026-08-15): when reg_types
                // confirm all three operands are integer types (I8..U64, all
                // stored as Value::I64), emit native Cranelift
                // iadd/isub/imul via raw load/store on frame.regs; skip the
                // extern "C" helper call entirely. Otherwise fall back to the
                // type-dispatching helper (handles Str concat, F64, mixed
                // types, etc.).
                //
                // Safety of raw store: when reg_types[dst] is an integer type,
                // every write to that register slot is Value::I64 (initial
                // Null also has no Drop), so raw bit-copy without Drop is
                // sound. Div/Rem on i64 panic on /0 — keep helper for those
                // (zero-check + exception propagation lives there). Add/Sub/Mul
                // are wrapping (`vm-wrapping-int-arith`, 2026-04-28) matching
                // Cranelift defaults, at i64 width for all integer types.
                Instruction::StrConcat { dst, a, b } => {
                    let (d, av, bv) = (self.ri(*dst), self.ri(*a), self.ri(*b));
                    let inst = self.builder.ins().call(self.hr_str_concat, &[self.frame_val, self.ctx_val, d, av, bv]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::ToStr { dst, src } => {
                    let d = self.ri(*dst); let s = self.ri(*src);
                    let inst = self.builder.ins().call(self.hr_to_str, &[self.frame_val, self.ctx_val, d, s]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }

                // Calls
                // formalize-jit-method-token Phase 2.C (2026-05-08): emit
                // pre-resolved MethodId + name (fallback for cross-zpkg).
                // Helper checks id first; UNRESOLVED → uses name HashMap.
            _ => anyhow::bail!("tr_value: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
