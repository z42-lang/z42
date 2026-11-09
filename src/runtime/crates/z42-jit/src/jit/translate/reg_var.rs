//! Promoted-register (2C) read/store: Variable / 2B cache / memory resolution.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// jit-unbox-regalloc Phase 2C: read an integer reg's i64 payload — from its
/// resident Cranelift `Variable` (via `use_var`, Cranelift inserts the SSA
/// phis) if promoted, else via the 2B block-local cache (which loads from
/// `frame.regs` on a miss).
#[inline]
pub(super) fn load_int(
    builder: &mut FunctionBuilder, cache: &mut RegCache, promoted: &[bool],
    regs_base: cranelift_codegen::ir::Value, reg: u32,
) -> cranelift_codegen::ir::Value {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.use_var(Variable::from_u32(reg))
    } else {
        cache.load_i64(builder, regs_base, reg)
    }
}

/// Write an integer reg's i64 payload — to its resident `Variable` (`def_var`)
/// if promoted, else to the 2B cache (deferred spill). No `frame.regs` store
/// either way until a flush (cache) or the `Ret` spill (Variable).
#[inline]
pub(super) fn store_int(
    builder: &mut FunctionBuilder, cache: &mut RegCache, promoted: &[bool],
    reg: u32, val: cranelift_codegen::ir::Value,
) {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.def_var(Variable::from_u32(reg), val);
    } else {
        cache.store_i64(reg, val);
    }
}

/// jit-unbox-regalloc Phase 2C (F64 residency): read an F64 reg's payload — from
/// its resident Cranelift `Variable` (declared F64-typed) if promoted, else via a
/// direct `frame.regs` memory load. F64 regs have NO block-local cache (unlike
/// the 2B integer cache — floats never enter `RegCache`); they are either
/// resident Variables or memory-backed, so no cache/flush interaction exists.
#[inline]
pub(super) fn load_f64(
    builder: &mut FunctionBuilder, promoted: &[bool],
    regs_base: cranelift_codegen::ir::Value, reg: u32,
) -> cranelift_codegen::ir::Value {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.use_var(Variable::from_u32(reg))
    } else {
        let addr = reg_addr(builder, regs_base, reg);
        load_payload(builder, addr, types::F64)
    }
}

/// Write an F64 reg's payload — to its resident `Variable` (`def_var`) if
/// promoted, else straight to `frame.regs[reg]` with the `TAG_F64` discriminant.
#[inline]
pub(super) fn store_f64(
    builder: &mut FunctionBuilder, promoted: &[bool],
    regs_base: cranelift_codegen::ir::Value, reg: u32, val: cranelift_codegen::ir::Value,
) {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.def_var(Variable::from_u32(reg), val);
    } else {
        let addr = reg_addr(builder, regs_base, reg);
        store_const_tag(builder, addr, TAG_F64, val);
    }
}
