//! Array new/get/set/len translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_array(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::ArrayNew(insn) => {
                    let d = self.ri(insn.dst); let s = self.ri(insn.size);
                    let t = self.builder.ins().iconst(types::I8, insn.elem_tag as i64);
                    let (etp, etl) = self.str_val(&insn.element_type);   // add-reflection-array-element-type
                    let inst = self.builder.ins().call(self.hr_array_new, &[self.frame_val, self.ctx_val, d, s, t, etp, etl]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::ArrayNewLit(insn) => {
                    let d = self.ri(insn.dst);
                    let (ep, el) = self.regs_val(&insn.elems);
                    let (etp, etl) = self.str_val(&insn.element_type);
                    let inst = self.builder.ins().call(self.hr_array_new_lit, &[self.frame_val, self.ctx_val, d, ep, el, etp, etl]);
                    // add-struct-jit-value-path (P5): now u8 (struct-literal pack / OOM can throw).
                    let ret = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::ArrayGet { dst, arr, idx } => {
                    // jit-inline-fastpaths: when the element (`dst`) and index are
                    // statically i64, do a NATIVE bounds-check + element load +
                    // unboxed store — no per-element `Value` round-trip through the
                    // `jit_array_get` helper. The array data self.ptr+len come either
                    // from the loop-invariant HOIST (方案 B: never-reassigned array
                    // ⇒ zero per-iteration call, approaching the native ceiling) or
                    // a per-get `jit_array_data` (方案 A). Cold OOB path reuses
                    // `jit_array_get` so the exception is identical; for a hoisted
                    // null/invalid array `len==0` routes every access there too.
                    if let (Some((val_tag, arr_width)), true) =
                        (arr_prim_elem(self.func, *dst), idx_int_ok(self.func, *idx))
                    {
                        // jit-inline-i32-arrays: `dst`'s IR type reliably equals the
                        // array element type, so `arr_width` (4=int / 8=long·double)
                        // is a compile-time constant here — no runtime-width branch.
                        use cranelift_codegen::ir::condcodes::IntCC;
                        use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
                        let (data_ptr, len, width) = if let Some(&(hptr, hlen, hw)) = self.hoisted_arrays.get(arr) {
                            (hptr, hlen, hw) // 方案 B: loop-invariant, hoisted in entry block
                        } else {
                            let ss_ptr = self.builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_len = self.builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_width = self.builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ptr_addr = self.builder.ins().stack_addr(self.ptr, ss_ptr, 0);
                            let len_addr = self.builder.ins().stack_addr(self.ptr, ss_len, 0);
                            let width_addr = self.builder.ins().stack_addr(self.ptr, ss_width, 0);
                            let a_c = self.builder.ins().iconst(types::I32, *arr as i64);
                            let inst = self.builder.ins().call(self.hr_array_data,
                                &[self.frame_val, self.ctx_val, a_c, ptr_addr, len_addr, width_addr]);
                            let ret = self.builder.inst_results(inst)[0];
                            self.check(ret); // not-an-array → exception exit (方案 A)
                            let dp = self.builder.ins().stack_load(self.ptr, ss_ptr, 0);
                            let dl = self.builder.ins().stack_load(types::I64, ss_len, 0);
                            let dw = self.builder.ins().stack_load(types::I64, ss_width, 0);
                            (dp, dl, dw)
                        };
                        // idx payload (i64) from regs[idx]
                        let idx_addr = reg_addr(self.builder, self.regs_base, *idx);
                        let idx_v = load_payload_i64(self.builder, idx_addr);
                        // width==0 → non-packed backing (`Boxed`/`Bytes`/…), e.g. a
                        // closure env array read with a primitive-typed `dst`, or an
                        // `object[]` — the fast-path self.ptr is null there, so route to
                        // the `jit_array_get` helper (`get_boxed` returns the value).
                        let width_zero = self.builder.ins().icmp_imm(IntCC::Equal, width, 0);
                        let helper_blk = self.builder.create_block();
                        let fast_blk   = self.builder.create_block();
                        let done_blk   = self.builder.create_block();
                        self.builder.ins().brif(width_zero, helper_blk, &[], fast_blk, &[]);
                        // helper fallback: identical to the non-inline path.
                        self.builder.switch_to_block(helper_blk);
                        let d = self.ri(*dst); let a = self.ri(*arr); let i = self.ri(*idx);
                        let hinst = self.builder.ins().call(self.hr_array_get, &[self.frame_val, self.ctx_val, d, a, i]);
                        let hret  = self.builder.inst_results(hinst)[0];
                        self.check(hret);
                        self.builder.ins().jump(done_blk, &[]);
                        // packed fast path: bounds-check, then native element load.
                        self.builder.switch_to_block(fast_blk);
                        let oob = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx_v, len);
                        let oob_blk = self.builder.create_block();
                        let in_blk  = self.builder.create_block();
                        self.builder.ins().brif(oob, oob_blk, &[], in_blk, &[]);
                        // cold OOB: reuse jit_array_get to set the identical exception.
                        self.builder.switch_to_block(oob_blk);
                        let d_c = self.builder.ins().iconst(types::I32, *dst as i64);
                        let a_c2 = self.builder.ins().iconst(types::I32, *arr as i64);
                        let i_c = self.builder.ins().iconst(types::I32, *idx as i64);
                        self.builder.ins().call(self.hr_array_get, &[self.frame_val, self.ctx_val, d_c, a_c2, i_c]);
                        self.dispatch_to_catch_or_return();
                        // in-bounds: native element load + unboxed store. The packed
                        // array buffer is contiguous `arr_width`-byte slots with NO
                        // per-element tag; when width!=0 the runtime backing matches
                        // the compile-time `arr_width` (dst = element type). width-4
                        // (`int[]`) sign-extends into the i64 payload; width-8
                        // (`long[]`/`double[]`) is a raw load. Tag = static `val_tag`.
                        self.builder.switch_to_block(in_blk);
                        let stride_c = self.builder.ins().iconst(types::I64, arr_width);
                        let elem_off = self.builder.ins().imul(idx_v, stride_c);
                        let elem_addr = self.builder.ins().iadd(data_ptr, elem_off);
                        let elem = if arr_width == 4 {
                            let e32 = self.builder.ins().load(types::I32, MemFlags::trusted(), elem_addr, 0);
                            self.builder.ins().sextend(types::I64, e32)
                        } else {
                            self.builder.ins().load(types::I64, MemFlags::trusted(), elem_addr, 0)
                        };
                        // store into the 16-byte register `Value` (tag + payload).
                        let dst_addr = reg_addr(self.builder, self.regs_base, *dst);
                        let tag_c = self.builder.ins().iconst(types::I8, val_tag); // I64=0 / F64=1
                        store_tagged(self.builder, dst_addr, tag_c, elem);
                        self.builder.ins().jump(done_blk, &[]);
                        self.builder.switch_to_block(done_blk);
                    } else {
                        let d = self.ri(*dst); let a = self.ri(*arr); let i = self.ri(*idx);
                        let inst = self.builder.ins().call(self.hr_array_get, &[self.frame_val, self.ctx_val, d, a, i]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::ArraySet { arr, idx, val } => {
                    // jit-inline-fastpaths: i64 element store → native bounds-check
                    // + native store (no write barrier needed — i64 is drop-free
                    // and a type-correct `long[]` slot's old value is also i64).
                    // Data self.ptr+len from the hoist (方案 B) or per-set `jit_array_data`.
                    // Cold OOB / null reuses `jit_array_set` (identical exception,
                    // + write barrier for the heap-ref-value case that stays here).
                    if arr_prim_elem(self.func, *val).is_some() && idx_int_ok(self.func, *idx) {
                        // jit-inline-i32-arrays: the value register's width does NOT
                        // reliably match the array element width (a narrowing store
                        // `int[i] = <i64 value>` has an i64 value into a 4-byte slot),
                        // and the IR carries no element type on the array reg. So the
                        // store width comes from the RUNTIME backing (`out_width`):
                        // 4 (`int[]`), 8 (`long[]`/`double[]`), or 0 (non-packed →
                        // fall back to the helper, which narrows/boxes + write-barriers).
                        use cranelift_codegen::ir::condcodes::IntCC;
                        use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
                        let (data_ptr, len, width) = if let Some(&(hptr, hlen, hw)) = self.hoisted_arrays.get(arr) {
                            (hptr, hlen, hw)
                        } else {
                            let ss_ptr = self.builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_len = self.builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_width = self.builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ptr_addr = self.builder.ins().stack_addr(self.ptr, ss_ptr, 0);
                            let len_addr = self.builder.ins().stack_addr(self.ptr, ss_len, 0);
                            let width_addr = self.builder.ins().stack_addr(self.ptr, ss_width, 0);
                            let a_c = self.builder.ins().iconst(types::I32, *arr as i64);
                            let inst = self.builder.ins().call(self.hr_array_data,
                                &[self.frame_val, self.ctx_val, a_c, ptr_addr, len_addr, width_addr]);
                            let ret = self.builder.inst_results(inst)[0];
                            self.check(ret);
                            let dp = self.builder.ins().stack_load(self.ptr, ss_ptr, 0);
                            let dl = self.builder.ins().stack_load(types::I64, ss_len, 0);
                            let dw = self.builder.ins().stack_load(types::I64, ss_width, 0);
                            (dp, dl, dw)
                        };
                        let idx_addr = reg_addr(self.builder, self.regs_base, *idx);
                        let idx_v = load_payload_i64(self.builder, idx_addr);
                        let val_addr = reg_addr(self.builder, self.regs_base, *val);
                        let val_v = load_payload_i64(self.builder, val_addr);
                        // width==0 → non-packed backing (byte[]/Boxed/bool[]/char[]) →
                        // route to the helper (narrowing/boxing + write barrier).
                        let width_zero = self.builder.ins().icmp_imm(IntCC::Equal, width, 0);
                        let helper_blk = self.builder.create_block();
                        let fast_blk   = self.builder.create_block();
                        let done_blk   = self.builder.create_block();
                        self.builder.ins().brif(width_zero, helper_blk, &[], fast_blk, &[]);
                        // helper fallback: identical semantics to the non-inline path.
                        self.builder.switch_to_block(helper_blk);
                        let a = self.ri(*arr); let i = self.ri(*idx); let v = self.ri(*val);
                        let hinst = self.builder.ins().call(self.hr_array_set, &[self.frame_val, self.ctx_val, a, i, v]);
                        let hret  = self.builder.inst_results(hinst)[0];
                        self.check(hret);
                        self.builder.ins().jump(done_blk, &[]);
                        // packed fast path: bounds-check, then native store by runtime width.
                        self.builder.switch_to_block(fast_blk);
                        let oob = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx_v, len);
                        let oob_blk   = self.builder.create_block();
                        let store_blk = self.builder.create_block();
                        self.builder.ins().brif(oob, oob_blk, &[], store_blk, &[]);
                        // cold OOB: reuse jit_array_set (identical exception).
                        self.builder.switch_to_block(oob_blk);
                        let a_c2 = self.builder.ins().iconst(types::I32, *arr as i64);
                        let i_c = self.builder.ins().iconst(types::I32, *idx as i64);
                        let v_c = self.builder.ins().iconst(types::I32, *val as i64);
                        self.builder.ins().call(self.hr_array_set, &[self.frame_val, self.ctx_val, a_c2, i_c, v_c]);
                        self.dispatch_to_catch_or_return();
                        // in-bounds: elem_addr = data_ptr + idx*width; store `width`
                        // bytes — width-4 truncates the i64 payload, width-8 raw.
                        self.builder.switch_to_block(store_blk);
                        let elem_off = self.builder.ins().imul(idx_v, width);
                        let elem_addr = self.builder.ins().iadd(data_ptr, elem_off);
                        let is_w4 = self.builder.ins().icmp_imm(IntCC::Equal, width, 4);
                        let store4_blk = self.builder.create_block();
                        let store8_blk = self.builder.create_block();
                        self.builder.ins().brif(is_w4, store4_blk, &[], store8_blk, &[]);
                        self.builder.switch_to_block(store4_blk);
                        let v32 = self.builder.ins().ireduce(types::I32, val_v);
                        self.builder.ins().store(MemFlags::trusted(), v32, elem_addr, 0);
                        self.builder.ins().jump(done_blk, &[]);
                        self.builder.switch_to_block(store8_blk);
                        self.builder.ins().store(MemFlags::trusted(), val_v, elem_addr, 0);
                        self.builder.ins().jump(done_blk, &[]);
                        self.builder.switch_to_block(done_blk);
                    } else {
                        let a = self.ri(*arr); let i = self.ri(*idx); let v = self.ri(*val);
                        let inst = self.builder.ins().call(self.hr_array_set, &[self.frame_val, self.ctx_val, a, i, v]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::ArrayLen { dst, arr } => {
                    let d = self.ri(*dst); let a = self.ri(*arr);
                    let inst = self.builder.ins().call(self.hr_array_len, &[self.frame_val, self.ctx_val, d, a]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }

                // Objects
            _ => anyhow::bail!("tr_array: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
