use super::*;

/// Precompute block label → index mapping for all functions in the module.
/// This eliminates the O(n) HashMap construction in every exec_function call.
pub fn build_block_indices(module: &mut Module) {
    for func in &mut module.functions {
        let ready = !func.blocks.is_empty() && func.branch_targets.len() == func.blocks.len();
        func.prepare();
        if !ready || func.fused_tails.len() != func.blocks.len() {
            func.fused_tails = crate::metadata::superinstr::compute_fused_tails(
                &func.blocks, &func.branch_targets, &func.reg_types);
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
