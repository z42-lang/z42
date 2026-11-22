//! Prepare loaded modules for VM assembly.
use anyhow::{Context, Result};

use super::{
    artifact::FUSION_INPUTS,
    availability::AvailabilityStats,
    build_block_indices, fold_availability,
    package::{build_func_index, prepare_module},
    LoadedArtifact, ZpkgCandidate,
};
use crate::{metadata::Module, pal::fs::Vfs};

/// Package inputs collected by the host before VM construction.
pub(crate) struct StartupPackages {
    pub(crate) search_dirs: Vec<std::path::PathBuf>,
    pub(crate) declared_candidates: Vec<(String, ZpkgCandidate)>,
    pub(crate) initially_loaded: Vec<String>,
    pub(crate) impl_pairs: Vec<(String, String)>,
    pub(crate) entry_dependencies: Vec<crate::zpkg::format::ZpkgDep>,
    pub(crate) artifacts: Vec<LoadedArtifact>,
}

/// Prepare the entry module for per-package assembly (merge-retirement M4
/// final): dependencies stay `LoadedArtifact`s and register through the lazy
/// loader after construction; the entry gets its own registry/indices pass.
pub(crate) fn prepare_program_modules(
    mut entry: Module,
    dependencies: Vec<LoadedArtifact>,
    source: &str,
) -> Result<(Module, Vec<LoadedArtifact>)> {
    prepare_module(&mut entry, FUSION_INPUTS)
        .with_context(|| format!("constraint verification failed for `{source}`"))?;
    Ok((entry, dependencies))
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
