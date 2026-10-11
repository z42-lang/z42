//! Inline string `Length` / `CharAt` (perf-str-jit-intrinsics).
//!
//! `s.Length` (`vcall get_Length`), `s.CharAt(i)` and `s[i]` (`call
//! Std.String.CharAt` / `get_Item`) are the innermost operations of every
//! string-processing loop written in z42. Even with the `[Native]` forwarder
//! short-circuit (`jit::forward`) each one is a helper call that clones the
//! arguments into a `SmallVec`, dispatches the builtin by id and boxes the
//! result. For a string whose block carries the all-ASCII bit the answer is two
//! or three loads, so the translator emits exactly that:
//!
//! ```text
//!   tag(recv) == Str?                       ─ no ─┐
//!   [static Call] package-init gate closed?  ─ no ─┤
//!   hdr = payload(recv) & HANDLE_ADDR_MASK         │
//!   hdr.flags & ASCII_BIT?                   ─ no ─┤
//!   len = hdr.size                                 │
//!   Length → dst = I64(len)                        │
//!   CharAt → (u64)i < len?                   ─ no ─┤
//!            dst = Char(hdr.data[i])               │
//!   done ◄──────────── slow: the original helper ◄─┘
//! ```
//!
//! The slow path is the site's unmodified helper sequence, so a null receiver,
//! a non-ASCII (or not-known-ASCII) string, an out-of-range index and a
//! negative index all produce the very same result / exception text as before.
//!
//! **Recognition** is by what the call target *forwards to*, never by name: the
//! target must be a builtin forwarder ([`crate::jit::forward::builtin_forward`] /
//! `chained_forward`) of `__str_length` / `__str_char_at` whose owner type has no
//! static constructor ([`crate::jit::forward::ForwardLookup`]).
//! - static `Call`: the site's token must already be bound at translate time;
//! - `VCall`: the receiver must be statically `string`; the target is the
//!   function `resolve_vcall` would pick first for a string receiver
//!   (`Std.String.<method>`), and the inline path additionally requires the
//!   runtime tag to be `Str`.
//!
//! **Barriers.** `jit_vcall` runs none, so the `VCall` form needs no gate.
//! `jit_call` runs the package-init barrier then the callee's type-init barrier
//! before dispatching. The latter is a no-op for an owner without a static
//! constructor (checked at translate time); the former is a no-op exactly when
//! no package initializer is pending or failed — the inline gate reads those two
//! counters (`JitModuleCtx::cctors`) and otherwise takes the helper path.
//!
//! **GC.** Nothing on the fast path allocates or reaches a safepoint, and the
//! receiver handle stays in its frame register (a root) throughout, so the block
//! cannot be collected under the loads; the heap is non-moving.
//!
//! The fast path stamps no call-site offset and skips the post-`Call` safepoint:
//! no z42 frame is entered and nothing on it can throw.

use super::*;
use super::ctx::TxCtx;
use crate::jit::forward::StrIntrinsic;

impl<'a, 'b> TxCtx<'a, 'b> {
    /// The intrinsic a static `Call` site can be emitted as, if any.
    pub(super) fn str_intrinsic_of_call(&self, insn: &CallInsn) -> Option<StrIntrinsic> {
        if !STR_INTRINSICS_SUPPORTED || !insn.method_type_args.is_empty() { return None; }
        let mid = method_id_at(self.func, self.block_idx, self.instr_idx);
        if mid == crate::metadata::tokens::UNRESOLVED { return None; }
        let k = StrIntrinsic::of_builtin(self.callees.forward_of_id(mid)?)?;
        self.str_operands_ok(k, &insn.args).then_some(k)
    }

    /// The intrinsic a `VCall` site on a statically-`string` receiver can be
    /// emitted as, if any.
    pub(super) fn str_intrinsic_of_vcall(&self, insn: &VCallInsn) -> Option<StrIntrinsic> {
        if !STR_INTRINSICS_SUPPORTED || !insn.method_type_args.is_empty()
            || !is_typed(self.func, insn.obj, IrType::Str) {
            return None;
        }
        let target = format!("{}.{}", crate::metadata::well_known_names::STD_STRING, insn.method);
        let k = StrIntrinsic::of_builtin(self.callees.forward_of_name(&target)?)?;
        // Receiver + arguments, as the builtin sees them.
        (insn.args.len() + 1 == k.argc()
            && insn.args.first().is_none_or(|&i| idx_int_ok(self.func, i))).then_some(k)
    }

    /// `operands` = receiver followed by the call's arguments.
    fn str_operands_ok(&self, k: StrIntrinsic, operands: &[u32]) -> bool {
        operands.len() == k.argc()
            && (k == StrIntrinsic::Length || idx_int_ok(self.func, operands[1]))
    }

    /// Emit intrinsic `k` writing `dst` from `operands` (receiver, then the index
    /// for `CharAt`), with `slow` — the site's own helper sequence — as the
    /// fallback. `gated`: also require the package-init barrier gate to be closed
    /// (static `Call` sites; see the module docs).
    pub(super) fn emit_str_intrinsic(
        &mut self, k: StrIntrinsic, dst: u32, operands: &[u32], gated: bool,
        slow: impl FnOnce(&mut Self),
    ) {
        use crate::metadata::vstr::jit_layout as sl;
        let flags = MemFlagsData::trusted();
        let slow_blk = self.builder.create_block();
        let done_blk = self.builder.create_block();

        // tag(recv) == Str
        let recv_addr = reg_addr(self.builder, self.regs_base, operands[0]);
        let tag = load_tag(self.builder, recv_addr);
        let is_str = self.builder.ins().icmp_imm_s(IntCC::Equal, tag, super::super::reg_access::TAG_STR as i64);
        self.continue_if(is_str, slow_blk);

        if gated { self.emit_module_init_gate(slow_blk); }

        // hdr = handle & mask; ASCII bit set?
        let bits = load_payload_i64(self.builder, recv_addr);
        let hdr = self.builder.ins().band_imm_s(bits, sl::HANDLE_ADDR_MASK as i64);
        let tflags = self.builder.ins().uload8(types::I32, flags, hdr, sl::FLAGS_OFFSET);
        let ascii = self.builder.ins().band_imm_s(tflags, sl::ASCII_BIT as i64);
        self.continue_if(ascii, slow_blk);

        let len = self.builder.ins().uload32(flags, hdr, sl::LEN_OFFSET);
        let dst_addr = reg_addr(self.builder, self.regs_base, dst);
        match k {
            StrIntrinsic::Length => store_const_tag(self.builder, dst_addr, TAG_I64, len),
            StrIntrinsic::CharAt => {
                let idx_addr = reg_addr(self.builder, self.regs_base, operands[1]);
                let idx = load_payload_i64(self.builder, idx_addr);
                // Unsigned: a negative index is out of range too (the helper reports it).
                let in_range = self.builder.ins().icmp(IntCC::UnsignedLessThan, idx, len);
                self.continue_if(in_range, slow_blk);
                let at = self.builder.ins().iadd(hdr, idx);
                let byte = self.builder.ins().uload8(types::I32, flags, at, sl::DATA_OFFSET);
                store_const_tag(self.builder, dst_addr, TAG_CHAR, byte);
            }
        }
        self.builder.ins().jump(done_blk, &[]);

        self.builder.switch_to_block(slow_blk);
        slow(self);
        self.builder.ins().jump(done_blk, &[]);
        self.builder.switch_to_block(done_blk);
    }

    /// Branch to `otherwise` unless `cond` is non-zero; continue in a fresh block.
    fn continue_if(&mut self, cond: cranelift_codegen::ir::Value, otherwise: cranelift_codegen::ir::Block) {
        let next = self.builder.create_block();
        self.builder.ins().brif(cond, next, &[], otherwise, &[]);
        self.builder.switch_to_block(next);
    }

    /// `jit_call`'s package-init barrier is a no-op ⇔ no package initializer is
    /// pending or failed: take `slow_blk` unless both counters of the running VM's
    /// cctor registry are zero (no registry attached ⇒ slow).
    fn emit_module_init_gate(&mut self, slow_blk: cranelift_codegen::ir::Block) {
        use crate::vm_context::cctor::CctorRegistry;
        let flags = MemFlagsData::trusted();
        let reg = self.builder.ins().load(self.ptr, flags, self.ctx_val,
            crate::jit::frame::JIT_MODULE_CTX_CCTORS_OFFSET as i32);
        self.continue_if(reg, slow_blk);
        let pending = self.builder.ins().load(types::I64, flags, reg, CctorRegistry::MODULE_PENDING_OFFSET as i32);
        let failed = self.builder.ins().load(types::I64, flags, reg, CctorRegistry::MODULE_FAILED_OFFSET as i32);
        let any = self.builder.ins().bor(pending, failed);
        let next = self.builder.create_block();
        self.builder.ins().brif(any, slow_blk, &[], next, &[]);
        self.builder.switch_to_block(next);
    }
}

/// The inline layout (`vstr::jit_layout`) is spelled for 64-bit handles only.
const STR_INTRINSICS_SUPPORTED: bool = cfg!(target_pointer_width = "64");

#[cfg(test)]
#[path = "str_intrinsics_tests.rs"]
mod str_intrinsics_tests;
