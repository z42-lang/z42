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
                    // add-reflection-array-element-type: the interned element-type handle,
                    // baked in as a pointer constant (entries are process-lifetime).
                    let et = self.builder.ins().iconst(self.ptr, insn.element_type.as_raw() as i64);
                    // `new T[n]` on a CLASS-level type param: pass its index so the helper zero-fills
                    // a primitive T from the receiver's (reg 0) type args — same resolution as interp.
                    // (Method-level T has no JIT carrier → `unsupported_reason`.)
                    let class_tp = if insn.type_param_kind == 2 { insn.type_param_index } else { -1 };
                    let tp = self.builder.ins().iconst(types::I32, class_tp as i64);
                    let tpk = self.builder.ins().iconst(types::I8, insn.type_param_kind as i64);
                    let inst = self.builder.ins().call(self.hr_array_new, &[self.frame_val, self.ctx_val, d, s, t, et, tp, tpk]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::ArrayNewLit(insn) => {
                    let d = self.ri(insn.dst);
                    let (ep, el) = self.regs_val(&insn.elems);
                    let et = self.builder.ins().iconst(self.ptr, insn.element_type.as_raw() as i64);
                    let inst = self.builder.ins().call(self.hr_array_new_lit, &[self.frame_val, self.ctx_val, d, ep, el, et]);
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
                    // a per-get `jit_array_data_opt` (方案 A). Cold OOB path reuses
                    // `jit_array_get` so the exception is identical; a null / non-packed
                    // array reports width 0, routing every access to the helper too.
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
                            // never throws: not-a-packed-array (incl. null) → width 0 → helper.
                            self.builder.ins().call(self.hr_array_data,
                                &[self.frame_val, self.ctx_val, a_c, ptr_addr, len_addr, width_addr]);
                            let dp = self.builder.ins().stack_load(self.ptr, self.ptr, ss_ptr, 0);
                            let dl = self.builder.ins().stack_load(self.ptr, types::I64, ss_len, 0);
                            let dw = self.builder.ins().stack_load(self.ptr, types::I64, ss_width, 0);
                            (dp, dl, dw)
                        };
                        // idx payload (i64) from regs[idx]
                        let idx_addr = reg_addr(self.builder, self.regs_base, *idx);
                        let idx_v = load_payload_i64(self.builder, idx_addr);
                        // width==0 → non-packed backing (`Boxed`/`Bytes`/…), e.g. a
                        // closure env array read with a primitive-typed `dst`, or an
                        // `object[]` — the fast-path self.ptr is null there, so route to
                        // the `jit_array_get` helper (`get_boxed` returns the value).
                        let width_zero = self.builder.ins().icmp_imm_s(IntCC::Equal, width, 0);
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
                            let e32 = self.builder.ins().load(types::I32, MemFlagsData::trusted(), elem_addr, 0);
                            self.builder.ins().sextend(types::I64, e32)
                        } else {
                            self.builder.ins().load(types::I64, MemFlagsData::trusted(), elem_addr, 0)
                        };
                        // store into the 16-byte register `Value` (tag + payload).
                        let dst_addr = reg_addr(self.builder, self.regs_base, *dst);
                        let tag_c = self.builder.ins().iconst(types::I8, val_tag); // I64=0 / F64=1
                        store_tagged(self.builder, dst_addr, tag_c, elem);
                        self.builder.ins().jump(done_blk, &[]);
                        self.builder.switch_to_block(done_blk);
                    } else if arr_ref_elem(self.func, *dst) && idx_int_ok(self.func, *idx) {
                        self.tr_array_get_ref(*dst, *arr, *idx);
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
                    // Data self.ptr+len from the hoist (方案 B) or per-set `jit_array_data_opt`.
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
                            // never throws: not-a-packed-array (incl. null) → width 0 → helper.
                            self.builder.ins().call(self.hr_array_data,
                                &[self.frame_val, self.ctx_val, a_c, ptr_addr, len_addr, width_addr]);
                            let dp = self.builder.ins().stack_load(self.ptr, self.ptr, ss_ptr, 0);
                            let dl = self.builder.ins().stack_load(self.ptr, types::I64, ss_len, 0);
                            let dw = self.builder.ins().stack_load(self.ptr, types::I64, ss_width, 0);
                            (dp, dl, dw)
                        };
                        let idx_addr = reg_addr(self.builder, self.regs_base, *idx);
                        let idx_v = load_payload_i64(self.builder, idx_addr);
                        let val_addr = reg_addr(self.builder, self.regs_base, *val);
                        let val_v = load_payload_i64(self.builder, val_addr);
                        // width==0 → non-packed backing (byte[]/Boxed/bool[]/char[]) →
                        // route to the helper (narrowing/boxing + write barrier).
                        let width_zero = self.builder.ins().icmp_imm_s(IntCC::Equal, width, 0);
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
                        let is_w4 = self.builder.ins().icmp_imm_s(IntCC::Equal, width, 4);
                        let store4_blk = self.builder.create_block();
                        let store8_blk = self.builder.create_block();
                        self.builder.ins().brif(is_w4, store4_blk, &[], store8_blk, &[]);
                        self.builder.switch_to_block(store4_blk);
                        let v32 = self.builder.ins().ireduce(types::I32, val_v);
                        self.builder.ins().store(MemFlagsData::trusted(), v32, elem_addr, 0);
                        self.builder.ins().jump(done_blk, &[]);
                        self.builder.switch_to_block(store8_blk);
                        self.builder.ins().store(MemFlagsData::trusted(), val_v, elem_addr, 0);
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

impl<'a, 'b> TxCtx<'a, 'b> {
    /// R3 reference read: `ArrayGet` into a whole-`Value` register on a REF (8 B reference words)
    /// or MIXED (16 B type-parameter cells, tag word first) array. `(base, len, stride | zero << 8)`
    /// comes from the entry-block hoist for a never-reassigned array, else from one
    /// `jit_array_ref_data_opt` call. Native bounds check, then an acquire load of the element's
    /// (tag) word, decoded exactly as `ref_word::decode` / `tparam_cell::load_or_zero` would:
    /// - low 3 bits non-zero: a reference word → `{KIND_TO_VALUE_TAG[kind], word & !7}`; a box
    ///   (kind 7) takes `jit_array_get`;
    /// - word `0`: never written → the array's zero kind (`PRIM_KIND_TO_VALUE_TAG[zero]`, `null`
    ///   for kind 0), payload 0;
    /// - otherwise a MIXED marker: `NULL(k)` → `null`; `PRIM(k)` → `{PRIM_KIND_TO_VALUE_TAG[k],
    ///   payload word}` (loaded after the tag word's acquire, so it is the published payload).
    /// Stride 0 (any other array) and an out-of-range index take `jit_array_get` (same exception).
    fn tr_array_get_ref(&mut self, dst: u32, arr: u32, idx: u32) {
        use cranelift_codegen::ir::condcodes::IntCC;
        use crate::metadata::types::ref_word::{KIND_MASK, KIND_TO_VALUE_TAG, SLOW_PATH_TAG};
        use crate::metadata::types::tparam_cell::PRIM_KIND_TO_VALUE_TAG;
        let (data_ptr, len, info) = match self.hoisted_ref_arrays.get(&arr) {
            Some(&h) => h,
            None => super::hoist::ref_array_data(
                self.builder, self.ptr, self.frame_val, self.ctx_val, self.hr_array_ref_data, arr),
        };
        let stride = self.builder.ins().band_imm_s(info, 0xFF);
        let idx_addr = reg_addr(self.builder, self.regs_base, idx);
        let idx_v = load_payload_i64(self.builder, idx_addr);
        let [helper_blk, check_blk, load_blk, ref_blk, store_ref_blk, marker_blk, zero_blk, prim_blk, val_blk, done_blk] =
            std::array::from_fn(|_| self.builder.create_block());
        let null_tag = self.builder.ins().iconst(types::I8, 5);
        // stride 0 (not a REF / MIXED array) or index out of range → helper (it raises).
        let no_fast = self.builder.ins().icmp_imm_s(IntCC::Equal, stride, 0);
        self.builder.ins().brif(no_fast, helper_blk, &[], check_blk, &[]);
        self.builder.switch_to_block(check_blk);
        let oob = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx_v, len);
        self.builder.ins().brif(oob, helper_blk, &[], load_blk, &[]);
        // Acquire-load the element's (tag) word.
        self.builder.switch_to_block(load_blk);
        let elem_off = self.builder.ins().imul(idx_v, stride);
        let elem_addr = self.builder.ins().iadd(data_ptr, elem_off);
        let raw = self.builder.ins().atomic_load(types::I64, MemFlagsData::trusted(), elem_addr);
        let dst_addr = reg_addr(self.builder, self.regs_base, dst);
        let kind = self.builder.ins().band_imm_u(raw, KIND_MASK as i64);
        let is_word = self.builder.ins().icmp_imm_u(IntCC::NotEqual, kind, 0);
        self.builder.ins().brif(is_word, ref_blk, &[], marker_blk, &[]);
        // A reference word.
        self.builder.switch_to_block(ref_blk);
        let table = self.builder.ins().iconst(self.ptr, KIND_TO_VALUE_TAG.as_ptr() as i64);
        let tag_addr = self.builder.ins().iadd(table, kind);
        let tag_i8 = self.builder.ins().load(types::I8, MemFlagsData::trusted(), tag_addr, 0);
        let tag_w = self.builder.ins().uextend(types::I32, tag_i8);
        let boxed = self.builder.ins().icmp_imm_u(IntCC::Equal, tag_w, SLOW_PATH_TAG as i64);
        self.builder.ins().brif(boxed, helper_blk, &[], store_ref_blk, &[]);
        self.builder.switch_to_block(store_ref_blk);
        let payload = self.builder.ins().band_imm_s(raw, !(KIND_MASK as i64));
        store_tagged(self.builder, dst_addr, tag_i8, payload);
        self.builder.ins().jump(done_blk, &[]);
        // Kind bits 0: the never-written word, or a MIXED primitive marker.
        self.builder.switch_to_block(marker_blk);
        let ptable = self.builder.ins().iconst(self.ptr, PRIM_KIND_TO_VALUE_TAG.as_ptr() as i64);
        let is_zero = self.builder.ins().icmp_imm_u(IntCC::Equal, raw, 0);
        self.builder.ins().brif(is_zero, zero_blk, &[], prim_blk, &[]);
        self.builder.switch_to_block(zero_blk);
        let zk = self.builder.ins().ushr_imm(info, 8);
        let zk = self.builder.ins().band_imm_u(zk, 0xF);
        let ztag_addr = self.builder.ins().iadd(ptable, zk);
        let ztag = self.builder.ins().load(types::I8, MemFlagsData::trusted(), ztag_addr, 0);
        let zero_payload = self.builder.ins().iconst(types::I64, 0);
        store_tagged(self.builder, dst_addr, ztag, zero_payload);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(prim_blk);
        let null_flag = self.builder.ins().band_imm_u(raw, 8);
        let is_null = self.builder.ins().icmp_imm_u(IntCC::NotEqual, null_flag, 0);
        let null_blk = self.builder.create_block();
        self.builder.ins().brif(is_null, null_blk, &[], val_blk, &[]);
        self.builder.switch_to_block(null_blk);
        let zp = self.builder.ins().iconst(types::I64, 0);
        store_tagged(self.builder, dst_addr, null_tag, zp);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(val_blk);
        let k = self.builder.ins().ushr_imm(raw, 4);
        let k = self.builder.ins().band_imm_u(k, 0xF);
        let ptag_addr = self.builder.ins().iadd(ptable, k);
        let ptag = self.builder.ins().load(types::I8, MemFlagsData::trusted(), ptag_addr, 0);
        let bits = self.builder.ins().load(types::I64, MemFlagsData::trusted(), elem_addr, 8);
        store_tagged(self.builder, dst_addr, ptag, bits);
        self.builder.ins().jump(done_blk, &[]);
        // Helper: a box, any other array, or the exception.
        self.builder.switch_to_block(helper_blk);
        let d = self.ri(dst); let a = self.ri(arr); let i = self.ri(idx);
        let inst = self.builder.ins().call(self.hr_array_get, &[self.frame_val, self.ctx_val, d, a, i]);
        let ret = self.builder.inst_results(inst)[0];
        self.check(ret);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(done_blk);
    }
}
