//! Native monomorphic field IC: `FieldGet` / `FieldSet` on a heap object as an inline guard plus
//! one cell access, the `jit_field_get` / `jit_field_set` helper as the fallback.
//!
//! ```text
//!   tag(regs[obj]) == Object ?                          ─ no → helper   (null, string, stack object, …)
//!   hdr  = payload & HANDLE_ADDR_MASK                     block header (`obj_jit_layout`)
//!   tid  = load32 [load64 [hdr + 16] + type_id_offset]    receiver's TypeId (header → Arc<TypeDesc>)
//!   e    = load64 [ic]                                    the site's FieldIC entry 0
//!   (e & GUARD_MASK) == tid << 32 | kind ?              ─ no → helper   (other type / kind, empty)
//!   cell = hdr + 32 + (e >> 16 & 0xFFFF)                  the field cell
//! ```
//!
//! `kind` is the cell kind the site's static types expect ([`InlineCell`]): the exact primitive
//! tag of the destination / value register, or `FieldLoc::KIND_REF`. The helper path installs the
//! IC (the interpreter's sites share the same entries), so a site is native from its second
//! execution on. Polymorphic receivers hit only entry 0 inline; the helper scans all four.
//!
//! - **Reads**: a primitive is a plain width-aware load (the relaxed atomic of x86-64 / AArch64)
//!   widened into the register; a reference word is an acquire load whose kind is decoded through
//!   `ref_word::KIND_TO_VALUE_TAG` (kind 7, a boxed value → helper).
//! - **Primitive writes**: a plain narrow store. No barrier — no heap edge.
//! - **Reference writes**: the value is encoded through `ref_word::VALUE_TAG_TO_KIND` (a primitive,
//!   a stack handle, a `FuncRef` → helper) and release-stored — unless the helper's barrier could
//!   act, in which case the helper does the whole store: an SATB mark is running (the overwritten
//!   word must be recorded), or the heap is generational, the owner is at least `promotion_age`
//!   old and not yet remembered, and the value is a heap reference that is young (or an array,
//!   whose age lives in another region's header). So a stored old→young edge always reaches
//!   `write_barrier_field` exactly as on the helper path.
//!
//! **Hoisting** (`hoist.rs`): for a receiver register no instruction writes, a guard evaluated once
//! in the entry block yields `(hdr, off)` — `off < 0` when entry 0 does not match yet — and every
//! access to that field inside a loop starts from it, falling back to the per-access guard. The
//! pointer stays valid for the activation: blocks never move and the frame register keeps the
//! receiver alive.

use super::*;
use super::ctx::TxCtx;
use cranelift_codegen::ir::{Block, BlockArg, Value as CValue};
use crate::metadata::resolver::{FieldIC, FIELD_IC_GUARD_MASK};
use crate::metadata::types::{ref_word, FieldLoc};

/// The inline layout (`obj_jit_layout`) is spelled for 64-bit handles only.
pub(super) const FIELD_IC_SUPPORTED: bool = cfg!(target_pointer_width = "64");

/// The cell a site's static types expect.
#[derive(Clone, Copy)]
pub(super) enum InlineCell {
    Prim(FieldPrim),
    Ref,
}

impl InlineCell {
    /// The `FieldLoc` kind byte an IC entry must carry for this site to take the native path.
    pub(super) fn ic_kind(self) -> u8 {
        match self {
            InlineCell::Prim(p) => p.field_tag,
            InlineCell::Ref => FieldLoc::KIND_REF,
        }
    }

    /// From the static type of the register a `FieldGet` writes / a `FieldSet` stores.
    pub(super) fn of_reg(func: &Function, reg: u32) -> Option<InlineCell> {
        if !FIELD_IC_SUPPORTED {
            return None;
        }
        if let Some(p) = field_prim_kind(func, reg) {
            return Some(InlineCell::Prim(p));
        }
        (is_typed(func, reg, IrType::Ref) || is_typed(func, reg, IrType::Str)).then_some(InlineCell::Ref)
    }
}

/// Hoisted guards, keyed by (receiver register, field name, cell kind): the `(hdr, off)` the
/// entry-block guard produced (`off < 0` = no match at entry).
pub(super) type HoistedFields = std::collections::HashMap<(u32, String, u8), (CValue, CValue)>;

/// The guard: `regs[obj]` is an object whose `TypeId` `ic`'s entry 0 caches with cell kind `kind`
/// → jump to `hit(hdr, off)`; anything else jumps to `miss`. Leaves the builder in no block.
pub(super) fn emit_guard(
    b: &mut FunctionBuilder, ptr: cranelift_codegen::ir::Type, regs_base: CValue, obj: u32,
    ic: *const FieldIC, kind: u8, hit: Block, miss: Block,
) {
    use crate::metadata::types::obj_jit_layout as ol;
    let flags = MemFlagsData::trusted();
    let addr = reg_addr(b, regs_base, obj);
    let tag = load_tag(b, addr);
    let is_obj = b.ins().icmp_imm_s(IntCC::Equal, tag, super::super::reg_access::TAG_OBJECT as i64);
    let next = b.create_block();
    b.ins().brif(is_obj, next, &[], miss, &[]);
    b.switch_to_block(next);
    let bits = load_payload_i64(b, addr);
    let hdr = b.ins().band_imm_s(bits, ol::HANDLE_ADDR_MASK as i64);
    let td = b.ins().load(types::I64, flags, hdr, ol::TYPE_DESC_OFFSET);
    let tid = b.ins().uload32(flags, td, ol::type_id_offset());
    let icp = b.ins().iconst(ptr, ic as i64);
    // One relaxed load of the packed (TypeId, FieldLoc) word — the pair is never torn.
    let entry = b.ins().load(types::I64, flags, icp, 0);
    let masked = b.ins().band_imm_s(entry, FIELD_IC_GUARD_MASK as i64);
    let tid_hi = b.ins().ishl_imm_u(tid, 32);
    let want = b.ins().bor_imm_u(tid_hi, kind as i64);
    let ok = b.ins().icmp(IntCC::Equal, masked, want);
    let shifted = b.ins().ushr_imm_u(entry, FieldLoc::OFFSET_SHIFT as i64);
    let off = b.ins().band_imm_u(shifted, 0xFFFF);
    b.ins().brif(ok, hit, &[BlockArg::Value(hdr), BlockArg::Value(off)], miss, &[]);
}

/// A block taking `(hdr, off)` — the native access's entry.
fn cell_block(b: &mut FunctionBuilder) -> Block {
    let blk = b.create_block();
    b.append_block_param(blk, types::I64);
    b.append_block_param(blk, types::I64);
    blk
}

impl<'a, 'b> TxCtx<'a, 'b> {
    /// `FieldGet` with the native IC when the site has one and `dst`'s type picks a cell kind;
    /// the plain helper call otherwise.
    pub(super) fn emit_field_get(&mut self, insn: &FieldGetInsn) {
        let FieldGetInsn { dst, obj, field_name } = insn;
        let ic = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
        let cell = InlineCell::of_reg(self.func, *dst).filter(|_| !ic.is_null());
        let Some(cell) = cell else { return self.field_get_helper(insn, ic) };

        let fast = cell_block(self.builder);
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        self.enter_cell(*obj, field_name, cell, ic, fast, slow);

        self.builder.switch_to_block(fast);
        let hdr = self.builder.block_params(fast)[0];
        let off = self.builder.block_params(fast)[1];
        let cell_addr = self.builder.ins().iadd(hdr, off);
        let dst_addr = reg_addr(self.builder, self.regs_base, *dst);
        let flags = MemFlagsData::trusted();
        let fields_off = crate::metadata::types::obj_jit_layout::FIELDS_OFFSET;
        match cell {
            InlineCell::Prim(fk) => {
                let raw = self.builder.ins().load(fk.load_ty, flags, cell_addr, fields_off);
                let payload = match fk.ext {
                    FieldExt::Sext => self.builder.ins().sextend(types::I64, raw),
                    FieldExt::Uext => self.builder.ins().uextend(types::I64, raw),
                    FieldExt::Keep | FieldExt::Float => raw, // I64 / F64 as-is
                };
                let tag = self.builder.ins().iconst(types::I8, fk.reg_tag);
                store_tagged(self.builder, dst_addr, tag, payload);
            }
            InlineCell::Ref => {
                // Acquire-load the word, decode its kind; kind 7 (a boxed value) → helper.
                let word_addr = self.builder.ins().iadd_imm_s(cell_addr, fields_off as i64);
                let raw = self.builder.ins().atomic_load(types::I64, flags, word_addr);
                let kind = self.builder.ins().band_imm_u(raw, ref_word::KIND_MASK as i64);
                let table = self.builder.ins().iconst(self.ptr, ref_word::KIND_TO_VALUE_TAG.as_ptr() as i64);
                let tag_addr = self.builder.ins().iadd(table, kind);
                let tag = self.builder.ins().load(types::I8, flags, tag_addr, 0);
                let tag_w = self.builder.ins().uextend(types::I32, tag);
                let boxed = self.builder.ins().icmp_imm_u(IntCC::Equal, tag_w, ref_word::SLOW_PATH_TAG as i64);
                let store_blk = self.builder.create_block();
                self.builder.ins().brif(boxed, slow, &[], store_blk, &[]);
                self.builder.switch_to_block(store_blk);
                let payload = self.builder.ins().band_imm_s(raw, !(ref_word::KIND_MASK as i64));
                store_tagged(self.builder, dst_addr, tag, payload);
            }
        }
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(slow);
        self.field_get_helper(insn, ic);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
    }

    /// `FieldSet` — see [`Self::emit_field_get`].
    pub(super) fn emit_field_set(&mut self, insn: &FieldSetInsn) {
        let FieldSetInsn { obj, field_name, val } = insn;
        let ic = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
        let barrier = self.callees.barrier_bytes();
        let cell = InlineCell::of_reg(self.func, *val)
            .filter(|_| !ic.is_null())
            // A reference store needs the heap's barrier gate.
            .filter(|c| matches!(c, InlineCell::Prim(_)) || barrier.is_some());
        let Some(cell) = cell else { return self.field_set_helper(insn, ic) };

        let fast = cell_block(self.builder);
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        self.enter_cell(*obj, field_name, cell, ic, fast, slow);

        self.builder.switch_to_block(fast);
        let hdr = self.builder.block_params(fast)[0];
        let off = self.builder.block_params(fast)[1];
        let cell_addr = self.builder.ins().iadd(hdr, off);
        let val_addr = reg_addr(self.builder, self.regs_base, *val);
        let flags = MemFlagsData::trusted();
        let fields_off = crate::metadata::types::obj_jit_layout::FIELDS_OFFSET;
        match cell {
            InlineCell::Prim(fk) => {
                if fk.ext == FieldExt::Float {
                    let v = load_payload(self.builder, val_addr, types::F64);
                    self.builder.ins().store(flags, v, cell_addr, fields_off);
                } else {
                    // The low `width` bytes of the payload — `prim_to_bits`' truncation.
                    let v64 = load_payload_i64(self.builder, val_addr);
                    let v = if fk.load_ty == types::I64 { v64 } else { self.builder.ins().ireduce(fk.load_ty, v64) };
                    self.builder.ins().store(flags, v, cell_addr, fields_off);
                }
            }
            InlineCell::Ref => {
                let (mode, promo) = barrier.expect("checked above");
                self.emit_ref_store(hdr, cell_addr, val_addr, mode, promo, slow);
            }
        }
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(slow);
        self.field_set_helper(insn, ic);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
    }

    /// Route into `fast(hdr, off)`: from the entry block's hoisted guard when the receiver and
    /// field were hoisted (re-checking per access only when the hoist did not match), else from a
    /// per-access guard. Misses go to `slow`.
    fn enter_cell(&mut self, obj: u32, field_name: &str, cell: InlineCell, ic: *const FieldIC, fast: Block, slow: Block) {
        let kind = cell.ic_kind();
        if let Some(&(hdr, off)) = self.hoisted_fields.get(&(obj, field_name.to_string(), kind)) {
            let guard = self.builder.create_block();
            let missed = self.builder.ins().icmp_imm_s(IntCC::SignedLessThan, off, 0);
            self.builder.ins().brif(missed, guard, &[], fast, &[BlockArg::Value(hdr), BlockArg::Value(off)]);
            self.builder.switch_to_block(guard);
        }
        emit_guard(self.builder, self.ptr, self.regs_base, obj, ic, kind, fast, slow);
    }

    /// The reference store's fast path, at `cell_addr + FIELDS_OFFSET` of the owner at `hdr`; the
    /// cases the helper must handle (module docs) branch to `slow` before anything is written.
    fn emit_ref_store(
        &mut self, hdr: CValue, cell_addr: CValue, val_addr: CValue,
        mode: *const std::sync::atomic::AtomicU8, promo: *const std::sync::atomic::AtomicU8, slow: Block,
    ) {
        use crate::metadata::types::obj_jit_layout as ol;
        let flags = MemFlagsData::trusted();
        // word = payload | kind (a null's word is 0); unencodable → helper.
        let vtag = load_tag(self.builder, val_addr);
        let vtag_w = self.builder.ins().uextend(types::I64, vtag);
        let table = self.builder.ins().iconst(self.ptr, ref_word::VALUE_TAG_TO_KIND.as_ptr() as i64);
        let kaddr = self.builder.ins().iadd(table, vtag_w);
        let kind = self.builder.ins().uload8(types::I64, flags, kaddr, 0);
        let bad = self.builder.ins().icmp_imm_u(IntCC::Equal, kind, ref_word::SLOW_PATH_KIND as i64);
        self.continue_unless(bad, slow);
        let payload = load_payload_i64(self.builder, val_addr);
        let tagged = self.builder.ins().bor(payload, kind);
        let zero = self.builder.ins().iconst(types::I64, 0);
        let word = self.builder.ins().select(kind, tagged, zero);
        // An SATB mark is running → the helper records the overwritten word.
        let marking_addr = self.builder.ins().iconst(self.ptr, crate::gc::satb::marking_heaps_addr() as i64);
        let marking = self.builder.ins().load(self.ptr, flags, marking_addr, 0);
        self.continue_unless(marking, slow);

        // Could `write_barrier_field` act? Only for a heap reference (kind ≠ 0) stored into an
        // old, not yet remembered owner of a generational heap.
        let store = self.builder.create_block();
        let check = self.builder.create_block();
        self.builder.ins().brif(kind, check, &[], store, &[]);
        self.builder.switch_to_block(check);
        let mode_addr = self.builder.ins().iconst(self.ptr, mode as i64);
        let mode_v = self.builder.ins().uload8(types::I32, flags, mode_addr, 0);
        let generational = self.builder.ins().icmp_imm_u(
            IntCC::Equal, mode_v, crate::gc::GcMode::GenerationalMarkSweep as i64);
        let owner_chk = self.builder.create_block();
        self.builder.ins().brif(generational, owner_chk, &[], store, &[]);
        self.builder.switch_to_block(owner_chk);
        let promo_addr = self.builder.ins().iconst(self.ptr, promo as i64);
        let promo_v = self.builder.ins().uload8(types::I32, flags, promo_addr, 0);
        let tt = self.builder.ins().uload8(types::I32, flags, hdr, ol::TYPE_TAG_OFFSET);
        let age = self.age_of(tt);
        let old = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, age, promo_v);
        let remembered = self.builder.ins().band_imm_u(tt, ol::REMEMBERED_BIT);
        let not_remembered = self.builder.ins().icmp_imm_u(IntCC::Equal, remembered, 0);
        let needs_value = self.builder.ins().band(old, not_remembered);
        let value_chk = self.builder.create_block();
        self.builder.ins().brif(needs_value, value_chk, &[], store, &[]);
        // The owner is old and unremembered: only a young value needs the barrier. An array's age
        // is in its region entry, not a var-block header → helper.
        self.builder.switch_to_block(value_chk);
        let is_array = self.builder.ins().icmp_imm_u(IntCC::Equal, kind, ref_word::RK_ARRAY as i64);
        self.continue_unless(is_array, slow);
        let vhdr = self.builder.ins().band_imm_s(payload, ol::HANDLE_ADDR_MASK as i64);
        let vtt = self.builder.ins().uload8(types::I32, flags, vhdr, ol::TYPE_TAG_OFFSET);
        let vage = self.age_of(vtt);
        let young = self.builder.ins().icmp(IntCC::UnsignedLessThan, vage, promo_v);
        self.builder.ins().brif(young, slow, &[], store, &[]);

        self.builder.switch_to_block(store);
        let addr = self.builder.ins().iadd_imm_s(cell_addr, ol::FIELDS_OFFSET as i64);
        if cfg!(target_arch = "x86_64") {
            // x86-64 is TSO: a plain store is a release store (`atomic_store` would add `mfence`).
            self.builder.ins().store(flags, word, addr, 0);
        } else {
            self.builder.ins().atomic_store(flags, word, addr);
        }
    }

    /// `gen_age` out of a block header's `type_tag` byte (as i32).
    fn age_of(&mut self, tt: CValue) -> CValue {
        use crate::metadata::types::obj_jit_layout as ol;
        let shifted = self.builder.ins().ushr_imm_u(tt, ol::AGE_SHIFT);
        self.builder.ins().band_imm_u(shifted, ol::AGE_MASK)
    }

    /// Continue in a fresh block when `cond` is zero; branch to `otherwise` when it is not.
    pub(super) fn continue_unless(&mut self, cond: CValue, otherwise: Block) {
        let next = self.builder.create_block();
        self.builder.ins().brif(cond, otherwise, &[], next, &[]);
        self.builder.switch_to_block(next);
    }

    fn field_get_helper(&mut self, insn: &FieldGetInsn, ic: *const FieldIC) {
        let FieldGetInsn { dst, obj, field_name } = insn;
        let d = self.ri(*dst);
        let o = self.ri(*obj);
        let (fp, fl) = self.str_val(field_name);
        let ic_val = self.builder.ins().iconst(self.ptr, ic as i64);
        let inst = self.builder.ins().call(self.hr_field_get, &[self.frame_val, self.ctx_val, d, o, fp, fl, ic_val]);
        let ret = self.builder.inst_results(inst)[0];
        self.check(ret);
    }

    fn field_set_helper(&mut self, insn: &FieldSetInsn, ic: *const FieldIC) {
        let FieldSetInsn { obj, field_name, val } = insn;
        let o = self.ri(*obj);
        let (fp, fl) = self.str_val(field_name);
        let v = self.ri(*val);
        let ic_val = self.builder.ins().iconst(self.ptr, ic as i64);
        let inst = self.builder.ins().call(self.hr_field_set, &[self.frame_val, self.ctx_val, o, fp, fl, v, ic_val]);
        let ret = self.builder.inst_results(inst)[0];
        self.check(ret);
    }
}

#[cfg(test)]
#[path = "field_ic_tests.rs"]
mod field_ic_tests;
