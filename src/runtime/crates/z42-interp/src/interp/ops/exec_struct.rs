//! Interpreter register adapters for the shared struct value operations.
use super::Frame;
use crate::context::VmContext;
use anyhow::Result;
use crate::exec::ops::structs::{struct_alloc_val, struct_copy_val, resolve_for_access};
use crate::objops::struct_leaf::{struct_field_get_val, struct_field_set_val};
#[cfg(all(test, z42_test_facade))]
use crate::exec::ops::structs::*;
#[cfg(all(test, z42_test_facade))]
use crate::object::{decode_prim, encode_prim, prim_width};
#[cfg(all(test, z42_test_facade))]
use crate::object as ty;

#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn struct_alloc(
    ctx: &VmContext, frame: &mut Frame, dst: u32, type_name: &str, size: u32,
) -> Result<()> {
    let v = struct_alloc_val(ctx, frame.frame_id(ctx), type_name, size);
    frame.set(dst, v);
    Ok(())
}


#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn struct_copy(
    ctx: &VmContext, frame: &mut Frame, dst: u32, src: u32, size: u32,
) -> Result<()> {
    let dst_val = frame.get(dst)?.clone();
    let src_val = frame.get(src)?.clone();
    struct_copy_val(ctx, &dst_val, &src_val, size)
}


#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn struct_field_get_prim(
    ctx: &VmContext, frame: &mut Frame,
    insn: &crate::zpkg::ir::StructFieldGetInsn,
) -> Result<()> {
    let base_val = frame.get(insn.base)?.clone();
    let byte_off = resolve_for_access(ctx, &insn.root_type, &insn.path, &base_val, "StructFieldGetPrim")?;
    let val = struct_field_get_val(ctx, &base_val, byte_off, insn.kind)?;
    frame.set(insn.dst, val);
    Ok(())
}


#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(super) fn struct_field_set_prim(
    ctx: &VmContext, frame: &mut Frame,
    insn: &crate::zpkg::ir::StructFieldSetInsn,
) -> Result<()> {
    let base_val = frame.get(insn.base)?.clone();
    let byte_off = resolve_for_access(ctx, &insn.root_type, &insn.path, &base_val, "StructFieldSetPrim")?;
    let v = frame.get(insn.val)?.clone();
    struct_field_set_val(ctx, &base_val, byte_off, insn.kind, &v)
}


#[cfg(all(test, z42_test_facade))]
#[path = "exec_struct_tests.rs"]
mod exec_struct_tests;
