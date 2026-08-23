//! Numeric cast (Convert) translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_convert(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::Convert { dst, src, to_tag } => {
                    // exec_value tag constants — keep in sync.
                    const T_I8:  u8 = 0x02;
                    const T_I16: u8 = 0x03;
                    const T_I32: u8 = 0x04;
                    const T_I64: u8 = 0x05;
                    const T_U8:  u8 = 0x06;
                    const T_U16: u8 = 0x07;
                    const T_U32: u8 = 0x08;
                    const T_U64: u8 = 0x09;
                    // jit-native-convert-float: float target tags.
                    const T_F32: u8 = 0x0A;
                    const T_F64: u8 = 0x0B;
                    let src_ty = self.func.reg_types
                        .get(*src as usize).copied().unwrap_or(IrType::Unknown);
                    let src_is_int = src_ty.is_integer();
                    let inline_int = src_is_int
                        && matches!(*to_tag,
                            T_I8 | T_I16 | T_I32 | T_I64 | T_U8 | T_U16 | T_U32 | T_U64);
                    // jit-native-convert-float (float→int): F64 source narrowed
                    // to an integer width via saturating fcvt.
                    let inline_f64_to_int = src_ty == IrType::F64
                        && matches!(*to_tag,
                            T_I8 | T_I16 | T_I32 | T_I64 | T_U8 | T_U16 | T_U32 | T_U64);
                    if inline_int {
                        emit_i64_convert(self.builder, self.regs_base, self.cache, self.promoted, *dst, *src, *to_tag);
                    } else if inline_f64_to_int {
                        emit_f64_to_int(self.builder, self.regs_base, self.cache, self.promoted, *dst, *src, *to_tag);
                    } else if src_is_int && matches!(*to_tag, T_F32 | T_F64) {
                        // int → f64 native (fcvt). src signedness picks
                        // fcvt_from_sint vs fcvt_from_uint.
                        let src_signed = matches!(src_ty,
                            IrType::I8 | IrType::I16 | IrType::I32 | IrType::I64);
                        emit_int_to_f64(self.builder, self.regs_base, *dst, *src, src_signed);
                    } else {
                        let d = self.ri(*dst);
                        let s = self.ri(*src);
                        let t = self.builder.ins().iconst(types::I32, *to_tag as i64);
                        let inst = self.builder.ins().call(self.hr_convert, &[self.frame_val, self.ctx_val, d, s, t]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }

                // impl-lambda-l2: lambdas / function references — JIT support
                // lands in a later iteration (L3+). Refuse to compile so the
                // caller keeps the function in Interp mode.
                // L3 closure helpers (impl-closure-l3-jit-complete).
                // Behaviour mirrors interp::exec_instr; see closure.md §6.
            _ => anyhow::bail!("tr_convert: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
