use super::*;

// ── Block decoding ────────────────────────────────────────────────────────────

pub(super) fn decode_block(data: &[u8], pool: &[String], id_map: &IdMap) -> Result<(Vec<Instruction>, Terminator)> {
    let mut c = Cursor::new(data);
    let mut instrs = Vec::new();

    while c.remaining() > 0 {
        let op  = c.read_u8()?;
        let typ = c.read_u8()?;
        let dst = c.read_u16()? as u32;

        match op {
            OP_RET     => return Ok((instrs, Terminator::Ret { reg: None })),
            OP_RET_VAL => return Ok((instrs, Terminator::Ret { reg: Some(dst) })),
            OP_BR      => {
                let lbl = c.read_u16()? as usize;
                return Ok((instrs, Terminator::Br { label: block_label(lbl) }));
            }
            OP_BR_COND => {
                let t = c.read_u16()? as usize;
                let f = c.read_u16()? as usize;
                return Ok((instrs, Terminator::BrCond {
                    cond: dst,
                    true_label:  block_label(t),
                    false_label: block_label(f),
                }));
            }
            OP_THROW => return Ok((instrs, Terminator::Throw { reg: dst })),
            _ => instrs.push(decode_instr(op, typ, dst, &mut c, pool, id_map)?),
        }
    }
    Ok((instrs, Terminator::Ret { reg: None }))
}

pub(super) fn decode_instr(op: u8, typ: u8, dst: u32, c: &mut Cursor, pool: &[String], id_map: &IdMap) -> Result<Instruction> {
    let instr = match op {
        OP_CONST_STR  => Instruction::ConstStr { dst, idx: c.read_u32()? },
        OP_CONST_I if typ == TAG_I64
                      => Instruction::ConstI64 { dst, val: c.read_i64()? },
        OP_CONST_I    => Instruction::ConstI32 { dst, val: c.read_i32()? },
        OP_CONST_F    => Instruction::ConstF64 { dst, val: c.read_f64()? },
        OP_CONST_BOOL => Instruction::ConstBool { dst, val: c.read_u8()? != 0 },
        OP_CONST_CHAR => {
            let code_point = c.read_i32()? as u32;
            Instruction::ConstChar { dst, val: char::from_u32(code_point).unwrap_or('\0') }
        }
        OP_CONST_NULL => Instruction::ConstNull { dst },
        OP_COPY       => Instruction::Copy { dst, src: c.read_u16()? as u32 },

        OP_ADD     => { let (a,b) = read_ab(c)?; Instruction::Add { dst, a, b } }
        OP_SUB     => { let (a,b) = read_ab(c)?; Instruction::Sub { dst, a, b } }
        OP_MUL     => { let (a,b) = read_ab(c)?; Instruction::Mul { dst, a, b } }
        OP_DIV     => { let (a,b) = read_ab(c)?; Instruction::Div { dst, a, b } }
        OP_REM     => { let (a,b) = read_ab(c)?; Instruction::Rem { dst, a, b } }
        OP_AND     => { let (a,b) = read_ab(c)?; Instruction::And { dst, a, b } }
        OP_OR      => { let (a,b) = read_ab(c)?; Instruction::Or  { dst, a, b } }
        OP_BIT_AND => { let (a,b) = read_ab(c)?; Instruction::BitAnd { dst, a, b } }
        OP_BIT_OR  => { let (a,b) = read_ab(c)?; Instruction::BitOr  { dst, a, b } }
        OP_BIT_XOR => { let (a,b) = read_ab(c)?; Instruction::BitXor { dst, a, b } }
        OP_SHL     => { let (a,b) = read_ab(c)?; Instruction::Shl { dst, a, b } }
        OP_SHR     => { let (a,b) = read_ab(c)?; Instruction::Shr { dst, a, b } }
        OP_STR_CONCAT => { let (a,b) = read_ab(c)?; Instruction::StrConcat { dst, a, b } }
        OP_EQ      => { let (a,b) = read_ab(c)?; Instruction::Eq { dst, a, b } }
        OP_NE      => { let (a,b) = read_ab(c)?; Instruction::Ne { dst, a, b } }
        OP_LT      => { let (a,b) = read_ab(c)?; Instruction::Lt { dst, a, b } }
        OP_LE      => { let (a,b) = read_ab(c)?; Instruction::Le { dst, a, b } }
        OP_GT      => { let (a,b) = read_ab(c)?; Instruction::Gt { dst, a, b } }
        OP_GE      => { let (a,b) = read_ab(c)?; Instruction::Ge { dst, a, b } }

        OP_NEG     => Instruction::Neg    { dst, src: c.read_u16()? as u32 },
        OP_NOT     => Instruction::Not    { dst, src: c.read_u16()? as u32 },
        OP_BIT_NOT => Instruction::BitNot { dst, src: c.read_u16()? as u32 },
        OP_TO_STR  => Instruction::ToStr  { dst, src: c.read_u16()? as u32 },
        OP_ARRAY_LEN => Instruction::ArrayLen { dst, arr: c.read_u16()? as u32 },

        OP_CALL => {
            // Phase 3 S3a (tokenize-ir-and-zbc-bump, 2026-05-09): IdMap dispatches
            // to v0.9 (pool_str) or v1.0 (IMPORT_BASE bit) decode based on header.
            let func = id_map.resolve_method(c.read_u32()?)?;
            let args = read_args(c)?;
            Instruction::Call(Box::new(CallInsn { dst, func, args, method_type_args: Box::from([]) }))
        }
        // add-generic-methods: Call carrying resolved method type_args (mta after
        // the method token, before args).
        OP_CALL_GENERIC => {
            let func = id_map.resolve_method(c.read_u32()?)?;
            let method_type_args = read_mta(c, pool)?;
            let args = read_args(c)?;
            Instruction::Call(Box::new(CallInsn { dst, func, args, method_type_args }))
        }
        OP_METHOD_TYPE_ARG => {
            let param_index = c.read_u8()?;
            Instruction::MethodTypeArg { dst, param_index }
        }
        OP_METHOD_DEFAULT => {
            let param_index = c.read_u8()?;
            Instruction::MethodDefault { dst, param_index }
        }
        OP_LOAD_FN => {
            let func = id_map.resolve_method(c.read_u32()?)?;
            Instruction::LoadFn(Box::new(LoadFnInsn { dst, func }))
        }
        OP_LOAD_FN_CACHED => {
            let func    = id_map.resolve_method(c.read_u32()?)?;
            let slot_id = c.read_u32()?;
            Instruction::LoadFnCached(Box::new(LoadFnCachedInsn { dst, func, slot_id }))
        }
        OP_CALL_INDIRECT => {
            let callee = c.read_u16()? as u32;
            let args   = read_args(c)?;
            Instruction::CallIndirect { dst, callee, args }
        }
        OP_MK_CLOS => {
            let fn_name     = id_map.resolve_method(c.read_u32()?)?;
            // 2026-05-02 impl-closure-l3-escape-stack: 1 byte flag
            let stack_alloc = c.read_u8()? != 0;
            let captures    = read_args(c)?;
            Instruction::MkClos(Box::new(MkClosInsn { dst, fn_name, captures, stack_alloc }))
        }
        OP_BUILTIN => {
            let name = pool_str_owned(pool, c.read_u32()?)?;
            let args = read_args(c)?;
            Instruction::Builtin(Box::new(BuiltinInsn { dst, name, args }))
        }
        OP_VCALL => {
            let method = pool_str_owned(pool, c.read_u32()?)?;
            let obj    = c.read_u16()? as u32;
            let args   = read_args(c)?;
            Instruction::VCall(Box::new(VCallInsn { dst, obj, method, args, method_type_args: Box::from([]) }))
        }
        OP_VCALL_GENERIC => {
            let method = pool_str_owned(pool, c.read_u32()?)?;
            let obj    = c.read_u16()? as u32;
            let method_type_args = read_mta(c, pool)?;
            let args   = read_args(c)?;
            Instruction::VCall(Box::new(VCallInsn { dst, obj, method, args, method_type_args }))
        }
        OP_FIELD_GET => {
            let obj        = c.read_u16()? as u32;
            let field_name = pool_str_owned(pool, c.read_u32()?)?;
            Instruction::FieldGet(Box::new(FieldGetInsn { dst, obj, field_name }))
        }
        OP_FIELD_SET => {
            let obj        = c.read_u16()? as u32;
            let field_name = pool_str_owned(pool, c.read_u32()?)?;
            let val        = c.read_u16()? as u32;
            Instruction::FieldSet(Box::new(FieldSetInsn { obj, field_name, val }))
        }
        OP_STATIC_GET => Instruction::StaticGet(Box::new(StaticGetInsn { dst, field: pool_str_owned(pool, c.read_u32()?)? })),
        OP_STATIC_SET => {
            let field = pool_str_owned(pool, c.read_u32()?)?;
            let val   = c.read_u16()? as u32;
            Instruction::StaticSet(Box::new(StaticSetInsn { field, val }))
        }
        // add-struct-value-semantics Phase A: blob value type instructions
        // (byte layout mirrors z42 ZbcInstr encode).
        OP_STRUCT_ALLOC => {
            let type_name = pool_str_owned(pool, c.read_u32()?)?;
            let size = c.read_u32()?;
            Instruction::StructAlloc(Box::new(StructAllocInsn { dst, type_name, size }))
        }
        OP_STRUCT_COPY => {
            let src  = c.read_u16()? as u32;
            let size = c.read_u32()?;
            Instruction::StructCopy { dst, src, size }
        }
        OP_STRUCT_FIELD_GET_PRIM => {
            let base     = c.read_u16()? as u32;
            let byte_off = c.read_u32()?;
            let kind     = c.read_u8()?;
            Instruction::StructFieldGetPrim { dst, base, byte_off, kind }
        }
        OP_STRUCT_FIELD_SET_PRIM => {
            let base     = c.read_u16()? as u32;
            let byte_off = c.read_u32()?;
            let kind     = c.read_u8()?;
            let val      = c.read_u16()? as u32;
            Instruction::StructFieldSetPrim { base, byte_off, kind, val }
        }
        OP_OBJ_NEW => {
            let class_name = id_map.resolve_type(c.read_u32()?)?;
            let ctor_name  = id_map.resolve_method(c.read_u32()?)?;
            let args       = read_args(c)?;
            // D-8b-3 Phase 2: type_args list (resolved generic type-arguments)
            let t_count = c.read_u8()? as usize;
            let mut type_args = Vec::with_capacity(t_count);
            for _ in 0..t_count {
                type_args.push(pool_str_owned(pool, c.read_u32()?)?);
            }
            // add-escape-analysis-stack-alloc (zbc 1.29): trailing stack-alloc flag.
            let stack_alloc = c.read_u8()? != 0;
            Instruction::ObjNew(Box::new(ObjNewInsn { dst, class_name, ctor_name, args, type_args: type_args.into_boxed_slice(), stack_alloc }))
        }
        OP_TYPEOF => {
            // add-reflection-generic-type-definition: type_name + structured
            // generic args (mirrors ObjNew type_args encoding).
            let type_name = pool_str_owned(pool, c.read_u32()?)?;
            let t_count = c.read_u8()? as usize;
            let mut type_args = Vec::with_capacity(t_count);
            for _ in 0..t_count {
                type_args.push(pool_str_owned(pool, c.read_u32()?)?);
            }
            Instruction::Typeof(Box::new(TypeofInsn { dst, type_name, type_args: type_args.into_boxed_slice() }))
        }
        OP_IS_INSTANCE => {
            let obj        = c.read_u16()? as u32;
            let class_name = id_map.resolve_type(c.read_u32()?)?;
            Instruction::IsInstance(Box::new(IsInstanceInsn { dst, obj, class_name }))
        }
        OP_AS_CAST => {
            let obj        = c.read_u16()? as u32;
            let class_name = id_map.resolve_type(c.read_u32()?)?;
            Instruction::AsCast(Box::new(AsCastInsn { dst, obj, class_name }))
        }
        OP_ARRAY_NEW     => {
            let size = c.read_u16()? as u32;
            let elem_tag = c.read_u8()?;
            // add-reflection-array-element-type (zbc 1.16): element type FQ name.
            let et_idx = c.read_u32()?;
            let element_type = c.pool_str(pool, et_idx)?.to_owned();
            // add-escape-analysis-stack-alloc (zbc 1.29): trailing stack-alloc flag.
            let stack_alloc = c.read_u8()? != 0;
            // fix-generic-array-value-zero-init (zbc 1.37): trailing type-param ref.
            let type_param_kind = c.read_u8()?;
            let type_param_index = c.read_u16()? as i32 - 1;   // biased: 0 → -1 (none)
            Instruction::ArrayNew(Box::new(crate::metadata::bytecode::ArrayNewInsn { dst, size, elem_tag, element_type, stack_alloc, type_param_kind, type_param_index }))
        }
        OP_ARRAY_NEW_LIT => {
            let elems = read_args(c)?;
            let et_idx = c.read_u32()?;
            let element_type = c.pool_str(pool, et_idx)?.to_owned();
            // add-escape-analysis-stack-alloc (zbc 1.29): trailing stack-alloc flag.
            let stack_alloc = c.read_u8()? != 0;
            Instruction::ArrayNewLit(Box::new(crate::metadata::bytecode::ArrayNewLitInsn { dst, elems, element_type, stack_alloc }))
        }
        OP_ARRAY_GET     => {
            let arr = c.read_u16()? as u32;
            let idx = c.read_u16()? as u32;
            Instruction::ArrayGet { dst, arr, idx }
        }
        OP_ARRAY_SET     => {
            let arr = c.read_u16()? as u32;
            let idx = c.read_u16()? as u32;
            let val = c.read_u16()? as u32;
            Instruction::ArraySet { arr, idx, val }
        }

        OP_CALL_NATIVE => {
            let module    = pool_str_owned(pool, c.read_u32()?)?;
            let type_name = pool_str_owned(pool, c.read_u32()?)?;
            let symbol    = pool_str_owned(pool, c.read_u32()?)?;
            let args      = read_args(c)?;
            Instruction::CallNative(Box::new(CallNativeInsn { dst, module, type_name, symbol, args }))
        }
        OP_CALL_NATIVE_VTABLE => {
            let recv = c.read_u16()? as u32;
            let slot = c.read_u16()?;
            let args = read_args(c)?;
            Instruction::CallNativeVtable { dst, recv, vtable_slot: slot, args }
        }
        OP_PIN_PTR   => Instruction::PinPtr   { dst, src: c.read_u16()? as u32 },
        OP_UNPIN_PTR => Instruction::UnpinPtr { pinned: c.read_u16()? as u32 },

        // Spec impl-ref-out-in-runtime: address-load decoding (operand layout
        // mirrors C# `BinaryFormat/ZbcWriter.Instructions.cs`).
        OP_LOAD_LOCAL_ADDR => {
            let slot = c.read_u16()? as u32;
            Instruction::LoadLocalAddr { dst, slot }
        }
        OP_LOAD_ELEM_ADDR => {
            let arr = c.read_u16()? as u32;
            let idx = c.read_u16()? as u32;
            Instruction::LoadElemAddr { dst, arr, idx }
        }
        OP_LOAD_FIELD_ADDR => {
            let obj = c.read_u16()? as u32;
            let field_name = pool_str_owned(pool, c.read_u32()?)?;
            Instruction::LoadFieldAddr(Box::new(LoadFieldAddrInsn { dst, obj, field_name }))
        }

        // add-default-generic-typeparam (D-8b-3 Phase 2)
        OP_DEFAULT_OF => {
            let param_index = c.read_u8()?;
            Instruction::DefaultOf { dst, param_index }
        }

        // fix-numeric-cast-lowering (2026-05-13)
        OP_CONVERT => {
            let src = c.read_u16()? as u32;
            Instruction::Convert { dst, src, to_tag: typ }
        }

        _ => bail!("unknown opcode 0x{op:02X}"),
    };
    Ok(instr)
}

pub(super) fn read_ab(c: &mut Cursor) -> Result<(u32, u32)> {
    Ok((c.read_u16()? as u32, c.read_u16()? as u32))
}

pub(super) fn read_args(c: &mut Cursor) -> Result<Box<[u32]>> {
    let count = c.read_u8()? as usize;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count { args.push(c.read_u16()? as u32); }
    Ok(args.into_boxed_slice())
}

/// add-generic-methods: method type-argument list — count(u16) + count× pool idx(u32).
pub(super) fn read_mta(c: &mut Cursor, pool: &[String]) -> Result<Box<[String]>> {
    let count = c.read_u16()? as usize;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count { names.push(pool_str_owned(pool, c.read_u32()?)?); }
    Ok(names.into_boxed_slice())
}

pub(super) fn pool_str_owned(pool: &[String], idx: u32) -> Result<String> {
    pool.get(idx as usize)
        .map(|s| s.clone())
        .ok_or_else(|| anyhow::anyhow!("string pool index {} out of range", idx))
}

// ── String pool rebuild (ConstStr remap) ─────────────────────────────────────

// ── Conversion helpers ────────────────────────────────────────────────────────

/// Decode a u8 TypeTag to its canonical string. Kept as a debug / disasm
/// helper after 1.7 align-zbc-reader-writer-asymmetry made SIGS/TYPE carry
/// the authoritative string via str_idx. Reader no longer calls it on the
/// hot path; future linter / disasm tooling may.
#[allow(dead_code)]
pub(super) fn type_tag_to_str(tag: u8) -> &'static str {
    match tag {
        0x01 => "bool",
        0x02 => "i8",
        0x03 => "i16",
        0x04 => "i32",
        0x05 => "i64",
        0x06 => "u8",
        0x07 => "u16",
        0x08 => "u32",
        0x09 => "u64",
        0x0A => "f32",
        0x0B => "f64",
        0x0C => "char",
        0x0D => "str",
        0x20 => "object",
        0x21 => "array",
        _    => "void",
    }
}
