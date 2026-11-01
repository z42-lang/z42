//! Object new/typeof/field/vcall/is-instance/as-cast/static translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_object(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::ObjNew(insn) => {
                    // add-escape-analysis-stack-alloc: JIT ignores stack_alloc in v1
                    // (heap-allocates); the optimization targets interp (interp-first).
                    let ObjNewInsn { dst, class_name, ctor_name, args, type_args: _, stack_alloc: _, ctor_known } = &**insn;
                    // expand-jit-type-args: bake the site's interned type-arguments
                    // handle (`TypeArgs::as_raw`, a leaked process-lifetime entry) as a
                    // pointer constant for `jit_obj_new`.
                    let d = self.ri(*dst);
                    let (cp, cl) = self.str_val(class_name);
                    let (kp, kl) = self.str_val(ctor_name);
                    let (ap, al) = self.regs_val(args);
                    let tap = self.builder.ins().iconst(self.ptr, self.func.object_args(self.block_idx, self.instr_idx).as_raw() as i64);
                    // P1-2 PR 5: bake the site cache's address (class + ctor FnId + ctorless
                    // mark; stable through `Function.resolved`, like the FieldIC pointer below).
                    let cm = obj_new_site_ptr_at(self.func, self.block_idx, self.instr_idx);
                    let cmv = self.builder.ins().iconst(self.ptr, cm as i64);
                    // encode-ctorless-objnew: compile-time positive ctor marker (i8).
                    let ckv = self.builder.ins().iconst(types::I8, *ctor_known as i64);
                    let inst = self.builder.ins().call(self.hr_obj_new,
                        &[self.frame_val, self.ctx_val, d, cp, cl, kp, kl, ap, al, tap, cmv, ckv]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::Typeof(insn) => {
                    // add-reflection-generic-type-definition: marshal type_name +
                    // the IR `type_args: Box<[String]>` storage as `*const String`
                    // + count (mirrors ObjNew type_args). Helper can't throw.
                    let TypeofInsn { dst, type_name, type_args } = &**insn;
                    let d = self.ri(*dst);
                    let (np, nl) = self.str_val(type_name);
                    let tap = self.builder.ins().iconst(self.ptr, type_args.as_ptr() as i64);
                    let tac = self.builder.ins().iconst(types::I64, type_args.len() as i64);
                    self.builder.ins().call(self.hr_typeof, &[self.frame_val, self.ctx_val, d, np, nl, tap, tac]);
                }
                // P1-4: native monomorphic field IC (guard + one cell access), the helper as
                // the fallback — see `field_ic`.
                Instruction::FieldGet(insn) => self.emit_field_get(insn),
                Instruction::FieldSet(insn) => self.emit_field_set(insn),
                // Phase 2.E: emit VCallIC pointer as trailing helper arg.
                Instruction::VCall(insn) => {
                    // perf-str-jit-intrinsics: `s.Length` on a string receiver lowers to
                    // `vcall get_Length` — inline it, the helper sequence is the slow path.
                    match self.str_intrinsic_of_vcall(insn) {
                        Some(k) => {
                            let mut recv_args: Vec<u32> = Vec::with_capacity(insn.args.len() + 1);
                            recv_args.push(insn.obj);
                            recv_args.extend_from_slice(&insn.args);
                            self.emit_str_intrinsic(k, insn.dst, &recv_args, false,
                                |cx| cx.emit_vcall_helper(insn));
                        }
                        None => self.emit_vcall_helper(insn),
                    }
                }
                Instruction::IsInstance(insn) => {
                    let IsInstanceInsn { dst, obj, class_name } = &**insn;
                    let target = self.func.type_key(self.block_idx, self.instr_idx);
                    let d = self.ri(*dst); let o = self.ri(*obj);
                    let (cp, cl) = self.str_val(class_name);
                    let kp = self.builder.ins().iconst(self.ptr, target as *const _ as i64);
                    self.builder.ins().call(self.hr_is_instance, &[self.frame_val, self.ctx_val, d, o, cp, cl, kp]);
                }
                Instruction::AsCast(insn) => {
                    let AsCastInsn { dst, obj, class_name } = &**insn;
                    let target = self.func.type_key(self.block_idx, self.instr_idx);
                    let d = self.ri(*dst); let o = self.ri(*obj);
                    let (cp, cl) = self.str_val(class_name);
                    let kp = self.builder.ins().iconst(self.ptr, target as *const _ as i64);
                    self.builder.ins().call(self.hr_as_cast, &[self.frame_val, self.ctx_val, d, o, cp, cl, kp]);
                }

                // Static fields: the barrier's steady-state check + one cell access, the helper
                // otherwise — see `static_field`.
                Instruction::StaticGet(insn) => self.emit_static_get(insn),
                Instruction::StaticSet(insn) => self.emit_static_set(insn),

                // C1 native interop scaffold: JIT translation lands in
                // L3.M16. Refuse to compile a function that contains these
                // opcodes; caller should keep the function in Interp mode.
                // converge-vm-arith-semantics (H3): these arms are UNREACHABLE in
                // practice — `jit_unsupported_reason` (driven by `unsupported_reason`)
                // routes any function containing them to the interpreter before
                // translation. Kept for match exhaustiveness (no `_`
                // wildcard). Each bail sources its reason from the single source of
                // truth `unsupported_reason(instr)` so the prescan list and these arms
                // can never name a different opcode set.
            _ => anyhow::bail!("tr_object: unexpected opcode in category dispatch"),
        }
        Ok(())
    }

    /// The `jit_vcall` helper sequence for a `VCall` site. Also the slow path of an
    /// inlined string intrinsic.
    pub(super) fn emit_vcall_helper(&mut self, insn: &VCallInsn) {
        let VCallInsn { dst, obj, method, args, method_type_args } = insn;
        let d = self.ri(*dst); let o = self.ri(*obj);
        let (mp, ml) = self.str_val(method);
        let (ap, al) = self.regs_val(args);
        let ic_ptr = vcall_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
        let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
        // jit-stack-trace: call-site code offset.
        let off_val = self.builder.ins().iconst(types::I32, self.func.linear_offset(self.block_idx as u32, self.instr_idx as u32) as i64);
        // Generic site → `jit_vcall_mta` with the site's type args baked (see `emit_call_helper`).
        let inst = if method_type_args.is_empty() {
            self.builder.ins().call(self.hr_vcall, &[self.frame_val, self.ctx_val, d, o, mp, ml, ap, al, ic_val, off_val])
        } else {
            let tap = self.builder.ins().iconst(self.ptr, method_type_args.as_ptr() as i64);
            let tal = self.builder.ins().iconst(types::I64, method_type_args.len() as i64);
            self.builder.ins().call(self.hr_vcall_mta, &[self.frame_val, self.ctx_val, d, o, mp, ml, ap, al, ic_val, off_val, tap, tal])
        };
        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
    }
}
