//! Native length reads shared by `ArrayLen` and array `FieldGet`.

use super::*;
use crate::object::array::jit_len_layout as layout;
use crate::jit::reg_access::TAG_ARRAY;

impl TxCtx<'_, '_> {
    pub(super) fn emit_array_length(
        &mut self, dst: u32, arr: u32, fallback: impl FnOnce(&mut Self),
    ) {
        let slow = self.builder.create_block();
        let done = self.builder.create_block();
        let flags = MemFlagsData::trusted();

        let receiver = reg_addr(self.builder, self.regs_base, arr);
        let tag = load_tag(self.builder, receiver);
        let is_array = self.builder.ins().icmp_imm_u(IntCC::Equal, tag, TAG_ARRAY as i64);
        self.continue_if(is_array, slow);

        // The frame keeps the receiver rooted, and there is no safepoint between the
        // guard and the length load. Keep ArrRef::borrow's stale-generation check.
        let handle = load_payload_i64(self.builder, receiver);
        let header = self.builder.ins().band_imm_s(handle, layout::HANDLE_ADDR_MASK as i64);
        let expected_gen = self.builder.ins().ushr_imm_u(handle, layout::HANDLE_GEN_SHIFT as i64);
        let generation = self.builder.ins().uload16(types::I64, flags, header, layout::GEN_OFFSET);
        let is_live = self.builder.ins().icmp(IntCC::Equal, generation, expected_gen);
        self.continue_if(is_live, slow);

        let kind = self.builder.ins().load(types::I64, flags, header, layout::KIND_OFFSET);
        let mode = self.builder.ins().band_imm_u(kind, layout::MODE_MASK_BITS);
        let fixed_width = self.builder.ins().icmp_imm_u(
            IntCC::UnsignedLessThan, mode, layout::STRUCT_MODE,
        );
        self.continue_if(fixed_width, slow);

        let shifts = self.builder.ins().iconst(types::I64, layout::PACKED_SHIFTS);
        let nibble = self.builder.ins().ishl_imm_u(mode, 2);
        let shift = self.builder.ins().ushr(shifts, nibble);
        let shift = self.builder.ins().band_imm_u(shift, 15);
        let size = self.builder.ins().uload32(flags, header, layout::SIZE_OFFSET);
        let length = self.builder.ins().ushr(size, shift);
        let dest = reg_addr(self.builder, self.regs_base, dst);
        store_const_tag(self.builder, dest, TAG_I64, length);
        self.builder.ins().jump(done, &[]);

        self.builder.switch_to_block(slow);
        fallback(self);
        self.builder.ins().jump(done, &[]);
        self.builder.switch_to_block(done);
    }
}
