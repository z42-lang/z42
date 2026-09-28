//! Struct value-type instruction translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_structs(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::StructAlloc(insn) => {
                    let d = self.ri(insn.dst);
                    let (tp, tl) = self.str_val(&insn.type_name);
                    let sz = self.builder.ins().iconst(types::I32, insn.size as i64);
                    self.builder.ins().call(self.hr_struct_alloc, &[self.frame_val, self.ctx_val, d, tp, tl, sz]);
                }
                Instruction::StructCopy { dst, src, size } => {
                    let d = self.ri(*dst); let s = self.ri(*src);
                    let sz = self.builder.ins().iconst(types::I32, *size as i64);
                    let inst = self.builder.ins().call(self.hr_struct_copy, &[self.frame_val, self.ctx_val, d, s, sz]);
                    let ret = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                // symbolic-struct-field-access P2：把 (root 类型名, 字段序号路径) 传进 helper，
                // 由它解析偏移 —— 与 interp 走**同一个** `resolve_field_path`，不在两侧各抄一份
                // （struct 路径上「判据复制」正是本 change 要消掉的东西）。
                // ⚠️ 字段访问在 JIT 里**本来就恒是 helper 调用**（烘焙偏移也只是个 iconst 实参），
                // 所以符号化的边际代价是已付调用里的一次查表，不是新增一次调用。
                Instruction::StructFieldGetPrim(i) => {
                    let d = self.ri(i.dst); let b = self.ri(i.base);
                    let (rp, rl) = self.str_val(&i.root_type);
                    let (pp, pl) = self.path_val(&i.path);
                    let k   = self.builder.ins().iconst(types::I8, i.kind as i64);
                    let inst = self.builder.ins().call(self.hr_struct_field_get_prim, &[self.frame_val, self.ctx_val, d, b, rp, rl, pp, pl, k]);
                    let ret = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::StructFieldSetPrim(i) => {
                    let b = self.ri(i.base); let v = self.ri(i.val);
                    let (rp, rl) = self.str_val(&i.root_type);
                    let (pp, pl) = self.path_val(&i.path);
                    let k   = self.builder.ins().iconst(types::I8, i.kind as i64);
                    let inst = self.builder.ins().call(self.hr_struct_field_set_prim, &[self.frame_val, self.ctx_val, b, rp, rl, pp, pl, k, v]);
                    let ret = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                // 2026-05-07 D-8b-3 Phase 2 + switch-multicast-funcpredicate-to-generic-exception:
                // emit `jit_default_of(frame, ctx, dst, param_index)` helper call.
                // JIT-allocated instances still have empty type_args (jit_obj_new
                // doesn't propagate them yet), so the helper falls through to Null
                // when called on a JIT-allocated generic instance — same path as
                // method-level / free generic graceful-degradation.
                Instruction::DefaultOf { dst, param_index } => {
                    let d  = self.ri(*dst);
                    let pi = self.builder.ins().iconst(types::I32, *param_index as i64);
                    let inst = self.builder.ins().call(self.hr_default_of, &[self.frame_val, self.ctx_val, d, pi]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                // add-generic-methods: unreachable — jit_unsupported_reason routes any
                // function containing these (method-generic body) to the interpreter
                // before translation. Kept for match exhaustiveness.
            _ => anyhow::bail!("tr_structs: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
