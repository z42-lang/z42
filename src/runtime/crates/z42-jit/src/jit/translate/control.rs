//! Control-flow leaf emitters: exception-table scan + entry safepoint check.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// Find every exception_table entry whose try region covers `block_idx`,
/// in source order. catch-by-generic-type  requires the JIT to
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

/// runtime-audit P0-4: stack overflow is fatal. Compare the stack pointer with
/// the context's `JitHead::stack_limit` — the limit of the thread driving it,
/// stored at its bottom frame push; below it, record the overflow and return the
/// "thrown" code. One load and a compare on the hot path.
pub(super) fn emit_stack_check(
    builder:   &mut FunctionBuilder,
    ptr:       cranelift_codegen::ir::Type,
    ctx_val:   cranelift_codegen::ir::Value,
    frame_val: cranelift_codegen::ir::Value,
    hr_overflow: cranelift_codegen::ir::FuncRef,
) {
    let flags = MemFlagsData::trusted();
    let sp = builder.ins().get_stack_pointer(ptr);
    let limit = builder.ins().load(
        ptr, flags, ctx_val,
        crate::components::bridge::VM_CONTEXT_STACK_LIMIT_OFFSET as i32,
    );
    let over = builder.ins().icmp(IntCC::UnsignedLessThan, sp, limit);
    let ok_blk = builder.create_block();
    let over_blk = builder.create_block();
    builder.ins().brif(over, over_blk, &[], ok_blk, &[]);
    builder.switch_to_block(over_blk);
    builder.ins().call(hr_overflow, &[frame_val, ctx_val]);
    let one = builder.ins().iconst(types::I8, 1);
    builder.ins().return_(&[one]);
    builder.switch_to_block(ok_blk);
}

/// Inline the mutator's countdown and call the VM slow path when it expires.
/// A concurrent collector poke can race the load/store, as in the interpreter.
/// Emission continues in the fast block, which the caller seals later.
pub(super) fn emit_safepoint_check(
    builder:   &mut FunctionBuilder,
    ctx_val:   cranelift_codegen::ir::Value,
    frame_val: cranelift_codegen::ir::Value,
    hr_slow:   cranelift_codegen::ir::FuncRef,
) {
    let flags = MemFlagsData::trusted();
    let skip_off = crate::components::bridge::VM_CONTEXT_SAFEPOINT_SKIP_OFFSET as i32;
    let prev = builder.ins().load(types::I32, flags, ctx_val, skip_off);
    let newv = builder.ins().iadd_imm_s(prev, -1);
    builder.ins().store(flags, newv, ctx_val, skip_off);
    // prev u> 1  ⇒  still throttled, take the fast (skip) path.
    let cond = builder.ins().icmp_imm_s(IntCC::UnsignedGreaterThan, prev, 1);
    let fast_blk = builder.create_block();
    let slow_blk = builder.create_block();
    builder.ins().brif(cond, fast_blk, &[], slow_blk, &[]);
    builder.switch_to_block(slow_blk);
    builder.ins().call(hr_slow, &[frame_val, ctx_val]);
    builder.ins().jump(fast_blk, &[]);
    builder.switch_to_block(fast_blk);
}
