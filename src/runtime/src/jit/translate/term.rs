//! Block terminator translation (Ret/Br/BrCond/Throw) + pre-terminator flush.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;
use super::ctx::TxCtx;
use crate::jit::helpers::value::JIT_GET_BOOL_ERR;
use crate::jit::reg_access::{PAYLOAD_OFFSET, TAG_OFFSET};

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_terminator(&mut self, term: &Terminator, block_instr_count: usize) -> Result<()> {
        // Block-end flush (2B): spill dirty cached scalars before the terminator.
        self.cache.flush(self.builder, self.regs_base);
                match term {
            Terminator::Ret { reg: None } => {
                let zero = self.builder.ins().iconst(types::I8, 0);
                self.builder.ins().return_(&[zero]);
            }
            Terminator::Ret { reg: Some(r) } => {
                // Inline return: copy the 16-byte `Value` into `frame.ret` and set
                // `frame.has_ret = 1` (`call_native` turns that into
                // `Returned(Some(_))`; a void `Ret` leaves it 0). `Value` is `Copy`,
                // so a raw two-word copy is exactly what a clone does.
                let flags = MemFlagsData::trusted();
                let ret_off = crate::jit::frame::JIT_FRAME_RET_OFFSET as i32;
                if self.promoted.get(*r as usize).copied().unwrap_or(false) {
                    // 2C: a resident Variable's value lives in SSA, not memory —
                    // write tag + payload straight into `frame.ret`. F64 residents
                    // carry the TAG_F64 discriminant; integer residents TAG_I64.
                    let v = self.builder.use_var(Variable::from_u32(*r));
                    let tag = if self.func.reg_types.get(*r as usize).copied() == Some(IrType::F64) {
                        TAG_F64
                    } else {
                        TAG_I64
                    };
                    let tag = self.builder.ins().iconst(types::I8, tag as i64);
                    self.builder.ins().store(flags, tag, self.frame_val, ret_off + TAG_OFFSET);
                    self.builder.ins().store(flags, v, self.frame_val, ret_off + PAYLOAD_OFFSET);
                } else {
                    // The block-end flush above made the slot current in memory.
                    let addr = reg_addr(self.builder, self.regs_base, *r);
                    let lo = self.builder.ins().load(types::I64, flags, addr, 0);
                    let hi = self.builder.ins().load(types::I64, flags, addr, 8);
                    self.builder.ins().store(flags, lo, self.frame_val, ret_off);
                    self.builder.ins().store(flags, hi, self.frame_val, ret_off + 8);
                }
                let one = self.builder.ins().iconst(types::I8, 1);
                self.builder.ins().store(flags, one, self.frame_val,
                    crate::jit::frame::JIT_FRAME_HAS_RET_OFFSET as i32);
                let zero = self.builder.ins().iconst(types::I8, 0);
                self.builder.ins().return_(&[zero]);
            }
            Terminator::Br { label } => {
                let target = self.func.blocks.iter().position(|b| &b.label == label)
                    .expect("Br label not found");
                // add-gc-safepoint-jit (2026-05-21): backward branch =
                // loop back-edge; check safepoint so long-running JIT
                // loops park promptly when GC requests a pause.
                if target <= self.block_idx {
                    emit_safepoint_check(self.builder, self.ptr, self.ctx_val, self.frame_val, self.hr_check_safepoint_slow);
                }
                self.builder.ins().jump(self.cl_blocks[target], &[]);
            }
            Terminator::BrCond { cond, true_label, false_label } => {
                // add-gc-safepoint-jit (2026-05-21): BrCond's runtime target
                // isn't known until cond is evaluated; check unconditionally.
                // Idle fast path is cheap; this catches loops where the
                // back-edge is a BrCond rather than a Br.
                emit_safepoint_check(self.builder, self.ptr, self.ctx_val, self.frame_val, self.hr_check_safepoint_slow);

                let true_idx  = self.func.blocks.iter().position(|blk| &blk.label == true_label)
                    .expect("true_label not found");
                let false_idx = self.func.blocks.iter().position(|blk| &blk.label == false_label)
                    .expect("false_label not found");

                // C2 P1 step 4 (2026-05-28): when reg_types[cond] confirms
                // Bool, skip the `jit_get_bool` helper call entirely — load
                // the i8 payload byte directly from `frame.regs[cond]` and
                // feed it to `brif`. Closes the dominant remaining helper-
                // call cost in the canonical numeric loop (every backward
                // branch was paying a function call to read a Bool we'd
                // *just* written via the cmp fast path).
                let cond_is_bool = self.func.reg_types
                    .get(*cond as usize)
                    .copied() == Some(IrType::Bool);
                if cond_is_bool {
                    let addr = reg_addr(self.builder, self.regs_base, *cond);
                    let b    = load_payload(self.builder, addr, types::I8);
                    self.builder.ins().brif(b, self.cl_blocks[true_idx], &[], self.cl_blocks[false_idx], &[]);
                } else {
                    let cv   = self.ri(*cond);
                    let inst = self.builder.ins().call(self.hr_get_bool, &[self.frame_val, self.ctx_val, cv]);
                    let b    = self.builder.inst_results(inst)[0];
                    // `jit_get_bool` returns 255 for a non-Bool condition, with the
                    // exception already set. Propagate it rather than letting the
                    // non-zero byte take the true branch.
                    let is_err = self.builder.ins().icmp_imm(IntCC::Equal, b, JIT_GET_BOOL_ERR as i64);
                    self.check(is_err);
                    self.builder.ins().brif(b, self.cl_blocks[true_idx], &[], self.cl_blocks[false_idx], &[]);
                }
            }
            Terminator::Throw { reg } => {
                let rv = self.ri(*reg);
                // jit-stack-trace: pass the throw site's code offset so jit_throw
                // can stamp the throwing frame before populating
                // Std.Exception.StackTrace. Throw is a block terminator; mirror
                // interp's end-of-block slot (`block_instr_count`).
                let off_val = self.builder.ins().iconst(types::I32, self.func.linear_offset(self.block_idx as u32, block_instr_count as u32) as i64);
                self.builder.ins().call(self.hr_throw, &[self.frame_val, self.ctx_val, rv, off_val]);
                self.dispatch_to_catch_or_return();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "term_tests.rs"]
mod term_tests;
