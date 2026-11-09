//! Bind the loader's lazy fusion inputs to the process runtime config.
use std::path::PathBuf;

use crate::pal::fs::Vfs;
use crate::vm::loader::{LazyLoader, LoadedArtifact, ZpkgCandidate, availability, package};
use crate::vm::loader::fusion_inputs::{FusionInputs, FusionInputSource};
use crate::vm::runtime::Module;

pub(crate) const FUSION_INPUTS: FusionInputSource = process_fusion_inputs;

fn process_fusion_inputs() -> FusionInputs {
    let config = crate::config::runtime_config();
    FusionInputs {
        no_fusion: config.no_fusion,
        no_typed_fusion: config.no_typed_fusion,
        fusion_debug: config.fusion_debug,
    }
}

/// Load a compiler output artifact from `path` on `fs`, returning a `LoadedArtifact`.
/// The `.zsym` sidecar and an indexed zpkg's scattered zbcs are read from `fs` too.
///
/// Format is determined by file extension (case-insensitive):
/// - `.zbc`  → binary zbc (full mode)
/// - `.zpkg` → binary zpkg (packed mode)
pub fn load_artifact(fs: &dyn Vfs, path: &str) -> anyhow::Result<LoadedArtifact> {
    package::load_artifact_with_fusion(fs, path, FUSION_INPUTS)
}

/// Load a compiler output artifact from in-memory bytes, returning a
/// `LoadedArtifact`. Format is detected by magic bytes.
///
/// Used by the embedding API (`z42_host_load_zbc`) where the host hands
/// the runtime raw bytes rather than a filesystem path. Behaviour
/// mirrors [`load_artifact`] modulo the source of the byte stream;
/// the same registry / verification / index passes run.
///
/// Spec: docs/internals/src/runtime/embedding.md §4.4 (z42_host_load_zbc),
///       变更 add-embedding-api/.
pub fn load_artifact_from_bytes(raw: &[u8]) -> anyhow::Result<LoadedArtifact> {
    package::load_artifact_from_bytes_with_fusion(raw, FUSION_INPUTS)
}

/// Precompute block label → index mapping for all functions in the module.
/// This eliminates the O(n) HashMap construction in every exec_function call.
pub fn build_block_indices(module: &mut Module) {
    package::build_block_indices_with_fusion(module, FUSION_INPUTS)
}

/// 加载期折叠 + 剪枝。返回统计供测试与 trace 断言。
///
/// `declared` 是「声明了但尚未加载」的候选 zpkg（与 lazy loader 用的是同一份）；probe 从 `fs`
/// （这台 VM 的文件系统）读它们。
pub fn fold_availability(
    fs: &dyn Vfs,
    module: &mut Module,
    declared: &[(String, ZpkgCandidate)],
) -> availability::AvailabilityStats {
    availability::fold_availability_with_fusion(fs, module, declared, FUSION_INPUTS)
}

pub trait LazyLoaderFactory {
    fn new(search_dirs: Vec<PathBuf>, main_pool_len: usize, declared: Vec<(String, ZpkgCandidate)>, initially_loaded: Vec<String>) -> Self;
}
impl LazyLoaderFactory for LazyLoader {
    fn new(
        search_dirs: Vec<PathBuf>,
        main_pool_len: usize,
        declared: Vec<(String, ZpkgCandidate)>,
        initially_loaded: Vec<String>,
    ) -> Self {
        Self::with_fusion_inputs(search_dirs, main_pool_len, declared, initially_loaded, FUSION_INPUTS)
    }
}
