//! `TxCtx` — the per-block translation context threaded through every
//! instruction handler.
//!
//! `translate_function` (mod.rs) builds one `TxCtx` per z42 block after the
//! function-wide prologue, then dispatches each instruction to a `tr_*` method
//! (in the category submodules `value` / `arith` / `compare` / `convert` /
//! `call` / `array` / `object` / `structs` / `control`). Splitting a single
//! 1770-line function into per-opcode methods needs the shared Cranelift state
//! (the `builder`, the block-local `cache`, the imported helper `FuncRef`s, the
//! hoisted loop-invariant maps, the per-block catch chain) carried in one place;
//! `TxCtx` is that place. The six former local macros (`ri!` / `str_val!` /
//! `regs_val!` / `check!` / `emit_dispatch_to_catch_or_return!` /
//! `emit_int_divrem!`) become inherent methods here — they can't move as free
//! macros because `macro_rules!` resolves captured identifiers at the definition
//! site, not the call site.

use super::*;
use cranelift_codegen::ir::{Block, FuncRef, Value};

/// Per-block translation context. `'a` bounds the borrows of function-wide state
/// (helper refs, promotion whitelist, hoisted maps) and the per-block catch
/// chain; `'b` is the Cranelift `FunctionBuilder`'s frontend lifetime.
pub(super) struct TxCtx<'a, 'b> {
    pub(super) builder: &'a mut FunctionBuilder<'b>,
    /// jit-unbox-regalloc Phase 2B block-local integer-scalar cache.
    pub(super) cache: &'a mut RegCache,
    pub(super) func: &'a Function,
    pub(super) promoted: &'a [bool],
    pub(super) cl_blocks: &'a [Block],
    pub(super) regs_base: Value,
    pub(super) frame_val: Value,
    pub(super) ctx_val: Value,
    pub(super) ptr: cranelift_codegen::ir::Type,
    pub(super) block_idx: usize,
    pub(super) instr_idx: usize,
    /// Wildcard single-covering-entry shortcut (unconditional jump on throw).
    pub(super) catch_info: Option<(Block, u32)>,
    /// All covering exception-table entries for this block, in source order.
    pub(super) catch_chain: &'a [(Block, u32, Option<&'a str>)],
    /// jit-inline-fastpaths: loop-invariant array (data_ptr, len, width).
    pub(super) hoisted_arrays: &'a std::collections::HashMap<u32, (Value, Value, Value)>,
    /// FieldGet/Set P5-B: loop-invariant primitive field (bytes_ptr, offset).
    pub(super) hoisted_fields: &'a std::collections::HashMap<(u32, String), (Value, Value)>,
    /// FieldGet T1-B: loop-invariant reference field (bytes_ptr, offset, tag).
    pub(super) hoisted_ref_fields: &'a std::collections::HashMap<(u32, String), (Value, Value, Value)>,
    // ── imported helper FuncRefs (per-function; named identically to the
    //    `let hr_x = imp!(..)` locals so mod.rs constructs via field shorthand) ──
    pub(super) hr_const_i32: FuncRef,
    pub(super) hr_const_i64: FuncRef,
    pub(super) hr_const_f64: FuncRef,
    pub(super) hr_const_bool: FuncRef,
    pub(super) hr_const_char: FuncRef,
    pub(super) hr_const_null: FuncRef,
    pub(super) hr_const_str: FuncRef,
    pub(super) hr_copy: FuncRef,
    pub(super) hr_add: FuncRef,
    pub(super) hr_sub: FuncRef,
    pub(super) hr_mul: FuncRef,
    pub(super) hr_div: FuncRef,
    pub(super) hr_rem: FuncRef,
    pub(super) hr_eq: FuncRef,
    pub(super) hr_ne: FuncRef,
    pub(super) hr_lt: FuncRef,
    pub(super) hr_le: FuncRef,
    pub(super) hr_gt: FuncRef,
    pub(super) hr_ge: FuncRef,
    pub(super) hr_and: FuncRef,
    pub(super) hr_or: FuncRef,
    pub(super) hr_not: FuncRef,
    pub(super) hr_neg: FuncRef,
    pub(super) hr_bit_and: FuncRef,
    pub(super) hr_bit_or: FuncRef,
    pub(super) hr_bit_xor: FuncRef,
    pub(super) hr_bit_not: FuncRef,
    pub(super) hr_shl: FuncRef,
    pub(super) hr_shr: FuncRef,
    pub(super) hr_str_concat: FuncRef,
    pub(super) hr_to_str: FuncRef,
    pub(super) hr_call: FuncRef,
    pub(super) hr_builtin: FuncRef,
    pub(super) hr_array_new: FuncRef,
    pub(super) hr_array_new_lit: FuncRef,
    pub(super) hr_array_get: FuncRef,
    pub(super) hr_array_data: FuncRef,
    pub(super) hr_array_set: FuncRef,
    pub(super) hr_array_len: FuncRef,
    pub(super) hr_obj_new: FuncRef,
    pub(super) hr_typeof: FuncRef,
    pub(super) hr_field_get: FuncRef,
    pub(super) hr_field_set: FuncRef,
    pub(super) hr_vcall: FuncRef,
    pub(super) hr_is_instance: FuncRef,
    pub(super) hr_as_cast: FuncRef,
    pub(super) hr_static_get: FuncRef,
    pub(super) hr_static_set: FuncRef,
    pub(super) hr_struct_alloc: FuncRef,
    pub(super) hr_struct_copy: FuncRef,
    pub(super) hr_struct_field_get_prim: FuncRef,
    pub(super) hr_struct_field_set_prim: FuncRef,
    pub(super) hr_get_bool: FuncRef,
    pub(super) hr_set_ret: FuncRef,
    pub(super) hr_throw: FuncRef,
    pub(super) hr_install_catch: FuncRef,
    pub(super) hr_match_catch_type: FuncRef,
    pub(super) hr_load_fn: FuncRef,
    pub(super) hr_mk_clos: FuncRef,
    pub(super) hr_call_indirect: FuncRef,
    pub(super) hr_load_fn_cached: FuncRef,
    pub(super) hr_default_of: FuncRef,
    pub(super) hr_convert: FuncRef,
    pub(super) hr_check_safepoint_slow: FuncRef,
}

impl<'a, 'b> TxCtx<'a, 'b> {
    /// Emit an i32 constant for a register index (former `ri!` macro).
    #[inline]
    pub(super) fn ri(&mut self, r: u32) -> Value {
        self.builder.ins().iconst(types::I32, r as i64)
    }

    /// Embed a `&str` as (ptr, len: i64) Cranelift constants (former `str_val!`).
    /// SAFETY: the string is 'static from the bytecode module (lives for the
    /// whole JitModule lifetime).
    #[inline]
    pub(super) fn str_val(&mut self, s: &str) -> (Value, Value) {
        let bytes: &'static [u8] = unsafe {
            std::slice::from_raw_parts(s.as_ptr(), s.len())
        };
        let sptr = self.builder.ins().iconst(self.ptr, bytes.as_ptr() as i64);
        let slen = self.builder.ins().iconst(types::I64, bytes.len() as i64);
        (sptr, slen)
    }

    /// Pack a `&[u32]` of register indices into (ptr, len) constants (former
    /// `regs_val!`). SAFETY: as `str_val`.
    #[inline]
    pub(super) fn regs_val(&mut self, regs: &[u32]) -> (Value, Value) {
        let slice: &'static [u32] = unsafe {
            std::slice::from_raw_parts(regs.as_ptr(), regs.len())
        };
        let rptr = self.builder.ins().iconst(self.ptr, slice.as_ptr() as i64);
        let rlen = self.builder.ins().iconst(types::I64, slice.len() as i64);
        (rptr, rlen)
    }

    /// After a helper call returning u8: branch to catch dispatch or return 1 on
    /// error (former `check!`). Blocks are NOT sealed here; `seal_all_blocks()`
    /// runs once after all edges are established.
    #[inline]
    pub(super) fn check(&mut self, ret: Value) {
        let ok_blk  = self.builder.create_block();
        let exc_blk = self.builder.create_block();
        self.builder.ins().brif(ret, exc_blk, &[], ok_blk, &[]);
        self.builder.switch_to_block(exc_blk);
        self.dispatch_to_catch_or_return();
        self.builder.switch_to_block(ok_blk);
    }

    /// Emit the exception dispatch: wildcard fast-path jump, typed/multi-catch
    /// probe chain, or return-1 propagation (former
    /// `emit_dispatch_to_catch_or_return!`). catch-by-generic-type (2026-05-06).
    pub(super) fn dispatch_to_catch_or_return(&mut self) {
        if let Some((catch_cl, catch_reg)) = self.catch_info {
            let creg = self.ri(catch_reg);
            self.builder.ins().call(self.hr_install_catch, &[self.frame_val, self.ctx_val, creg]);
            self.builder.ins().jump(catch_cl, &[]);
        } else if !self.catch_chain.is_empty() {
            let mut closed_by_wildcard = false;
            // Clone the (Copy) entries out first so the loop doesn't hold a borrow
            // of `self.catch_chain` while calling `&mut self` methods.
            let chain: Vec<(Block, u32, Option<&str>)> = self.catch_chain.to_vec();
            for (catch_cl, catch_reg, ty) in chain {
                match ty {
                    None => {
                        let creg = self.ri(catch_reg);
                        self.builder.ins().call(self.hr_install_catch, &[self.frame_val, self.ctx_val, creg]);
                        self.builder.ins().jump(catch_cl, &[]);
                        closed_by_wildcard = true;
                        break;
                    }
                    Some(t) => {
                        let (tptr, tlen) = self.str_val(t);
                        let inst = self.builder.ins().call(self.hr_match_catch_type, &[self.frame_val, self.ctx_val, tptr, tlen]);
                        let m = self.builder.inst_results(inst)[0];
                        let take_blk = self.builder.create_block();
                        let next_blk = self.builder.create_block();
                        self.builder.ins().brif(m, take_blk, &[], next_blk, &[]);
                        self.builder.switch_to_block(take_blk);
                        let creg = self.ri(catch_reg);
                        self.builder.ins().call(self.hr_install_catch, &[self.frame_val, self.ctx_val, creg]);
                        self.builder.ins().jump(catch_cl, &[]);
                        self.builder.switch_to_block(next_blk);
                    }
                }
            }
            if !closed_by_wildcard {
                let one = self.builder.ins().iconst(types::I8, 1);
                self.builder.ins().return_(&[one]);
            }
        } else {
            let one = self.builder.ins().iconst(types::I8, 1);
            self.builder.ins().return_(&[one]);
        }
    }

    /// Emit native integer `sdiv`/`srem` with a cold guard routing `b ∈ {0,-1}`
    /// to the scalar helper (former `emit_int_divrem!`). `hr_fn` is `hr_div` or
    /// `hr_rem`. SEMANTICS anchor: see mod.rs / `crate::semantics` (INT_DIV_ZERO,
    /// INT_DIV_GUARD).
    pub(super) fn emit_int_divrem(&mut self, dst: u32, a: u32, b: u32, hr_fn: FuncRef, is_div: bool) {
        let a_addr = reg_addr(self.builder, self.regs_base, a);
        let b_addr = reg_addr(self.builder, self.regs_base, b);
        let ai = load_payload_i64(self.builder, a_addr);
        let bi = load_payload_i64(self.builder, b_addr);
        let bp1 = self.builder.ins().iadd_imm(bi, 1);
        let danger = self.builder.ins().icmp_imm(IntCC::UnsignedLessThanOrEqual, bp1, 1);
        let cold_blk = self.builder.create_block();
        let fast_blk = self.builder.create_block();
        let done_blk = self.builder.create_block();
        self.builder.ins().brif(danger, cold_blk, &[], fast_blk, &[]);
        // cold: reuse the helper (throw on 0 / interp-parity on -1).
        self.builder.switch_to_block(cold_blk);
        let d = self.ri(dst); let av = self.ri(a); let bv = self.ri(b);
        let cinst = self.builder.ins().call(hr_fn, &[self.frame_val, self.ctx_val, d, av, bv]);
        let cret = self.builder.inst_results(cinst)[0];
        self.check(cret);
        self.builder.ins().jump(done_blk, &[]);
        // fast: native sdiv/srem at i64 width.
        self.builder.switch_to_block(fast_blk);
        let q = if is_div {
            self.builder.ins().sdiv(ai, bi)
        } else {
            self.builder.ins().srem(ai, bi)
        };
        let d_addr = reg_addr(self.builder, self.regs_base, dst);
        store_const_tag(self.builder, d_addr, TAG_I64, q);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(done_blk);
    }
}
