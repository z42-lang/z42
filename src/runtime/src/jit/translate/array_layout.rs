//! Guarded reads of heap array headers for native length and element access.

use super::*;
use crate::object::array::jit_layout as layout;
use crate::jit::translate::reg_access::TAG_ARRAY;

impl TxCtx<'_, '_> {
    pub(super) fn emit_array_length(
        &mut self, dst: u32, arr: u32, fallback: impl FnOnce(&mut Self),
    ) {
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        let header = self.guard_array_header(arr, slow);
        let mode = self.array_mode(header);
        let fixed_width = self.builder.ins().icmp_imm_u(
            IntCC::UnsignedLessThan, mode, layout::STRUCT_MODE,
        );
        self.continue_if(fixed_width, slow);

        let shifts = self.builder.ins().iconst(types::I64, layout::PACKED_SHIFTS);
        let nibble = self.builder.ins().ishl_imm_u(mode, 2);
        let shift = self.builder.ins().ushr(shifts, nibble);
        let shift = self.builder.ins().band_imm_u(shift, 15);
        let size = self.array_payload_size(header);
        let length = self.builder.ins().ushr(size, shift);
        let dest = reg_addr(self.builder, self.regs_base, dst);
        store_const_tag(self.builder, dest, TAG_I64, length);
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(slow);
        fallback(self);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
    }

    /// Decode the shape for a per-access packed read. The mode must match the
    /// destination type before that type can determine the slot width and value tag.
    pub(super) fn packed_read_data(
        &mut self, arr: u32, mode: crate::object::ArrayMode, slow: cranelift_codegen::ir::Block,
    ) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
        let header = self.guard_array_header(arr, slow);
        let actual_mode = self.array_mode(header);
        let matches = self.builder.ins().icmp_imm_u(IntCC::Equal, actual_mode, mode as i64);
        self.continue_if(matches, slow);

        let size = self.array_payload_size(header);
        let len = self.builder.ins().ushr_imm_u(size, mode.shift() as i64);
        let data = self.builder.ins().iadd_imm_s(header, layout::DATA_OFFSET);
        (data, len)
    }

    fn guard_array_header(
        &mut self, arr: u32, slow: cranelift_codegen::ir::Block,
    ) -> cranelift_codegen::ir::Value {
        let flags = MemFlagsData::trusted();
        let receiver = reg_addr(self.builder, self.regs_base, arr);
        let tag = load_tag(self.builder, receiver);
        let is_array = self.builder.ins().icmp_imm_u(IntCC::Equal, tag, TAG_ARRAY as i64);
        self.continue_if(is_array, slow);

        // The frame keeps the receiver rooted, and there is no safepoint between the
        // guard and the array access. Keep ArrRef::borrow's stale-generation check.
        let handle = load_payload_i64(self.builder, receiver);
        let header = self.builder.ins().band_imm_s(handle, layout::HANDLE_ADDR_MASK as i64);
        let expected_gen = self.builder.ins().ushr_imm_u(handle, layout::HANDLE_GEN_SHIFT as i64);
        let generation = self.builder.ins().uload16(types::I64, flags, header, layout::GEN_OFFSET);
        let is_live = self.builder.ins().icmp(IntCC::Equal, generation, expected_gen);
        self.continue_if(is_live, slow);
        header
    }

    fn array_mode(&mut self, header: cranelift_codegen::ir::Value) -> cranelift_codegen::ir::Value {
        let kind = self.builder.ins().load(types::I64, MemFlagsData::trusted(), header, layout::KIND_OFFSET);
        self.builder.ins().band_imm_u(kind, layout::MODE_MASK_BITS)
    }

    fn array_payload_size(&mut self, header: cranelift_codegen::ir::Value) -> cranelift_codegen::ir::Value {
        self.builder.ins().uload32(MemFlagsData::trusted(), header, layout::SIZE_OFFSET)
    }
}
