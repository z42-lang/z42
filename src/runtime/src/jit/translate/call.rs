//! Call/Builtin/closure (LoadFn/MkClos/CallIndirect) translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_call(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::Call(insn) => {
                    // add-generic-methods: generic Call sites are filtered out by
                    // jit_unsupported_reason (stay interp); non-generic reach here.
                    let CallInsn { dst, func: fname, args, .. } = &**insn;
                    let d = self.ri(*dst);
                    let (np, nl) = self.str_val(fname);
                    let (ap, al) = self.regs_val(args);
                    let mid = method_id_at(self.func, self.block_idx, self.instr_idx);
                    let mid_val = self.builder.ins().iconst(types::I32, mid as i64);
                    // make-vm-loading-lazy: per-site IC caching the resolved
                    // lazy/merged fn id, so a cross-zpkg call resolves the name
                    // once then hits the lock-free by-id fast path thereafter.
                    let ic_ptr = call_jit_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                    let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                    // 2026-05-10 jit-stack-trace + span-column-propagate: pass
                    // current source (line, col) so jit_call can stamp the
                    // caller's frame info before descending into the callee.
                    let (line, col) = crate::interp::resolve_line(self.func.line_table(), self.block_idx as u32, self.instr_idx as u32);
                    let line_val = self.builder.ins().iconst(types::I32, line as i64);
                    let col_val  = self.builder.ins().iconst(types::I32, col as i64);
                    // add-offline-symbolication: bake linearized code offset (caller frame).
                    let off_val = self.builder.ins().iconst(types::I32, self.func.linear_offset(self.block_idx as u32, self.instr_idx as u32) as i64);
                    let inst = self.builder.ins().call(self.hr_call, &[self.frame_val, self.ctx_val, d, mid_val, np, nl, ap, al, ic_val, line_val, col_val, off_val]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    // add-gc-safepoint-jit (2026-05-21): post-Call safepoint
                    // — long callees may yield to a GC request that arrived
                    // partway through; the caller catches it on return.
                    emit_safepoint_check(self.builder, self.ptr, self.ctx_val, self.frame_val, self.hr_check_safepoint_slow);
                }
                Instruction::Builtin(insn) => {
                    let BuiltinInsn { dst, name, args } = &**insn;
                    // formalize-jit-method-token Phase 2 (2026-05-08): emit
                    // pre-resolved BuiltinId as i32 const, drop name pointers.
                    // Resolver populates Function.resolved.builtin_tokens at
                    // load (closed set, never UNRESOLVED at this point).
                    let d = self.ri(*dst);
                    let (ap, al) = self.regs_val(args);
                    let builtin_id = self.func.resolved.get()
                        .and_then(|r| {
                            let site = *r.site_index.get(self.block_idx)?.get(self.instr_idx)?;
                            r.builtin_tokens.get(site as usize).copied()
                        })
                        .unwrap_or_else(|| {
                            // Fallback: resolver hadn't run (shouldn't happen
                            // in production via Vm::run, but guards against
                            // direct compile_module callers in tests).
                            crate::corelib::builtin_id_of(name)
                                .unwrap_or_else(|| panic!("unknown builtin `{}`", name))
                                .0
                        });
                    let bid = self.builder.ins().iconst(types::I32, builtin_id as i64);
                    let inst = self.builder.ins().call(self.hr_builtin, &[self.frame_val, self.ctx_val, d, bid, ap, al]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }

                // Arrays
                Instruction::LoadFn(insn) => {
                    let LoadFnInsn { dst, func } = &**insn;
                    let d = self.ri(*dst);
                    let (np, nl) = self.str_val(func);
                    let inst = self.builder.ins().call(self.hr_load_fn, &[self.frame_val, self.ctx_val, d, np, nl]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                // 2026-05-02 D1b: cached method group conversion
                Instruction::LoadFnCached(insn) => {
                    let LoadFnCachedInsn { dst, func, slot_id } = &**insn;
                    let d = self.ri(*dst);
                    let (np, nl) = self.str_val(func);
                    let sid = self.builder.ins().iconst(types::I32, *slot_id as i64);
                    let inst = self.builder.ins().call(self.hr_load_fn_cached,
                        &[self.frame_val, self.ctx_val, d, np, nl, sid]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::MkClos(insn) => {
                    let MkClosInsn { dst, fn_name, captures, stack_alloc } = &**insn;
                    let d = self.ri(*dst);
                    let (np, nl) = self.str_val(fn_name);
                    let (cp, cl) = self.regs_val(captures);
                    let sa = self.builder.ins().iconst(types::I8, if *stack_alloc { 1 } else { 0 });
                    let inst = self.builder.ins().call(self.hr_mk_clos,
                        &[self.frame_val, self.ctx_val, d, np, nl, cp, cl, sa]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::CallIndirect { dst, callee, args } => {
                    let d = self.ri(*dst);
                    let c = self.ri(*callee);
                    let (ap, al) = self.regs_val(args);
                    // 2026-05-10 jit-stack-trace + span-column-propagate.
                    let (line, col) = crate::interp::resolve_line(self.func.line_table(), self.block_idx as u32, self.instr_idx as u32);
                    let line_val = self.builder.ins().iconst(types::I32, line as i64);
                    let col_val  = self.builder.ins().iconst(types::I32, col as i64);
                    let off_val = self.builder.ins().iconst(types::I32, self.func.linear_offset(self.block_idx as u32, self.instr_idx as u32) as i64);
                    let inst = self.builder.ins().call(self.hr_call_indirect,
                        &[self.frame_val, self.ctx_val, d, c, ap, al, line_val, col_val, off_val]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    // add-gc-safepoint-jit (2026-05-21): post-CallIndirect
                    // safepoint, see Instruction::Call for rationale.
                    emit_safepoint_check(self.builder, self.ptr, self.ctx_val, self.frame_val, self.hr_check_safepoint_slow);
                }
            _ => anyhow::bail!("tr_call: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
