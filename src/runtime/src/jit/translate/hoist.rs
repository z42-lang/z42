//! Loop-invariant hoists computed once in the entry block: array data
//! ptr/len/width, primitive field bytes-ptr/offset, reference field
//! bytes-ptr/offset/tag. Split out of `translate/mod.rs` (H2).

use super::*;
use cranelift_codegen::ir::{FuncRef, Value};
use std::collections::HashMap;

pub(super) fn compute_hoists(
    builder: &mut FunctionBuilder,
    func: &Function,
    ptr: cranelift_codegen::ir::Type,
    frame_val: Value,
    ctx_val: Value,
    hr_array_data_opt: FuncRef,
    hr_obj_field_slot: FuncRef,
    hr_obj_ref_field_slot: FuncRef,
) -> (
    HashMap<u32, (Value, Value, Value)>,
    HashMap<(u32, String), (Value, Value)>,
    HashMap<(u32, String), (Value, Value, Value)>,
) {
    let written: std::collections::HashSet<u32> = {
        let mut w = std::collections::HashSet::new();
        for b in &func.blocks {
            for ins in &b.instructions {
                if let Some(d) = ins.written_reg() { w.insert(d); }
            }
        }
        w
    };
    // hoisted (ptr, len, width): width is the runtime packed slot width the
    // ArraySet inline consults (jit-inline-i32-arrays). ArrayGet ignores it.
    let hoisted_arrays: std::collections::HashMap<u32, (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> = {
        let mut candidates: Vec<u32> = Vec::new();
        let consider = |arr: &u32, ok: bool, candidates: &mut Vec<u32>| {
            if ok && !written.contains(arr) && !candidates.contains(arr) {
                candidates.push(*arr);
            }
        };
        for b in &func.blocks {
            for ins in &b.instructions {
                match ins {
                    Instruction::ArrayGet { dst, arr, idx } => consider(arr,
                        arr_prim_elem(func, *dst).is_some() && idx_int_ok(func, *idx),
                        &mut candidates),
                    // i32/i64/f64 ArraySet also reads the loop-invariant data ptr/len/width.
                    Instruction::ArraySet { arr, idx, val } => consider(arr,
                        arr_prim_elem(func, *val).is_some() && idx_int_ok(func, *idx),
                        &mut candidates),
                    _ => {}
                }
            }
        }
        candidates.sort_unstable(); // deterministic codegen order
        let mut map = std::collections::HashMap::new();
        for arr in candidates {
            use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
            let ss_ptr = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_len = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_width = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
            let len_addr = builder.ins().stack_addr(ptr, ss_len, 0);
            let width_addr = builder.ins().stack_addr(ptr, ss_width, 0);
            let a_c = builder.ins().iconst(types::I32, arr as i64);
            builder.ins().call(hr_array_data_opt, &[frame_val, ctx_val, a_c, ptr_addr, len_addr, width_addr]);
            let dptr = builder.ins().stack_load(ptr, ss_ptr, 0);
            let dlen = builder.ins().stack_load(types::I64, ss_len, 0);
            let dwidth = builder.ins().stack_load(types::I64, ss_width, 0);
            map.insert(arr, (dptr, dlen, dwidth));
        }
        map
    };

    // ── FieldGet/Set P5-B: hoist (bytes_ptr, byte_offset) for never-reassigned objects ──
    // For an object register never written (e.g. `this`) accessed via `FieldGet`/
    // `FieldSet` on an inline-primitive field, resolve (bytes_ptr, offset) ONCE in the
    // entry block via the non-throwing byte-aware `jit_obj_field_slot`. Keyed by
    // (obj_reg, field_name); the expected (width, tag) come from the field's static
    // type. The per-access inline then does a native width-aware byte load/store at
    // `bytes_ptr + offset`; `offset < 0` (null / non-object / field-not-found /
    // reference / struct root / string / layout mismatch) routes to the helper.
    let hoisted_fields: std::collections::HashMap<(u32, String), (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> = {
        let mut cands: Vec<(u32, &str, u32, u8)> = Vec::new();
        for b in &func.blocks {
            for ins in &b.instructions {
                // FieldGet (dst) / FieldSet (val) on a never-reassigned object read the
                // loop-invariant bytes ptr + offset. width/tag come from the field type.
                let hit = match ins {
                    Instruction::FieldGet(insn) => field_prim_kind(func, insn.dst)
                        .map(|k| (insn.obj, insn.field_name.as_str(), k.width, k.field_tag)),
                    Instruction::FieldSet(insn) => field_prim_kind(func, insn.val)
                        .map(|k| (insn.obj, insn.field_name.as_str(), k.width, k.field_tag)),
                    _ => None,
                };
                if let Some((obj, fname, w, tag)) = hit {
                    if !written.contains(&obj) && !cands.iter().any(|(o, f, _, _)| *o == obj && *f == fname) {
                        cands.push((obj, fname, w, tag));
                    }
                }
            }
        }
        cands.sort_unstable();
        let mut map = std::collections::HashMap::new();
        for (obj, fname, exp_w, exp_tag) in cands {
            use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
            let ss_ptr = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_off = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
            let off_addr = builder.ins().stack_addr(ptr, ss_off, 0);
            let o_c = builder.ins().iconst(types::I32, obj as i64);
            let fp = builder.ins().iconst(ptr, fname.as_ptr() as i64);
            let fl = builder.ins().iconst(types::I64, fname.len() as i64);
            let w_c = builder.ins().iconst(types::I32, exp_w as i64);
            let tag_c = builder.ins().iconst(types::I32, exp_tag as i64);
            builder.ins().call(hr_obj_field_slot,
                &[frame_val, ctx_val, o_c, fp, fl, w_c, tag_c, ptr_addr, off_addr]);
            let bptr = builder.ins().stack_load(ptr, ss_ptr, 0);
            let off = builder.ins().stack_load(types::I64, ss_off, 0);
            map.insert((obj, fname.to_string()), (bptr, off));
        }
        map
    };

    // ── FieldGet T1-B: hoist (bytes_ptr, byte_offset, tag) for byte-inlined ──────
    // ── reference (class-instance / array) fields of never-reassigned objects ────
    // Twin of the P5-B primitive hoist above: for a `FieldGet` whose `dst` is a heap
    // reference (`IrType::Ref`) on a never-reassigned object, resolve
    // (bytes_ptr, offset, tag) ONCE via the non-throwing `jit_obj_ref_field_slot`.
    // `tag` is the `Value` discriminant to stamp on a non-null load (7=Object/6=Array,
    // hoisted since `IrType::Ref` does not distinguish the two). The per-access inline
    // then does a native 8B tagged-pointer load + `raw==0 ? Null : tagged store`;
    // `offset < 0` (non-object receiver / null / field-not-found / side-table ref =
    // closure·func·string / struct root) routes to `jit_field_get`. There is no
    // FieldSet twin — a reference store needs the GC write barrier, so it stays on the
    // helper. A field is primitive XOR reference, so this never overlaps `hoisted_fields`.
    let hoisted_ref_fields: std::collections::HashMap<(u32, String), (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> = {
        let mut cands: Vec<(u32, &str)> = Vec::new();
        for b in &func.blocks {
            for ins in &b.instructions {
                if let Instruction::FieldGet(insn) = ins {
                    // Only inline a reference read whose static dst type is `Ref` (any
                    // heap object — object/array/list/dict/null). This gates OUT
                    // `IrType::Str` (side-table string GcRef → helper) and primitives
                    // (handled by the P5-B path), and guarantees the dst's prior value
                    // is Drop-free {Object,Array,Null,StackObject,StackArray} so the
                    // native `store_tagged` may overwrite it without a drop.
                    if field_prim_kind(func, insn.dst).is_none()
                        && is_typed(func, insn.dst, IrType::Ref)
                        && !written.contains(&insn.obj)
                        && !cands.iter().any(|(o, f)| *o == insn.obj && *f == insn.field_name.as_str())
                    {
                        cands.push((insn.obj, insn.field_name.as_str()));
                    }
                }
            }
        }
        cands.sort_unstable();
        let mut map = std::collections::HashMap::new();
        for (obj, fname) in cands {
            use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
            let ss_ptr = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_off = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_tag = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
            let off_addr = builder.ins().stack_addr(ptr, ss_off, 0);
            let tag_addr = builder.ins().stack_addr(ptr, ss_tag, 0);
            let o_c = builder.ins().iconst(types::I32, obj as i64);
            let fp = builder.ins().iconst(ptr, fname.as_ptr() as i64);
            let fl = builder.ins().iconst(types::I64, fname.len() as i64);
            builder.ins().call(hr_obj_ref_field_slot,
                &[frame_val, ctx_val, o_c, fp, fl, ptr_addr, off_addr, tag_addr]);
            let bptr = builder.ins().stack_load(ptr, ss_ptr, 0);
            let off = builder.ins().stack_load(types::I64, ss_off, 0);
            let tag = builder.ins().stack_load(types::I32, ss_tag, 0);
            map.insert((obj, fname.to_string()), (bptr, off, tag));
        }
        map
    };
    (hoisted_arrays, hoisted_fields, hoisted_ref_fields)
}
