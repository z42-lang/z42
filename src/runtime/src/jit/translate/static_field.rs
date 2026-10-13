//! Native `StaticGet` / `StaticSet`: the static-field barrier's steady-state check plus one access to
//! the field's cell, the `jit_static_get` / `jit_static_set` helper as the fallback.
//!
//! ```text
//!   cell  = <baked>                                   the field's StaticCell (SegVec: never moves)
//!   load32 [cell + READY] (acquire) == load32 [epoch] ? ─ no → helper   (barrier not settled: it runs
//!                                                                     package initializers / the cctor)
//!   tag word / payload word                           `tparam_cell` protocol, as on the helper path
//! ```
//!
//! The gate is `VmContext::ensure_static_field_init`'s first line: the cell records the cctor
//! registry's barrier epoch at which the whole barrier was last found a no-op, and every event that
//! can make it act again moves the epoch. Both addresses belong to the VM's core, like the JIT itself.
//!
//! - **Reads** decode the tag word exactly as `tparam_cell::load` (one acquire load): a reference word
//!   → `{KIND_TO_VALUE_TAG[kind], word & !7}`; `PRIM(k)` → `{PRIM_KIND_TO_VALUE_TAG[k], payload}`. A
//!   box (kind 7), a `FuncRef` and every `null` (`0` / `NULL(k)`: the helper confirms the field and
//!   lazily zero-initialises a never-assigned value-type static) take the helper.
//! - **Primitive writes** (`I8..U64` / `F64` / `Bool` / `Char` value register, its tag re-checked at
//!   run time): the cell already holds that kind (`PRIM(k)`) → one payload store, no barrier. Any other
//!   state (never written, `null` of the kind, another kind, a box) → helper.
//! - **Reference writes** (`Ref` / `Str` value register): encoded through `VALUE_TAG_TO_KIND`
//!   (a primitive, a stack handle, a `FuncRef` → helper); an SATB mark running → helper (it records the
//!   overwritten word); a cell that never held a primitive (`0` or a direct reference word) → one CAS
//!   from the word just read, the protocol's own transition (a racing claim is never undone; a failed
//!   CAS → helper). Statics are GC roots scanned by every collection, so no generational barrier.
//!
//! Nothing between the gate and the access reaches a safepoint.

use super::*;
use super::ctx::TxCtx;
use cranelift_codegen::ir::{Block, Value as CValue};
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::types::{ref_word, tparam_cell};
use crate::vm_context::static_table::StaticCell;

/// The cell offsets are spelled for 64-bit words and pointers.
const STATIC_INLINE_SUPPORTED: bool = cfg!(target_pointer_width = "64");

/// What a `StaticSet` site's value register lets it store natively.
#[derive(Clone, Copy)]
enum StaticStore {
    /// A primitive: the register's `Value` tag, the cell kind it claims (`tparam_cell` `k`), and how to
    /// read the payload's significant bits.
    Prim { reg_tag: u8, kind: u64, payload_ty: types::Type },
    Ref,
}

impl StaticStore {
    fn of_reg(func: &Function, reg: u32) -> Option<StaticStore> {
        let (reg_tag, kind, payload_ty) = match func.reg_types.get(reg as usize)? {
            IrType::I8 | IrType::I16 | IrType::I32 | IrType::I64
            | IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64 => (TAG_I64, 1, types::I64),
            IrType::F64 => (TAG_F64, 2, types::I64),
            // `Value::Bool` / `Value::Char` only define the payload's low byte / low 4 bytes.
            IrType::Bool => (TAG_BOOL, 3, types::I8),
            IrType::Char => (TAG_CHAR, 4, types::I32),
            IrType::Ref | IrType::Str => return Some(StaticStore::Ref),
            _ => return None, // F32 / Void / Unknown → helper
        };
        Some(StaticStore::Prim { reg_tag, kind, payload_ty })
    }
}

impl<'a, 'b> TxCtx<'a, 'b> {
    /// The baked (cell, epoch) of this site's field, when it resolved to an id with a cell.
    fn static_site(&self, field_id: u32) -> Option<(*const StaticCell, *const std::sync::atomic::AtomicU32)> {
        if !STATIC_INLINE_SUPPORTED || field_id == UNRESOLVED {
            return None;
        }
        self.callees.static_cell(field_id)
    }

    /// Continue in a fresh block when the barrier recorded in `cell` is current; `slow` otherwise.
    fn emit_static_gate(&mut self, cell: CValue, epoch: *const std::sync::atomic::AtomicU32, slow: Block) {
        let flags = MemFlagsData::trusted();
        let epoch_addr = self.builder.ins().iconst(self.ptr, epoch as i64);
        let current = self.builder.ins().load(types::I32, flags, epoch_addr, 0);
        let ready_addr = self.builder.ins().iadd_imm_s(cell, StaticCell::JIT_READY_OFFSET as i64);
        let ready = self.builder.ins().atomic_load(types::I32, flags, ready_addr);
        let stale = self.builder.ins().icmp(IntCC::NotEqual, ready, current);
        self.continue_unless(stale, slow);
    }

    pub(super) fn emit_static_get(&mut self, insn: &StaticGetInsn) {
        let field_id = static_field_id_at(self.func, self.block_idx, self.instr_idx);
        let Some((cell, epoch)) = self.static_site(field_id) else {
            return self.static_get_helper(insn, field_id);
        };
        let flags = MemFlagsData::trusted();
        let [slow, ref_blk, marker_blk, prim_blk, done] = std::array::from_fn(|_| self.builder.create_block());
        let cell_v = self.builder.ins().iconst(self.ptr, cell as i64);
        self.emit_static_gate(cell_v, epoch, slow);

        let tag_addr = self.builder.ins().iadd_imm_s(cell_v, StaticCell::JIT_TAG_OFFSET as i64);
        let raw = self.builder.ins().atomic_load(types::I64, flags, tag_addr);
        let dst_addr = reg_addr(self.builder, self.regs_base, insn.dst);
        let kind = self.builder.ins().band_imm_u(raw, ref_word::KIND_MASK as i64);
        self.builder.ins().brif(kind, ref_blk, &[], marker_blk, &[]);

        // A reference word; a box / `FuncRef` → helper.
        self.builder.switch_to_block(ref_blk);
        let table = self.builder.ins().iconst(self.ptr, ref_word::KIND_TO_VALUE_TAG.as_ptr() as i64);
        let tag_at = self.builder.ins().iadd(table, kind);
        let tag = self.builder.ins().load(types::I8, flags, tag_at, 0);
        let tag_w = self.builder.ins().uextend(types::I32, tag);
        let slow_tag = self.builder.ins().icmp_imm_u(IntCC::Equal, tag_w, ref_word::SLOW_PATH_TAG as i64);
        self.continue_unless(slow_tag, slow);
        let payload = self.builder.ins().band_imm_s(raw, !(ref_word::KIND_MASK as i64));
        store_tagged(self.builder, dst_addr, tag, payload);
        self.builder.ins().jump(done, &[]);

        // Kind bits 0: never written (`0`) or `NULL(k)` → helper (it confirms the null); `PRIM(k)`.
        self.builder.switch_to_block(marker_blk);
        let unset = self.builder.ins().icmp_imm_u(IntCC::Equal, raw, 0);
        let null_flag = self.builder.ins().band_imm_u(raw, tparam_cell::NULL_FLAG as i64);
        let unset_w = self.builder.ins().uextend(types::I64, unset);
        let nullish = self.builder.ins().bor(unset_w, null_flag);
        self.builder.ins().brif(nullish, slow, &[], prim_blk, &[]);
        self.builder.switch_to_block(prim_blk);
        let k = self.builder.ins().ushr_imm_u(raw, 4);
        let k = self.builder.ins().band_imm_u(k, 0xF);
        let ptable = self.builder.ins().iconst(self.ptr, tparam_cell::PRIM_KIND_TO_VALUE_TAG.as_ptr() as i64);
        let ptag_at = self.builder.ins().iadd(ptable, k);
        let ptag = self.builder.ins().load(types::I8, flags, ptag_at, 0);
        // Loaded after the tag word's acquire: the payload published before `PRIM(k)`.
        let bits = self.builder.ins().load(types::I64, flags, cell_v, StaticCell::JIT_PAYLOAD_OFFSET);
        store_tagged(self.builder, dst_addr, ptag, bits);
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(slow);
        self.static_get_helper(insn, field_id);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
    }

    pub(super) fn emit_static_set(&mut self, insn: &StaticSetInsn) {
        let field_id = static_field_id_at(self.func, self.block_idx, self.instr_idx);
        let site = self.static_site(field_id).zip(StaticStore::of_reg(self.func, insn.val));
        let Some(((cell, epoch), store)) = site else {
            return self.static_set_helper(insn, field_id);
        };
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        let cell_v = self.builder.ins().iconst(self.ptr, cell as i64);
        self.emit_static_gate(cell_v, epoch, slow);
        let val_addr = reg_addr(self.builder, self.regs_base, insn.val);
        match store {
            StaticStore::Prim { reg_tag, kind, payload_ty } =>
                self.emit_static_prim_store(cell_v, val_addr, reg_tag, kind, payload_ty, slow),
            StaticStore::Ref => self.emit_static_ref_store(cell_v, val_addr, slow),
        }
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(slow);
        self.static_set_helper(insn, field_id);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
    }

    /// The register holds a `reg_tag` primitive and the cell already holds kind `kind`
    /// (`PRIM(kind)`) → only the payload word changes; otherwise `slow`.
    fn emit_static_prim_store(
        &mut self, cell: CValue, val_addr: CValue, reg_tag: u8, kind: u64, payload_ty: types::Type, slow: Block,
    ) {
        let flags = MemFlagsData::trusted();
        let vtag = load_tag(self.builder, val_addr);
        let other_tag = self.builder.ins().icmp_imm_u(IntCC::NotEqual, vtag, reg_tag as i64);
        self.continue_unless(other_tag, slow);
        let w0 = self.builder.ins().load(types::I64, flags, cell, StaticCell::JIT_TAG_OFFSET);
        let other_kind = self.builder.ins().icmp_imm_u(IntCC::NotEqual, w0, tparam_cell::prim_word(kind) as i64);
        self.continue_unless(other_kind, slow);
        let raw = load_payload(self.builder, val_addr, payload_ty);
        let bits = if payload_ty == types::I64 { raw } else { self.builder.ins().uextend(types::I64, raw) };
        self.builder.ins().store(flags, bits, cell, StaticCell::JIT_PAYLOAD_OFFSET);
    }

    /// A reference (or `null`) into a cell that never held a primitive: CAS the tag word from the
    /// word just read; every other case branches to `slow` before anything is written.
    fn emit_static_ref_store(&mut self, cell: CValue, val_addr: CValue, slow: Block) {
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
        // The cell never held a primitive: `0`, or a direct reference word (kind 1–6, not a box).
        let tag_addr = self.builder.ins().iadd_imm_s(cell, StaticCell::JIT_TAG_OFFSET as i64);
        let w0 = self.builder.ins().load(types::I64, flags, tag_addr, 0);
        let old_kind = self.builder.ins().band_imm_u(w0, ref_word::KIND_MASK as i64);
        let is_unset = self.builder.ins().icmp_imm_u(IntCC::Equal, w0, 0);
        let is_direct = self.builder.ins().icmp_imm_u(IntCC::NotEqual, old_kind, 0);
        let not_boxed = self.builder.ins().icmp_imm_u(IntCC::NotEqual, old_kind, ref_word::RK_BOXED_VALUE as i64);
        let direct = self.builder.ins().band(is_direct, not_boxed);
        let ok = self.builder.ins().bor(is_unset, direct);
        let not_ok = self.builder.ins().icmp_imm_u(IntCC::Equal, ok, 0);
        self.continue_unless(not_ok, slow);
        let seen = self.builder.ins().atomic_cas(flags, tag_addr, w0, word);
        let raced = self.builder.ins().icmp(IntCC::NotEqual, seen, w0);
        self.continue_unless(raced, slow);
    }

    fn static_get_helper(&mut self, insn: &StaticGetInsn, field_id: u32) {
        let StaticGetInsn { dst, field } = insn;
        let d = self.ri(*dst);
        let (fp, fl) = self.str_val(field);
        let id_val = self.builder.ins().iconst(types::I32, field_id as i64);
        // The helper may throw (a type initializer that failed, a missing field).
        let inst = self.builder.ins().call(self.hr_static_get, &[self.frame_val, self.ctx_val, d, id_val, fp, fl]);
        let ret = self.builder.inst_results(inst)[0];
        self.check(ret);
    }

    fn static_set_helper(&mut self, insn: &StaticSetInsn, field_id: u32) {
        let StaticSetInsn { field, val } = insn;
        let v = self.ri(*val);
        let (fp, fl) = self.str_val(field);
        let id_val = self.builder.ins().iconst(types::I32, field_id as i64);
        let inst = self.builder.ins().call(self.hr_static_set, &[self.frame_val, self.ctx_val, id_val, v, fp, fl]);
        let ret = self.builder.inst_results(inst)[0];
        self.check(ret);
    }
}

#[cfg(test)]
#[path = "static_field_tests.rs"]
mod static_field_tests;
