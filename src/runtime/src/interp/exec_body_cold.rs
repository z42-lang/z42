//! The slow paths of the execution loop (`exec_function_body`), kept out of line so
//! they do not take registers / stack from the hot dispatch loop: `ref` / `out`
//! parameter copy-in, exception unwinding to a local handler, internal-error
//! location, and the label-map branch fallback. See the 「热循环布局」 note in
//! `exec_instr.rs`.

use crate::vm::runtime::{Function, Module};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{Context, Result};
use std::collections::HashMap;

use super::{
    deref_ref, find_handler, fire_exception_caught, fire_exception_thrown, resolve_line,
    run_ref_writebacks, Frame,
};

/// Where a thrown value goes from this frame.
pub(super) enum Unwind {
    /// A local handler catches it: continue at this block (the catch register is set).
    Catch(usize),
    /// No local handler: return it as `ExecOutcome::Thrown` (ref writebacks already run).
    Propagate(Value),
}

/// Spec impl-ref-out-in-runtime (Decision R2 architecture E): entry copy-in of one
/// `ref` / `out` parameter. The register holds a `Value::Ref` transient-arena handle
/// (created in the caller's frame, still live here): resolve its `RefKind`, replace
/// the register with the underlying value — so callee handlers never see a `Ref` —
/// and remember the kind in `frame.ref_writebacks` for the exit copy-out. Must run
/// before the callee frame is pushed: `Ref::Stack { frame_idx }` names the caller's
/// stack position, which `ctx.frame_state_at` resolves against the current stack.
#[cold]
#[inline(never)]
pub(super) fn copy_in_ref_param(
    ctx: &VmContext, frame: &mut Frame, reg: usize, idx: u32, frame_id: u32,
) -> Result<()> {
    let kind = ctx.transient_arena.lock().ref_kind(idx, frame_id)?;
    frame.regs[reg] = deref_ref(&kind, ctx)?;
    frame.ref_writebacks.push((reg as u32, kind));
    Ok(())
}

/// A callee threw `val` (value-based propagation). Callees may throw via the JIT's
/// `set_exception` path, which doesn't run `Terminator::Throw`, so the stack trace is
/// populated here too (idempotent: an already-populated object is skipped). A local
/// catch is an unwind of exactly 1 frame (the callee); the throw itself was counted /
/// emitted at its origin.
#[cold]
#[inline(never)]
pub(super) fn callee_threw(
    ctx: &VmContext, module: &Module, func: &Function, frame: &mut Frame, block_idx: usize, val: Value,
) -> Result<Unwind> {
    crate::exception::populate_stack_trace(&val, ctx, module);
    catch_or_propagate(ctx, module, func, frame, block_idx, val, 1)
}

/// `Terminator::Throw`. Stamps the throwing frame's site first, so the snapshot's top
/// entry shows the throw site (not whatever the previous `Call` left) — end of block,
/// since a terminator has no intra-block offset — and counts / emits the throw before
/// the handler lookup, so even an immediately caught exception shows in the event
/// stream. A local catch unwinds 0 frames.
#[cold]
#[inline(never)]
pub(super) fn throw_here(
    ctx: &VmContext, module: &Module, func: &Function, frame: &mut Frame, block_idx: usize, val: Value,
) -> Result<Unwind> {
    let end = func.blocks.get(block_idx).map_or(0, |b| b.instructions.len());
    ctx.set_top_frame_pc(func.linear_offset(block_idx as u32, end as u32));
    crate::exception::populate_stack_trace(&val, ctx, module);
    fire_exception_thrown(ctx, module, &val);
    catch_or_propagate(ctx, module, func, frame, block_idx, val, 0)
}

/// Offer `val` to this frame's handlers covering `block_idx`. No handler → run the
/// ref writebacks (spec impl-ref-out-in-runtime: mutations the callee made to `ref` /
/// `out` params before the throw stay visible to the caller, as in C#) and propagate.
fn catch_or_propagate(
    ctx: &VmContext, module: &Module, func: &Function, frame: &mut Frame, block_idx: usize, val: Value,
    frames_unwound: u32,
) -> Result<Unwind> {
    let block_map = &func.block_index;
    let Some(entry_idx) = find_handler(
        ctx, func, block_idx, block_map, &module.type_registry, &val,
        frame.type_param_env(),
    ) else {
        run_ref_writebacks(frame, ctx)?;
        return Ok(Unwind::Propagate(val));
    };
    let entry = &func.exception_table()[entry_idx];
    fire_exception_caught(ctx, module, &val, frames_unwound);
    frame.set(entry.catch_reg, val);
    Ok(Unwind::Catch(label_target(block_map, &entry.catch_label)?))
}

/// An internal VM error from an instruction, with its source location. The cause is
/// inlined into the message ahead of the location (fix-vm-error-display-loses-cause):
/// `e.context(..)` would make the location the topmost message, and `Display` (`{e}`)
/// shows only that.
#[cold]
#[inline(never)]
pub(super) fn locate_error(e: anyhow::Error, func: &Function, block_idx: usize, instr_idx: usize) -> anyhow::Error {
    let (line, col) = resolve_line(func.line_table(), block_idx as u32, instr_idx as u32);
    let loc_str = if col > 0 { format!("line {line}, col {col}") } else { format!("line {line}") };
    anyhow::anyhow!("{}\n  at {} ({})", e, func.name, loc_str)
}

/// A branch target by label — the fallback when `Function::branch_targets` was not
/// precomputed (hand-built test functions), and for catch labels.
#[cold]
#[inline(never)]
pub(super) fn label_target(block_map: &HashMap<String, usize>, label: &str) -> Result<usize> {
    block_map.get(label).copied().with_context(|| format!("undefined block `{label}`"))
}
