//! Control-flow leaf emitters: exception-table scan + entry safepoint check.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// Find every exception_table entry whose try region covers `block_idx`,
/// in source order. catch-by-generic-type (2026-05-06) requires the JIT to
/// see all covering entries (not just the first) so it can emit a typed-catch
/// chain that probes each candidate's `catch_type` against the thrown value's
/// class and jumps to the first matching handler.
pub(super) fn find_handler_entries(func: &Function, block_idx: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, entry) in func.exception_table().iter().enumerate() {
        let Some(start) = func.blocks.iter().position(|b| b.label == entry.try_start) else { continue };
        let Some(end)   = func.blocks.iter().position(|b| b.label == entry.try_end)   else { continue };
        if block_idx >= start && block_idx < end {
            out.push(i);
        }
    }
    out
}

/// inline-jit-safepoint-check (2026-08-01): emit the cooperative-GC safepoint
/// **fast path** inline as native load/store + branch, replacing a
/// `jit_check_safepoint` helper call (~10ns) on the hot path. See
/// `docs/spec/changes/inline-jit-safepoint-check/design.md`.
///
/// Mirrors `gc::safepoint::check_safepoint`:
/// ```text
///   vm_ctx = *(ctx + JIT_MODULE_CTX_VM_CTX_OFFSET)
///   prev   = *(vm_ctx + SAFEPOINT_SKIP_OFFSET)      // plain i32 load
///            *(vm_ctx + SAFEPOINT_SKIP_OFFSET) = prev - 1
///   if prev u> 1 { fast: continue }                 // ~99.9%
///   else         { slow: jit_check_safepoint_slow(frame, ctx); continue }
/// ```
/// The decrement is a plain (non-atomic) load/store: `safepoint_skip` is
/// single-writer per mutator (only `force_safepoint`, test-only, writes it
/// cross-thread), so RMW atomicity is unnecessary — and dropping it is what
/// makes the fast path inlinable as two bare `mov`s (the `atomic_rmw` form
/// panicked on x86_64 Cranelift lowering; the load/store form does not).
///
/// The current block ends with a `brif`; emission continues in the created
/// `fast` block, which the caller keeps building into. Blocks are sealed later
/// by `seal_all_blocks()` (per this file's convention).
pub(super) fn emit_safepoint_check(
    builder:   &mut FunctionBuilder,
    ptr:       cranelift_codegen::ir::Type,
    ctx_val:   cranelift_codegen::ir::Value,
    frame_val: cranelift_codegen::ir::Value,
    hr_slow:   cranelift_codegen::ir::FuncRef,
) {
    let flags = MemFlags::trusted();
    // vm_ctx pointer lives inside JitModuleCtx.
    let vm_ctx = builder.ins().load(
        ptr, flags, ctx_val,
        crate::jit::frame::JIT_MODULE_CTX_VM_CTX_OFFSET as i32,
    );
    let skip_off = crate::vm_context::VM_CONTEXT_SAFEPOINT_SKIP_OFFSET as i32;
    let prev = builder.ins().load(types::I32, flags, vm_ctx, skip_off);
    let newv = builder.ins().iadd_imm(prev, -1);
    builder.ins().store(flags, newv, vm_ctx, skip_off);
    // prev u> 1  ⇒  still throttled, take the fast (skip) path.
    let cond = builder.ins().icmp_imm(IntCC::UnsignedGreaterThan, prev, 1);
    let fast_blk = builder.create_block();
    let slow_blk = builder.create_block();
    builder.ins().brif(cond, fast_blk, &[], slow_blk, &[]);
    builder.switch_to_block(slow_blk);
    builder.ins().call(hr_slow, &[frame_val, ctx_val]);
    builder.ins().jump(fast_blk, &[]);
    builder.switch_to_block(fast_blk);
}
