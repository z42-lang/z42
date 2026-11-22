//! Prepare loaded modules for VM assembly.
use anyhow::{Context, Result};

use super::{
    artifact::FUSION_INPUTS,
    availability::AvailabilityStats,
    build_block_indices, fold_availability, merge_modules,
    package::{build_func_index, prepare_module},
    LoadedArtifact, ZpkgCandidate,
};
use crate::{metadata::Module, pal::fs::Vfs};

/// Keep dependencies available for per-package registration, or merge them
/// with the entry module when the caller selects the eager merge path.
pub(crate) fn prepare_program_modules(
    mut entry: Module,
    dependencies: Vec<LoadedArtifact>,
    merge_eager: bool,
    source: &str,
) -> Result<(Module, Vec<LoadedArtifact>)> {
    if !merge_eager {
        prepare_module(&mut entry, FUSION_INPUTS)
            .with_context(|| format!("constraint verification failed for `{source}`"))?;
        return Ok((entry, dependencies));
    }

    // With no merge, the entry already has the registries from artifact loading.
    if dependencies.is_empty() {
        return Ok((entry, dependencies));
    }

    // Entry lookup uses the original module name. Keep the entry last in the
    // merge order so the string pools, functions and types retain their order.
    let entry_name = entry.name.clone();
    let mut modules: Vec<_> = dependencies
        .into_iter()
        .map(|artifact| artifact.module)
        .collect();
    modules.push(entry);
    let mut module =
        merge_modules(modules).with_context(|| format!("merging modules for `{source}`"))?;
    module.name = entry_name;
    prepare_module(&mut module, FUSION_INPUTS)
        .with_context(|| format!("constraint verification failed for `{source}`"))?;
    Ok((module, Vec::new()))
}

/// Fold against declared packages and rebuild execution indices when the module changes.
pub(crate) fn prepare_program_availability(
    fs: &dyn Vfs,
    module: &mut Module,
    declared: &[(String, ZpkgCandidate)],
) -> Option<AvailabilityStats> {
    let stats = fold_availability(fs, module, declared);
    if stats.is_noop() {
        return None;
    }

    build_block_indices(module);
    build_func_index(module);
    Some(stats)
}
