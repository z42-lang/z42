use super::*;

/// STRS segment-dict (zbc 1.21 / zpkg 0.25): unique `.`-split segments deduped once,
/// each string = sequence of segment indices, reconstructed via `join('.')`.
pub(super) fn read_strs(sec: &[u8]) -> Result<Vec<String>> {
    let mut c = Cursor::new(sec);
    let seg_count = c.read_u32()? as usize;
    let mut seg_dict: Vec<&str> = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        let len = c.read_varint()? as usize;
        let b = c.read_bytes(len)?;
        seg_dict.push(std::str::from_utf8(b)?);
    }
    let str_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(str_count);
    for _ in 0..str_count {
        let seg_n = c.read_varint()? as usize;
        let mut name = String::new();
        for j in 0..seg_n {
            let seg_idx = c.read_varint()? as usize;
            let seg = seg_dict.get(seg_idx).ok_or_else(|| {
                anyhow::anyhow!("STRS segment index {} out of range ({})", seg_idx, seg_count)
            })?;
            if j > 0 { name.push('.'); }
            name.push_str(seg);
        }
        result.push(name);
    }
    Ok(result)
}

// ── NSPC section ─────────────────────────────────────────────────────────────

pub(super) fn read_nspc(sec: &[u8]) -> Result<String> {
    if sec.len() < 2 { return Ok(String::new()); }
    let len = u16::from_le_bytes([sec[0], sec[1]]) as usize;
    if len == 0 || sec.len() < 2 + len { return Ok(String::new()); }
    Ok(std::str::from_utf8(&sec[2..2+len])?.to_owned())
}

// ── TYPE section ──────────────────────────────────────────────────────────────

pub(super) fn read_type(sec: &[u8], pool: &[String]) -> Result<Vec<ClassDesc>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut classes = Vec::with_capacity(count);
    for _ in 0..count {
        let name_idx = c.read_u32()?;
        let base_idx = c.read_u32()?;
        let fld_count = c.read_u16()? as usize;
        let name = c.pool_str(pool, name_idx)?.to_owned();
        let base_class = if base_idx == u32::MAX {
            None
        } else {
            Some(c.pool_str(pool, base_idx)?.to_owned())
        };
        let mut fields = Vec::with_capacity(fld_count);
        for _ in 0..fld_count {
            let fnam_idx = c.read_u32()?;
            let _type_tag_hint = c.read_u8()?;       // 1.7: tag retained as hint only
            let type_str_idx = c.read_u32()?;        // 1.7 align-zbc-reader-writer-asymmetry: authoritative
            let name = c.pool_str(pool, fnam_idx)?.to_owned();
            let type_tag = c.pool_str(pool, type_str_idx)?.to_owned();
            let attributes = read_attr_refs(&mut c, pool)?;  // 1.14 field attrs
            let visibility = c.read_u8()?;                    // 1.23 add-member-visibility
            fields.push(FieldDesc { name, type_tag, attributes, visibility });
        }
        // Generic type parameters + per-tp constraints (L3-G3a)
        let tp_count = c.read_u8()? as usize;
        let mut type_params = Vec::with_capacity(tp_count);
        let mut type_param_constraints = Vec::with_capacity(tp_count);
        for _ in 0..tp_count {
            let tp_idx = c.read_u32()?;
            type_params.push(c.pool_str(pool, tp_idx)?.to_owned());
            type_param_constraints.push(read_constraint_bundle(&mut c, pool)?);
        }
        // C3 add-attribute-reflection (zbc 1.10): per-class user attribute refs.
        let attr_count = c.read_u16()? as usize;
        let mut attributes = Vec::with_capacity(attr_count);
        for _ in 0..attr_count {
            let type_idx = c.read_u32()?;
            let factory_idx = c.read_u32()?;
            attributes.push(crate::metadata::bytecode::AttributeRef {
                type_name: c.pool_str(pool, type_idx)?.to_owned(),
                factory_func: c.pool_str(pool, factory_idx)?.to_owned(),
            });
        }
        // add-reflection-type-flags (zbc 1.12): class-shape flags byte.
        let class_flags = c.read_u8()?;
        // enforce-class-access (zbc 1.33): class declaration visibility byte
        // (0=public/1=private/2=protected/3=internal), immediately following
        // class_flags. Consumed by the compiler's cross-package `internal`-class
        // reference enforcement; complete-class-access-control surfaces it as
        // `Type.IsPublic` etc. reflection (stored into ClassDesc.visibility below).
        let class_visibility = c.read_u8()?;
        // add-reflection-static-fields (zbc 1.13): static fields block (same
        // shape as the instance fields block above).
        let static_count = c.read_u16()? as usize;
        let mut static_fields = Vec::with_capacity(static_count);
        for _ in 0..static_count {
            let snam_idx = c.read_u32()?;
            let _type_tag_hint = c.read_u8()?;
            let type_str_idx = c.read_u32()?;
            let name = c.pool_str(pool, snam_idx)?.to_owned();
            let type_tag = c.pool_str(pool, type_str_idx)?.to_owned();
            let attributes = read_attr_refs(&mut c, pool)?;  // 1.14 field attrs
            let visibility = c.read_u8()?;                    // 1.23 add-member-visibility
            static_fields.push(crate::metadata::bytecode::FieldDesc { name, type_tag, attributes, visibility });
        }
        // add-reflection-get-interfaces (zbc 1.17): per-class interface block —
        // u16 count + interface_name_idx[] u32. Surfaced by Type.GetInterfaces().
        let iface_count = c.read_u16()? as usize;
        let mut interfaces = Vec::with_capacity(iface_count);
        for _ in 0..iface_count {
            let idx = c.read_u32()?;
            interfaces.push(c.pool_str(pool, idx)?.to_owned());
        }
        // add-enum-type-metadata (zbc 1.22): trailing enum-member block, present
        // only when CLASS_FLAG_ENUM is set. member_count:u16 + (name_idx, i64)×n.
        let enum_members = if class_flags & crate::metadata::bytecode::CLASS_FLAG_ENUM != 0 {
            let em_count = c.read_u16()? as usize;
            let mut ems = Vec::with_capacity(em_count);
            for _ in 0..em_count {
                let nidx = c.read_u32()?;
                let val = c.read_i64()?;
                ems.push((c.pool_str(pool, nidx)?.to_owned(), val));
            }
            ems.into_boxed_slice()
        } else {
            Box::new([]) as Box<[(String, i64)]>
        };
        // fix-crosspkg-interface-impl (zbc 1.28): trailing interface-method block,
        // present only when CLASS_FLAG_INTERFACE is set. Layout: mcount:u16 +
        // (name_idx:u32, ret_idx:u32, pcount:u8, ptype_idx:u32×pcount)×n. The block
        // exists so the COMPILER (TsigReconcile) can restore imported interface
        // methods from dep zpkgs; the VM resolves interface calls via vtable.
        // add-interface-member-reflection: read into `iface_methods` (was discarded)
        // so `Type.GetMethods()` can surface interface method signatures.
        let iface_methods: Box<[crate::metadata::bytecode::IfaceMethodSig]> =
            if class_flags & crate::metadata::bytecode::CLASS_FLAG_INTERFACE != 0 {
                let im_count = c.read_u16()? as usize;
                let mut ims = Vec::with_capacity(im_count);
                for _ in 0..im_count {
                    let name_idx = c.read_u32()?;
                    let ret_idx = c.read_u32()?;
                    let pcount = c.read_u8()? as usize;
                    let mut param_types = Vec::with_capacity(pcount);
                    for _ in 0..pcount {
                        let ptype_idx = c.read_u32()?;
                        param_types.push(c.pool_str(pool, ptype_idx)?.to_owned());
                    }
                    ims.push(crate::metadata::bytecode::IfaceMethodSig {
                        name: c.pool_str(pool, name_idx)?.to_owned(),
                        ret_type: c.pool_str(pool, ret_idx)?.to_owned(),
                        param_types: param_types.into_boxed_slice(),
                    });
                }
                ims.into_boxed_slice()
            } else {
                Box::new([])
            };
        // add-struct-value-semantics (zbc 1.31): trailing value-struct layout
        // block, present only when CLASS_FLAG_STRUCT. Layout: size:u32 +
        // ref_count:u16 + (byte_off:u32, kind:u8)×n — the reference bitmap the
        // runtime uses to locate + clone heap refs in a value-struct blob (byte
        // size / field offsets are baked into the access instructions, not here).
        let struct_layout_desc = if class_flags & crate::metadata::bytecode::CLASS_FLAG_STRUCT != 0 {
            let size = c.read_u32()?;
            let ref_count = c.read_u16()? as usize;
            let mut ref_offsets = Vec::with_capacity(ref_count);
            let mut ref_kinds = Vec::with_capacity(ref_count);
            for _ in 0..ref_count {
                ref_offsets.push(c.read_u32()?);
                ref_kinds.push(c.read_u8()?);
            }
            Some(crate::metadata::bytecode::StructLayoutDesc {
                size,
                ref_offsets: ref_offsets.into_boxed_slice(),
                ref_kinds: ref_kinds.into_boxed_slice(),
            })
        } else {
            None
        };
        // add-struct-heap-inline (P3b, zbc 1.32): trailing inline-struct layout block,
        // present only when CLASS_FLAG_HAS_INLINE_STRUCT. Same shape as the struct block
        // (size:u32 + ref_count:u16 + (byte_off:u32, kind:u8)×n) — the class's composed
        // inline byte region size + object-relative reference bitmap. Follows the struct
        // block in the record (a type is a struct XOR a class-with-inline-fields).
        let inline_layout_desc = if class_flags & crate::metadata::bytecode::CLASS_FLAG_HAS_INLINE_STRUCT != 0 {
            let size = c.read_u32()?;
            let ref_count = c.read_u16()? as usize;
            let mut ref_offsets = Vec::with_capacity(ref_count);
            let mut ref_kinds = Vec::with_capacity(ref_count);
            for _ in 0..ref_count {
                ref_offsets.push(c.read_u32()?);
                ref_kinds.push(c.read_u8()?);
            }
            Some(crate::metadata::bytecode::StructLayoutDesc {
                size,
                ref_offsets: ref_offsets.into_boxed_slice(),
                ref_kinds: ref_kinds.into_boxed_slice(),
            })
        } else {
            None
        };
        // unify-object-byte-layout (PR-1, zbc 1.33): trailing full object field layout
        // block for normal reference classes — gated by the *derived predicate* (no
        // free class-flags bit remained): a class that is not struct / interface /
        // enum / delegate. Follows the inline block. Layout: object_size:u32 +
        // field_count:u16 + (off:u32, size:u32, kind:u8)×n + ref_count:u16 +
        // (ref_off:u32, ref_kind:u8)×m. Dormant — carried into `TypeDescCold`, not yet
        // consumed. The writer (ClassDescBuilder / ZbcWriter) emits under the same
        // predicate, so presence stays in lockstep without a flag.
        let is_object_layout_class = class_flags
            & (crate::metadata::bytecode::CLASS_FLAG_STRUCT
                | crate::metadata::bytecode::CLASS_FLAG_INTERFACE
                | crate::metadata::bytecode::CLASS_FLAG_ENUM
                | crate::metadata::bytecode::CLASS_FLAG_DELEGATE)
            == 0;
        let object_layout_desc = if is_object_layout_class {
            let size = c.read_u32()?;
            let field_count = c.read_u16()? as usize;
            let mut field_offsets = Vec::with_capacity(field_count);
            let mut field_sizes = Vec::with_capacity(field_count);
            let mut field_kinds = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                field_offsets.push(c.read_u32()?);
                field_sizes.push(c.read_u32()?);
                field_kinds.push(c.read_u8()?);
            }
            let ref_count = c.read_u16()? as usize;
            let mut ref_offsets = Vec::with_capacity(ref_count);
            let mut ref_kinds = Vec::with_capacity(ref_count);
            for _ in 0..ref_count {
                ref_offsets.push(c.read_u32()?);
                ref_kinds.push(c.read_u8()?);
            }
            Some(crate::metadata::bytecode::ObjectLayoutDesc {
                size,
                field_offsets: field_offsets.into_boxed_slice(),
                field_sizes: field_sizes.into_boxed_slice(),
                field_kinds: field_kinds.into_boxed_slice(),
                ref_offsets: ref_offsets.into_boxed_slice(),
                ref_kinds: ref_kinds.into_boxed_slice(),
            })
        } else {
            None
        };
        classes.push(ClassDesc {
            name,
            base_class,
            fields: fields.into_boxed_slice(),
            type_params: type_params.into_boxed_slice(),
            type_param_constraints: type_param_constraints.into_boxed_slice(),
            attributes: attributes.into_boxed_slice(),
            class_flags,
            visibility: class_visibility,
            static_fields: static_fields.into_boxed_slice(),
            interfaces: interfaces.into_boxed_slice(),
            enum_members,
            iface_methods,
            // add-struct-value-semantics: populated by the struct-block parse
            // below (commit 2 wire); `None` until then / for non-struct classes.
            struct_layout: struct_layout_desc,
            // add-struct-heap-inline (P3b): composed inline-struct layout (zbc 1.32).
            inline_layout: inline_layout_desc,
            // unify-object-byte-layout (PR-1): full object field layout (zbc 1.34). Dormant.
            object_layout: object_layout_desc,
        });
    }
    Ok(classes)
}

/// Decode one constraint bundle. Mirrors ZbcWriter.WriteConstraintBundle (v0.6).
/// Layout: `flags: u8, [if bit2] base_class_idx: u32, [if bit3] type_param_constraint_idx: u32,
///          interface_count: u8, iface_idx[]: u32`.
/// add-field-attribute-reflection (zbc 1.14): read a per-field attr-ref block
/// (u16 count + (type-name, factory) str-idx pairs).
pub(super) fn read_attr_refs(
    c: &mut Cursor,
    pool: &[String],
) -> Result<Box<[crate::metadata::bytecode::AttributeRef]>> {
    let count = c.read_u16()? as usize;
    let mut refs = Vec::with_capacity(count);
    for _ in 0..count {
        let type_idx = c.read_u32()?;
        let factory_idx = c.read_u32()?;
        refs.push(crate::metadata::bytecode::AttributeRef {
            type_name: c.pool_str(pool, type_idx)?.to_owned(),
            factory_func: c.pool_str(pool, factory_idx)?.to_owned(),
        });
    }
    Ok(refs.into_boxed_slice())
}

pub(super) fn read_constraint_bundle(c: &mut Cursor, pool: &[String]) -> Result<ConstraintBundle> {
    let flags = c.read_u8()?;
    let requires_class       = (flags & 0x01) != 0;
    let requires_struct      = (flags & 0x02) != 0;
    let has_base             = (flags & 0x04) != 0;
    let has_type_param       = (flags & 0x08) != 0;
    let requires_constructor = (flags & 0x10) != 0;
    let requires_enum        = (flags & 0x20) != 0;
    let has_func_sig         = (flags & 0x40) != 0; // add-generic-func-constraint (zbc 1.4+)
    let base_class = if has_base {
        let idx = c.read_u32()?;
        Some(c.pool_str(pool, idx)?.to_owned())
    } else { None };
    let type_param_constraint = if has_type_param {
        let idx = c.read_u32()?;
        Some(c.pool_str(pool, idx)?.to_owned())
    } else { None };
    let iface_count = c.read_u8()? as usize;
    let mut interfaces = Vec::with_capacity(iface_count);
    for _ in 0..iface_count {
        let idx = c.read_u32()?;
        interfaces.push(c.pool_str(pool, idx)?.to_owned());
    }
    let func_signature = if has_func_sig {
        let param_count = c.read_u8()? as usize;
        let mut params = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            let idx = c.read_u32()?;
            params.push(c.pool_str(pool, idx)?.to_owned());
        }
        let ret_idx = c.read_u32()?;
        let ret = c.pool_str(pool, ret_idx)?.to_owned();
        Some(crate::metadata::bytecode::FuncSigDescriptor { params, ret })
    } else { None };
    Ok(ConstraintBundle {
        requires_class, requires_struct, base_class, interfaces, type_param_constraint,
        requires_constructor, requires_enum,
        func_signature,
    })
}

// ── SIGS section ─────────────────────────────────────────────────────────────

pub(super) struct FuncSig {
    pub(super) name: String,
    pub(super) param_count: usize,
    pub(super) ret_type: String,
    pub(super) exec_mode: ExecMode,
    pub(super) is_static: bool,
    /// 1.23 add-member-visibility: 0=public / 1=private / 2=protected. Surfaced by
    /// MethodInfo.IsPublic / IsPrivate.
    pub(super) visibility: u8,
    /// 1.24 add-method-modifiers: bit0=virtual / bit1=abstract. Surfaced by
    /// MethodInfo.IsVirtual (authoritative) / IsAbstract.
    pub(super) method_flags: u8,
    /// 1.25 add-param-metadata: required (logical) param count → ParameterInfo.IsOptional
    /// (pos >= min_arg); params-varargs logical index (0xFF=none) → IsParams.
    pub(super) min_arg: u16,
    pub(super) params_from: u8,
    /// 1.25 add-param-metadata: per-param source name (this-slot = "this") →
    /// ParameterInfo.Name (authoritative). Length == param_count.
    pub(super) param_names: Vec<String>,
    /// 1.25 add-param-metadata: per-param default value (kind, i64-payload, str-payload)
    /// → ParameterInfo.DefaultValue. kind 0=none/1=null/2=i64/3=f64bits/4=bool/5=str.
    pub(super) param_defaults: Vec<(u8, i64, String)>,
    /// 1.3 split-debug-symbols: per-parameter type names for trace signature
    /// decoration. Length always equals `param_count` (writer pads unknowns
    /// with "?"). Empty Vec when param_count == 0.
    pub(super) param_types: Vec<String>,
    pub(super) type_params: Vec<String>,
    pub(super) type_param_constraints: Vec<ConstraintBundle>,
    /// C3b add-attribute-reflection-methods: user attributes on this function.
    pub(super) custom_attributes: Vec<crate::metadata::bytecode::AttributeRef>,
    /// add-parameter-attribute-reflection (zbc 1.15): per-parameter attributes,
    /// aligned by index with the SIGS parameter array (length == param_count,
    /// incl. the implicit `this` slot for instance methods).
    pub(super) param_attributes: Vec<Box<[crate::metadata::bytecode::AttributeRef]>>,
}

pub(super) fn read_sigs(sec: &[u8], pool: &[String], has_is_static: bool) -> Result<Vec<FuncSig>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut sigs = Vec::with_capacity(count);
    for _ in 0..count {
        let name_idx    = c.read_u32()?;
        let param_count = c.read_u16()? as usize;
        let _ret_tag_hint = c.read_u8()?;            // 1.7: tag retained as hint only
        let ret_type_idx = c.read_u32()?;            // 1.7 align-zbc-reader-writer-asymmetry: authoritative
        let mode_byte   = c.read_u8()?;
        let is_static   = if has_is_static { c.read_u8()? != 0 } else { false };
        let visibility  = if has_is_static { c.read_u8()? } else { 0 };  // 1.23 add-member-visibility (after is_static)
        let method_flags = if has_is_static { c.read_u8()? } else { 0 }; // 1.24 add-method-modifiers (after visibility)
        let min_arg     = if has_is_static { c.read_u16()? } else { param_count as u16 }; // 1.25 add-param-metadata
        let params_from = if has_is_static { c.read_u8()? } else { 0xFF };

        // 1.3 split-debug-symbols: per-param type names (u32 strIdx × param_count).
        // 1.25 add-param-metadata: interleaved per-param name_str_idx + default_kind + payload.
        let mut param_types = Vec::with_capacity(param_count);
        let mut param_names = Vec::with_capacity(param_count);
        let mut param_defaults = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            let pt_idx = c.read_u32()?;
            param_types.push(c.pool_str(pool, pt_idx)?.to_owned());
            let name_idx = c.read_u32()?;                       // 1.25 name_str_idx
            param_names.push(c.pool_str(pool, name_idx)?.to_owned());
            let dk = c.read_u8()?;                              // 1.25 default_kind
            let (ival, sval) = match dk {
                2 | 3 => (c.read_i64()?, String::new()),
                4 => (c.read_u8()? as i64, String::new()),
                5 => { let si = c.read_u32()?; (0, c.pool_str(pool, si)?.to_owned()) }
                _ => (0, String::new()),
            };
            param_defaults.push((dk, ival, sval));
        }

        // Generic type params (added after is_static) + per-tp constraints (L3-G3a)
        let tp_count    = if has_is_static { c.read_u8()? as usize } else { 0 };
        let mut type_params = Vec::with_capacity(tp_count);
        let mut type_param_constraints = Vec::with_capacity(tp_count);
        for _ in 0..tp_count {
            let tp_idx = c.read_u32()?;
            type_params.push(c.pool_str(pool, tp_idx)?.to_owned());
            type_param_constraints.push(read_constraint_bundle(&mut c, pool)?);
        }
        // C3b add-attribute-reflection-methods (zbc 1.11): per-function attr refs.
        let attr_count = if has_is_static { c.read_u16()? as usize } else { 0 };
        let mut custom_attributes = Vec::with_capacity(attr_count);
        for _ in 0..attr_count {
            let type_idx = c.read_u32()?;
            let factory_idx = c.read_u32()?;
            custom_attributes.push(crate::metadata::bytecode::AttributeRef {
                type_name: c.pool_str(pool, type_idx)?.to_owned(),
                factory_func: c.pool_str(pool, factory_idx)?.to_owned(),
            });
        }
        // add-parameter-attribute-reflection (zbc 1.15): per-parameter attr block —
        // exactly param_count attr-ref blocks (each u16 count + (type, factory) pairs).
        let mut param_attributes = Vec::with_capacity(param_count);
        if has_is_static {
            for _ in 0..param_count {
                param_attributes.push(read_attr_refs(&mut c, pool)?);
            }
        }
        sigs.push(FuncSig {
            name: c.pool_str(pool, name_idx)?.to_owned(),
            param_count,
            ret_type: c.pool_str(pool, ret_type_idx)?.to_owned(),
            exec_mode: exec_mode_from_byte(mode_byte),
            is_static,
            visibility,
            method_flags,
            min_arg,
            params_from,
            param_names,
            param_defaults,
            param_types,
            type_params,
            type_param_constraints,
            custom_attributes,
            param_attributes,
        });
    }
    Ok(sigs)
}
