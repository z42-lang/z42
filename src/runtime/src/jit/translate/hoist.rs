//! Loop-invariant hoists computed once in the entry block: array data
//! ptr/len/width and field cells (`field_ic`'s guard, evaluated once). Split out of
//! `translate/mod.rs` (H2).

use super::*;
use cranelift_codegen::ir::{FuncRef, Value};
use std::collections::HashMap;

pub(super) fn compute_hoists(
    builder: &mut FunctionBuilder,
    func: &Function,
    ptr: cranelift_codegen::ir::Type,
    frame_val: Value,
    ctx_val: Value,
    regs_base: Value,
    hr_array_data_opt: FuncRef,
    hr_array_ref_data: FuncRef,
    ref_stores_inline: bool,
) -> (
    HashMap<u32, (Value, Value, Value)>,
    super::field_ic::HoistedFields,
    HashMap<u32, (Value, Value, Value)>,
) {
    // Registers anything in the function writes: an instruction's destination, or a catch
    // handler's exception register.
    let written: std::collections::HashSet<u32> = {
        let mut w = std::collections::HashSet::new();
        for b in &func.blocks {
            for ins in &b.instructions {
                if let Some(d) = ins.written_reg() { w.insert(d); }
            }
        }
        w.extend(func.exception_table().iter().map(|e| e.catch_reg));
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
            let dptr = builder.ins().stack_load(ptr, ptr, ss_ptr, 0);
            let dlen = builder.ins().stack_load(ptr, types::I64, ss_len, 0);
            let dwidth = builder.ins().stack_load(ptr, types::I64, ss_width, 0);
            map.insert(arr, (dptr, dlen, dwidth));
        }
        map
    };

    let hoisted_fields = hoist_fields(builder, func, ptr, regs_base, &written, ref_stores_inline);
    // ── R3: hoist (data_ptr, len, stride) for REF / MIXED arrays read into a whole-`Value`
    //    register (`arr_ref_elem`) — the twin of `hoisted_arrays` for reference reads. ──
    let hoisted_ref_arrays: HashMap<u32, (Value, Value, Value)> = {
        let mut cands: Vec<u32> = Vec::new();
        for b in &func.blocks {
            for ins in &b.instructions {
                if let Instruction::ArrayGet { dst, arr, idx } = ins {
                    if arr_prim_elem(func, *dst).is_none() && arr_ref_elem(func, *dst)
                        && idx_int_ok(func, *idx) && !written.contains(arr) && !cands.contains(arr)
                    {
                        cands.push(*arr);
                    }
                }
            }
        }
        cands.sort_unstable();
        let mut map = HashMap::new();
        for arr in cands {
            map.insert(arr, ref_array_data(builder, ptr, frame_val, ctx_val, hr_array_ref_data, arr));
        }
        map
    };
    (hoisted_arrays, hoisted_fields, hoisted_ref_arrays)
}

/// Blocks inside a loop: every block between a backward branch's target and its source (block
/// order — the compiler lays a loop body out contiguously). A misjudged block only costs or saves
/// one hoist; the hoist itself is valid anywhere in the function.
pub(super) fn loop_blocks(func: &Function) -> Vec<bool> {
    let mut in_loop = vec![false; func.blocks.len()];
    let idx = |l: &String| func.block_index.get(l).copied()
        .or_else(|| func.blocks.iter().position(|b| &b.label == l));
    for (s, b) in func.blocks.iter().enumerate() {
        let targets: Vec<usize> = match &b.terminator {
            Terminator::Br { label } => idx(label).into_iter().collect(),
            Terminator::BrCond { true_label, false_label, .. } =>
                idx(true_label).into_iter().chain(idx(false_label)).collect(),
            _ => Vec::new(),
        };
        for t in targets.into_iter().filter(|&t| t <= s) {
            in_loop[t..=s].iter_mut().for_each(|x| *x = true);
        }
    }
    in_loop
}

/// P1-4: for each (receiver, field, cell kind) accessed inside a loop through a receiver register
/// nothing writes (`written`), run the site's field-IC guard once here, in the entry block: its
/// `(hdr, off)` (`off < 0` = entry 0 did not match yet) dominates every access, which then skips
/// its own guard (`field_ic::enter_cell`). Inline only — no helper call, no name lookup — so a
/// function whose IC is still cold pays a few loads and the per-access guard. Valid for the whole
/// activation: the register keeps the receiver alive and blocks never move.
fn hoist_fields(
    builder: &mut FunctionBuilder, func: &Function, ptr: cranelift_codegen::ir::Type, regs_base: Value,
    written: &std::collections::HashSet<u32>, ref_stores_inline: bool,
) -> super::field_ic::HoistedFields {
    use super::field_ic::{emit_guard, InlineCell};
    let mut map = super::field_ic::HoistedFields::new();
    if !super::field_ic::FIELD_IC_SUPPORTED { return map; }
    let in_loop = loop_blocks(func);
    // (receiver, field, kind) → the first in-loop site's IC.
    let mut cands: Vec<(u32, &str, u8, *const crate::vm::runtime::dispatch::ic::FieldIC)> = Vec::new();
    for (bi, b) in func.blocks.iter().enumerate() {
        if !in_loop[bi] { continue; }
        for (ii, ins) in b.instructions.iter().enumerate() {
            let (obj, name, cell) = match ins {
                Instruction::FieldGet(i) => (i.obj, i.field_name.as_str(), InlineCell::of_reg(func, i.dst)),
                Instruction::FieldSet(i) => (i.obj, i.field_name.as_str(), InlineCell::of_reg(func, i.val)
                    .filter(|c| matches!(c, InlineCell::Prim(_)) || ref_stores_inline)),
                _ => continue,
            };
            let Some(cell) = cell else { continue };
            let ic = field_ic_ptr_at(func, bi, ii);
            let kind = cell.ic_kind();
            if ic.is_null() || written.contains(&obj)
                || cands.iter().any(|&(o, f, k, _)| o == obj && f == name && k == kind)
            {
                continue;
            }
            cands.push((obj, name, kind, ic));
        }
    }
    cands.sort_unstable_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2))); // deterministic codegen
    for (obj, name, kind, ic) in cands {
        let join = builder.create_block();
        builder.append_block_param(join, types::I64);
        builder.append_block_param(join, types::I64);
        let miss = builder.create_block();
        emit_guard(builder, ptr, regs_base, obj, ic, kind, join, miss);
        builder.switch_to_block(miss);
        let zero = builder.ins().iconst(types::I64, 0);
        let none = builder.ins().iconst(types::I64, -1);
        builder.ins().jump(join, &[
            cranelift_codegen::ir::BlockArg::Value(zero), cranelift_codegen::ir::BlockArg::Value(none),
        ]);
        builder.switch_to_block(join);
        let hdr = builder.block_params(join)[0];
        let off = builder.block_params(join)[1];
        map.insert((obj, name.to_string(), kind), (hdr, off));
    }
    map
}

/// Emit a `jit_array_ref_data_opt(arr)` call and load its `(data_ptr, len, stride)` results.
pub(super) fn ref_array_data(
    builder: &mut FunctionBuilder, ptr: cranelift_codegen::ir::Type, frame_val: Value, ctx_val: Value,
    hr_array_ref_data: FuncRef, arr: u32,
) -> (Value, Value, Value) {
    use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
    let slot = |b: &mut FunctionBuilder| b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
    let (ss_ptr, ss_len, ss_stride) = (slot(builder), slot(builder), slot(builder));
    let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
    let len_addr = builder.ins().stack_addr(ptr, ss_len, 0);
    let stride_addr = builder.ins().stack_addr(ptr, ss_stride, 0);
    let a_c = builder.ins().iconst(types::I32, arr as i64);
    builder.ins().call(hr_array_ref_data, &[frame_val, ctx_val, a_c, ptr_addr, len_addr, stride_addr]);
    (
        builder.ins().stack_load(ptr, ptr, ss_ptr, 0),
        builder.ins().stack_load(ptr, types::I64, ss_len, 0),
        builder.ins().stack_load(ptr, types::I64, ss_stride, 0),
    )
}
