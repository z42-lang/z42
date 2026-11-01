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

/// Return class names in topological order (base before derived).
pub(super) fn topo_sort_classes(module: &Module) -> Vec<String> {
    let base_of: rustc_hash::FxHashMap<&str, Option<&str>> = module.classes.iter()
        .map(|c| (c.name.as_str(), c.base_class.as_deref()))
        .collect();
    let mut visited: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
    let mut order: Vec<String> = Vec::new();

    fn visit<'m>(
        name: &'m str,
        base_of: &rustc_hash::FxHashMap<&'m str, Option<&'m str>>,
        visited: &mut rustc_hash::FxHashSet<&'m str>,
        order: &mut Vec<String>,
    ) {
        if !visited.insert(name) { return; }
        if let Some(Some(base)) = base_of.get(name) {
            visit(base, base_of, visited, order);
        }
        order.push(name.to_string());
    }

    for cls in &module.classes {
        visit(&cls.name, &base_of, &mut visited, &mut order);
    }
    order
}
