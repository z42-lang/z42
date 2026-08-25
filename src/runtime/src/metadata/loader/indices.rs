use super::*;

/// Precompute block label → index mapping for all functions in the module.
/// This eliminates the O(n) HashMap construction in every exec_function call.
pub fn build_block_indices(module: &mut Module) {
    use crate::metadata::bytecode::{BranchTargets, Terminator};
    for func in &mut module.functions {
        func.block_index = func.blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.label.clone(), i))
            .collect();
        // perf-vm-iteration: pre-resolve each block's branch terminator labels
        // to block indices so the interp jumps by index (no per-branch SipHash).
        // NoBranch when a label is undefined — runtime then falls back to the
        // label HashMap, preserving the existing "undefined block" error path.
        let idx = &func.block_index;
        func.branch_targets = func.blocks
            .iter()
            .map(|b| match &b.terminator {
                Terminator::Br { label } => idx
                    .get(label.as_str())
                    .map(|&t| BranchTargets::Br(t))
                    .unwrap_or(BranchTargets::NoBranch),
                Terminator::BrCond { true_label, false_label, .. } => {
                    match (idx.get(true_label.as_str()), idx.get(false_label.as_str())) {
                        (Some(&t), Some(&f)) => BranchTargets::BrCond(t, f),
                        _ => BranchTargets::NoBranch,
                    }
                }
                _ => BranchTargets::NoBranch,
            })
            .collect();
        // interp-superinstr-fusion: recognize fused block tails once, parallel to
        // branch_targets (needs the index-resolved targets above).
        func.fused_tails = crate::metadata::superinstr::compute_fused_tails(&func.blocks, &func.branch_targets, &func.reg_types);
        // perf-frame-name-precompute: build the stack-frame (name, file) Arc<str>
        // pair once here so `exec_function_body` clones it O(1) per call instead
        // of re-formatting + allocating on every call (40–60% of call-heavy
        // interp time). `format_frame_name` needs `param_types` (in the cold box)
        // + the display name; the file comes from the line table's first entry.
        let file: std::sync::Arc<str> = func.line_table().first()
            .and_then(|e| e.file.clone())
            .map(std::sync::Arc::from)
            .unwrap_or_else(|| std::sync::Arc::from(""));
        let name = std::sync::Arc::from(crate::metadata::bytecode::format_frame_name(func));
        func.frame_meta = Some((name, file));
        // interp-frame-presize: backfill the register-file length so the interp
        // `Frame::new*` pre-sizes the register file in one `resize` instead of
        // growing one slot at a time through the cold `set_grow` path. The zbc
        // reader leaves `max_reg = 0` (it is not carried on the wire); computing
        // it here — in the always-compiled loader, no jit dependency — makes the
        // pre-sizing kick in for every build (incl. interp-only / wasm). The JIT
        // reaches the same count via `translate::max_reg` (`reg_file_len - 1`).
        func.max_reg = func.reg_file_len();
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

/// Return class names in topological order (base before derived).
pub(super) fn topo_sort_classes(module: &Module) -> Vec<String> {
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut order: Vec<String> = Vec::new();

    fn visit(
        name: &str,
        module: &Module,
        visited: &mut std::collections::HashSet<String>,
        order: &mut Vec<String>,
    ) {
        if visited.contains(name) { return; }
        visited.insert(name.to_string());
        if let Some(desc) = module.classes.iter().find(|c| c.name == name) {
            if let Some(base) = &desc.base_class {
                visit(base, module, visited, order);
            }
        }
        order.push(name.to_string());
    }

    for cls in &module.classes {
        visit(&cls.name, module, &mut visited, &mut order);
    }
    order
}
