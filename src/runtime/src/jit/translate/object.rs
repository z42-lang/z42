//! Object new/typeof/field/vcall/is-instance/as-cast/static translation.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).
//! Each `tr_*` is dispatched from the driver's per-block instruction loop.

use super::*;
use super::ctx::TxCtx;

impl<'a, 'b> TxCtx<'a, 'b> {
    pub(super) fn tr_object(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
                Instruction::ObjNew(insn) => {
                    // add-escape-analysis-stack-alloc: JIT ignores stack_alloc in v1
                    // (heap-allocates); the optimization targets interp (interp-first).
                    let ObjNewInsn { dst, class_name, ctor_name, args, type_args, stack_alloc: _ } = &**insn;
                    // 2026-05-07 expand-jit-type-args: marshal `Vec<String>` as a
                    // `*const String` + count to `jit_obj_new`. The IR storage
                    // lives for the module lifetime, so the raw pointer is valid
                    // for the duration of all JIT-compiled calls.
                    let d = self.ri(*dst);
                    let (cp, cl) = self.str_val(class_name);
                    let (kp, kl) = self.str_val(ctor_name);
                    let (ap, al) = self.regs_val(args);
                    let tap = self.builder.ins().iconst(self.ptr, type_args.as_ptr() as i64);
                    let tac = self.builder.ins().iconst(types::I64, type_args.len() as i64);
                    // cache-ctorless-objnew: bake the per-site mark's address (stable
                    // through `Function.resolved`, like the FieldIC pointer below).
                    let cm = ctorless_mark_ptr_at(self.func, self.block_idx, self.instr_idx);
                    let cmv = self.builder.ins().iconst(self.ptr, cm as i64);
                    let inst = self.builder.ins().call(self.hr_obj_new,
                        &[self.frame_val, self.ctx_val, d, cp, cl, kp, kl, ap, al, tap, tac, cmv]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::Typeof(insn) => {
                    // add-reflection-generic-type-definition: marshal type_name +
                    // the IR `type_args: Box<[String]>` storage as `*const String`
                    // + count (mirrors ObjNew type_args). Helper can't throw.
                    let TypeofInsn { dst, type_name, type_args } = &**insn;
                    let d = self.ri(*dst);
                    let (np, nl) = self.str_val(type_name);
                    let tap = self.builder.ins().iconst(self.ptr, type_args.as_ptr() as i64);
                    let tac = self.builder.ins().iconst(types::I64, type_args.len() as i64);
                    self.builder.ins().call(self.hr_typeof, &[self.frame_val, self.ctx_val, d, np, nl, tap, tac]);
                }
                // formalize-jit-method-token Phase 2.E (2026-05-08): emit
                // FieldIC pointer as i64 const so helper can take IC fast
                // path on monomorphic sites. Pointer is stable through
                // Function.resolved (OnceLock-set, never overwritten).
                Instruction::FieldGet(insn) => {
                    let FieldGetInsn { dst, obj, field_name } = &**insn;
                    // P5-B: inline-primitive field of a hoisted (never-reassigned)
                    // object → native width-aware byte load + widen into the 16B
                    // register (mirrors `decode_prim`). `off < 0` (null / non-object /
                    // field-not-found / reference / struct root / string / layout
                    // mismatch) falls back to jit_field_get (Str.Length / Array.Length /
                    // null-throw / field-not-found→Null all preserved). Paths converge.
                    let hoisted = self.hoisted_fields.get(&(*obj, field_name.clone())).copied();
                    if let (Some(fk), Some((bytes_ptr, off))) =
                        (field_prim_kind(self.func, *dst), hoisted)
                    {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let bad = self.builder.ins().icmp_imm(IntCC::SignedLessThan, off, 0);
                        let fb_blk = self.builder.create_block();
                        let native_blk = self.builder.create_block();
                        let cont_blk = self.builder.create_block();
                        self.builder.ins().brif(bad, fb_blk, &[], native_blk, &[]);
                        // fallback: full helper (may continue OR throw via check!).
                        self.builder.switch_to_block(fb_blk);
                        let d = self.ri(*dst); let o = self.ri(*obj);
                        let (fp, fl) = self.str_val(field_name);
                        let ic_ptr = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                        let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                        let inst = self.builder.ins().call(self.hr_field_get, &[self.frame_val, self.ctx_val, d, o, fp, fl, ic_val]);
                        let ret = self.builder.inst_results(inst)[0];
                        self.check(ret);
                        self.builder.ins().jump(cont_blk, &[]);
                        // native byte load at bytes_ptr+off, widen per field type, store reg.
                        self.builder.switch_to_block(native_blk);
                        let elem_addr = self.builder.ins().iadd(bytes_ptr, off);
                        let raw = self.builder.ins().load(fk.load_ty, MemFlags::trusted(), elem_addr, 0);
                        let payload = match fk.ext {
                            FieldExt::Sext  => self.builder.ins().sextend(types::I64, raw),
                            FieldExt::Uext  => self.builder.ins().uextend(types::I64, raw),
                            FieldExt::Keep | FieldExt::Float => raw, // I64 / F64 stored as-is
                        };
                        let dst_addr = reg_addr(self.builder, self.regs_base, *dst);
                        let tag_c = self.builder.ins().iconst(types::I8, fk.reg_tag);
                        store_tagged(self.builder, dst_addr, tag_c, payload);
                        self.builder.ins().jump(cont_blk, &[]);
                        self.builder.switch_to_block(cont_blk);
                    } else if let Some((bytes_ptr, off, tag)) =
                        self.hoisted_ref_fields.get(&(*obj, field_name.clone())).copied()
                    {
                        // T1-B: byte-inlined reference field of a hoisted (never-
                        // reassigned) object → native 8B tagged-pointer load, then
                        // `raw==0 ? Value::Null : Value::Object/Array{tag, raw}` — byte-
                        // identical to `read_inline_ref` + the helper's register store.
                        // No write barrier (read only). `off < 0` (non-object receiver /
                        // null / field-not-found / side-table ref / struct root) → helper.
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let bad = self.builder.ins().icmp_imm(IntCC::SignedLessThan, off, 0);
                        let fb_blk = self.builder.create_block();
                        let native_blk = self.builder.create_block();
                        let null_blk = self.builder.create_block();
                        let store_blk = self.builder.create_block();
                        let cont_blk = self.builder.create_block();
                        self.builder.ins().brif(bad, fb_blk, &[], native_blk, &[]);
                        // fallback: full helper (may continue OR throw via check!).
                        self.builder.switch_to_block(fb_blk);
                        let d = self.ri(*dst); let o = self.ri(*obj);
                        let (fp, fl) = self.str_val(field_name);
                        let ic_ptr = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                        let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                        let inst = self.builder.ins().call(self.hr_field_get, &[self.frame_val, self.ctx_val, d, o, fp, fl, ic_val]);
                        let ret = self.builder.inst_results(inst)[0];
                        self.check(ret);
                        self.builder.ins().jump(cont_blk, &[]);
                        // native: load the 8B tagged pointer at bytes_ptr+off.
                        self.builder.switch_to_block(native_blk);
                        let dst_addr = reg_addr(self.builder, self.regs_base, *dst);
                        let elem_addr = self.builder.ins().iadd(bytes_ptr, off);
                        let raw = self.builder.ins().load(types::I64, MemFlags::trusted(), elem_addr, 0);
                        let is_null = self.builder.ins().icmp_imm(IntCC::Equal, raw, 0);
                        self.builder.ins().brif(is_null, null_blk, &[], store_blk, &[]);
                        // 0 sentinel → Value::Null (tag alone; prior slot is Drop-free Ref).
                        self.builder.switch_to_block(null_blk);
                        store_tag_const(self.builder, dst_addr, TAG_NULL);
                        self.builder.ins().jump(cont_blk, &[]);
                        // non-null → Value::Object(7)/Array(6) with the raw pointer payload.
                        self.builder.switch_to_block(store_blk);
                        let tag_i8 = self.builder.ins().ireduce(types::I8, tag);
                        store_tagged(self.builder, dst_addr, tag_i8, raw);
                        self.builder.ins().jump(cont_blk, &[]);
                        self.builder.switch_to_block(cont_blk);
                    } else {
                        let d = self.ri(*dst); let o = self.ri(*obj);
                        let (fp, fl) = self.str_val(field_name);
                        let ic_ptr = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                        let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                        let inst = self.builder.ins().call(self.hr_field_get, &[self.frame_val, self.ctx_val, d, o, fp, fl, ic_val]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                Instruction::FieldSet(insn) => {
                    let FieldSetInsn { obj, field_name, val } = &**insn;
                    // P5-B: inline-primitive field on a hoisted object → native
                    // width-aware byte store at `bytes_ptr + off` (mirrors
                    // `encode_prim`; low `width` bytes of the register payload). No
                    // write barrier (primitive is not a heap ref). `off < 0` /
                    // reference / struct root / string / layout mismatch → jit_field_set
                    // (write barrier + full semantics). z42 has no implicit narrowing,
                    // so `val`'s static width equals the packed field width.
                    let hoisted = self.hoisted_fields.get(&(*obj, field_name.clone())).copied();
                    if let (Some(fk), Some((bytes_ptr, off))) =
                        (field_prim_kind(self.func, *val), hoisted)
                    {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let bad = self.builder.ins().icmp_imm(IntCC::SignedLessThan, off, 0);
                        let fb_blk = self.builder.create_block();
                        let native_blk = self.builder.create_block();
                        let cont_blk = self.builder.create_block();
                        self.builder.ins().brif(bad, fb_blk, &[], native_blk, &[]);
                        self.builder.switch_to_block(fb_blk);
                        let o = self.ri(*obj);
                        let (fp, fl) = self.str_val(field_name);
                        let v = self.ri(*val);
                        let ic_ptr = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                        let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                        let inst = self.builder.ins().call(self.hr_field_set, &[self.frame_val, self.ctx_val, o, fp, fl, v, ic_val]);
                        let ret = self.builder.inst_results(inst)[0];
                        self.check(ret);
                        self.builder.ins().jump(cont_blk, &[]);
                        self.builder.switch_to_block(native_blk);
                        let val_addr = reg_addr(self.builder, self.regs_base, *val);
                        let elem_addr = self.builder.ins().iadd(bytes_ptr, off);
                        if fk.ext == FieldExt::Float {
                            // f64 field: store the 8-byte payload verbatim.
                            let v = load_payload(self.builder, val_addr, types::F64);
                            self.builder.ins().store(MemFlags::trusted(), v, elem_addr, 0);
                        } else {
                            // integer field: take the low `width` bytes of the i64 payload
                            // (ireduce = the same truncation `encode_prim`'s `as uN` does).
                            let v64 = load_payload_i64(self.builder, val_addr);
                            let to_store = if fk.load_ty == types::I64 {
                                v64
                            } else {
                                self.builder.ins().ireduce(fk.load_ty, v64)
                            };
                            self.builder.ins().store(MemFlags::trusted(), to_store, elem_addr, 0);
                        }
                        self.builder.ins().jump(cont_blk, &[]);
                        self.builder.switch_to_block(cont_blk);
                    } else {
                        let o = self.ri(*obj);
                        let (fp, fl) = self.str_val(field_name);
                        let v = self.ri(*val);
                        let ic_ptr = field_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                        let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                        let inst = self.builder.ins().call(self.hr_field_set, &[self.frame_val, self.ctx_val, o, fp, fl, v, ic_val]);
                        let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                    }
                }
                // Phase 2.E: emit VCallIC pointer as trailing helper arg.
                Instruction::VCall(insn) => {
                    // add-generic-methods: generic VCall sites filtered by jit_unsupported_reason.
                    let VCallInsn { dst, obj, method, args, .. } = &**insn;
                    let d = self.ri(*dst); let o = self.ri(*obj);
                    let (mp, ml) = self.str_val(method);
                    let (ap, al) = self.regs_val(args);
                    let ic_ptr = vcall_ic_ptr_at(self.func, self.block_idx, self.instr_idx);
                    let ic_val = self.builder.ins().iconst(self.ptr, ic_ptr as i64);
                    // 2026-05-10 jit-stack-trace + span-column-propagate.
                    let (line, col) = crate::interp::resolve_line(self.func.line_table(), self.block_idx as u32, self.instr_idx as u32);
                    let line_val = self.builder.ins().iconst(types::I32, line as i64);
                    let col_val  = self.builder.ins().iconst(types::I32, col as i64);
                    let off_val = self.builder.ins().iconst(types::I32, self.func.linear_offset(self.block_idx as u32, self.instr_idx as u32) as i64);
                    let inst = self.builder.ins().call(self.hr_vcall, &[self.frame_val, self.ctx_val, d, o, mp, ml, ap, al, ic_val, line_val, col_val, off_val]);
                    let ret  = self.builder.inst_results(inst)[0]; self.check(ret);
                }
                Instruction::IsInstance(insn) => {
                    let IsInstanceInsn { dst, obj, class_name } = &**insn;
                    let d = self.ri(*dst); let o = self.ri(*obj);
                    let (cp, cl) = self.str_val(class_name);
                    self.builder.ins().call(self.hr_is_instance, &[self.frame_val, self.ctx_val, d, o, cp, cl]);
                }
                Instruction::AsCast(insn) => {
                    let AsCastInsn { dst, obj, class_name } = &**insn;
                    let d = self.ri(*dst); let o = self.ri(*obj);
                    let (cp, cl) = self.str_val(class_name);
                    self.builder.ins().call(self.hr_as_cast, &[self.frame_val, self.ctx_val, d, o, cp, cl]);
                }

                // Static fields
                // formalize-jit-method-token Phase 2 (2026-05-08): emit
                // pre-resolved StaticFieldId directly. make-vm-loading-lazy: a
                // lazily-loaded fn has no resolved table → id is UNRESOLVED and
                // the helper resolves the field by NAME (passed as self.ptr+len).
                Instruction::StaticGet(insn) => {
                    let StaticGetInsn { dst, field } = &**insn;
                    let d = self.ri(*dst);
                    let (fp, fl) = self.str_val(field);
                    let field_id = static_field_id_at(self.func, self.block_idx, self.instr_idx);
                    let id_val = self.builder.ins().iconst(types::I32, field_id as i64);
                    self.builder.ins().call(self.hr_static_get, &[self.frame_val, self.ctx_val, d, id_val, fp, fl]);
                }
                Instruction::StaticSet(insn) => {
                    let StaticSetInsn { field, val } = &**insn;
                    let v = self.ri(*val);
                    let (fp, fl) = self.str_val(field);
                    let field_id = static_field_id_at(self.func, self.block_idx, self.instr_idx);
                    let id_val = self.builder.ins().iconst(types::I32, field_id as i64);
                    self.builder.ins().call(self.hr_static_set, &[self.frame_val, self.ctx_val, id_val, v, fp, fl]);
                }

                // C1 native interop scaffold: JIT translation lands in
                // L3.M16. Refuse to compile a function that contains these
                // opcodes; caller should keep the function in Interp mode.
                // converge-vm-arith-semantics (H3): these arms are UNREACHABLE in
                // practice — `jit_unsupported_reason` (driven by `unsupported_reason`)
                // routes any function containing them to the interpreter before
                // translation. Kept for match exhaustiveness (runtime-rust.md: no `_`
                // wildcard). Each bail sources its reason from the single source of
                // truth `unsupported_reason(instr)` so the prescan list and these arms
                // can never name a different opcode set.
            _ => anyhow::bail!("tr_object: unexpected opcode in category dispatch"),
        }
        Ok(())
    }
}
