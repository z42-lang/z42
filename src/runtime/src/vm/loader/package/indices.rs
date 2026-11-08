use super::*;
use crate::zpkg::ir::{BasicBlock, BranchTargets};
use crate::zpkg::ir::reg_type::IrType;
use crate::vm::runtime::SuperInstr;

pub(crate) fn build_block_indices_with_fusion(module: &mut Module, fusion_inputs: FusionInputSource) {
    for func in &mut module.functions {
        let ready = !func.blocks.is_empty() && func.branch_targets.len() == func.blocks.len();
        func.prepare();
        if !ready || func.fused_tails.len() != func.blocks.len() {
            func.fused_tails = compute_fused_tails(
                &func.blocks, &func.branch_targets, &func.reg_types, fusion_inputs);
        }
    }
}

/// Precompute function name → index mapping for O(1) call dispatch.
pub fn build_func_index(module: &mut Module) {
    module.func_index = module.functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
}

/// Precompute the fused-tail side table for a function's blocks (parallel to
/// `branch_targets`). Called once at load. Blocks with no fusable tail get `None`.
fn compute_fused_tails(blocks: &[BasicBlock], branch_targets: &[BranchTargets], reg_types: &[IrType], fusion_inputs: FusionInputSource) -> Vec<Option<SuperInstr>> {
    // Kill-switch / A/B knob: `Z42_NO_FUSION` disables recognition so the interp
    // takes the un-fused path — lets an operator turn the optimization off without a
    // rebuild (e.g. to isolate a suspected regression), and is how the framework's
    // isolated speedup is measured.
    let cfg = fusion_inputs();
    if cfg.no_fusion {
        return vec![None; blocks.len()];
    }
    // The knob is negative (`no-typed-fusion`), so the single inversion lives here —
    // the config field keeps the knob's own polarity (config.rs documents why).
    let typing_enabled = !cfg.no_typed_fusion;
    let out: Vec<Option<SuperInstr>> = blocks.iter().zip(branch_targets.iter())
        .map(|(b, t)| SuperInstr::recognize(b, t, reg_types, typing_enabled))
        .collect();
    if cfg.fusion_debug {
        let n = out.iter().filter(|o| o.is_some()).count();
        let typed = out.iter().filter(|o| matches!(o, Some(SuperInstr::CmpBr { typed: true, .. }))).count();
        if n > 0 { eprintln!("[FUSION] {} of {} blocks fused (CmpBr; {} typed i64)", n, blocks.len(), typed); }
    }
    out
}
