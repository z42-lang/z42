//! Bind the loader's lazy fusion inputs to the process runtime config.
use std::path::PathBuf;

use crate::pal::fs::Vfs;
use crate::loader::{LazyLoader, DecodedProgram, ZpkgCandidate, package};
use crate::loader::fusion_inputs::{FusionInputs, FusionInputSource};
use crate::metadata::Module;

pub(crate) const FUSION_INPUTS: FusionInputSource = process_fusion_inputs;

fn process_fusion_inputs() -> FusionInputs {
    let config = crate::config::runtime_config();
    FusionInputs {
        no_fusion: config.no_fusion,
        no_typed_fusion: config.no_typed_fusion,
        fusion_debug: config.fusion_debug,
    }
}

/// Decode the Z42 program at `path` on `fs`.
/// The `.zsym` sidecar and an indexed zpkg's scattered zbcs are read from `fs` too.
///
/// Format is determined by file extension (case-insensitive):
/// - `.zbc`  → binary zbc (full mode)
/// - `.zpkg` → binary zpkg (packed mode)
pub fn decode_program_file(fs: &dyn Vfs, path: &str) -> anyhow::Result<DecodedProgram> {
    package::decode_program_file_with_fusion(fs, path, FUSION_INPUTS)
}

/// Decode Z42 program bytes. The header magic selects the format.
///
/// Used by the embedding API (`z42_host_load_zbc`) where the host hands
/// the runtime raw bytes rather than a filesystem path. Behaviour
/// mirrors [`decode_program_file`] modulo the source of the byte stream;
/// the same registry / verification / index passes run.
///
/// Spec: docs/internals/src/runtime/embedding.md §4.4 (z42_host_load_zbc),
///       变更 add-embedding-api/.
pub fn decode_program_bytes(raw: &[u8]) -> anyhow::Result<DecodedProgram> {
    package::decode_program_bytes_with_fusion(raw, FUSION_INPUTS)
}

/// Decode bytes through the optional session cache. Linked VM state stays private
/// to each returned decoded while successful decodes share their definitions.
pub(crate) fn decode_program_bytes_cached(
    bytes: &[u8], cache: Option<&super::ImageCache>,
) -> anyhow::Result<DecodedProgram> {
    match cache {
        Some(cache) => cache.program_for(&super::content_id(bytes), || decode_program_bytes(bytes)),
        None => decode_program_bytes(bytes),
    }
}

/// Cache-backed reads use the file bytes as the identity. With no cache, use
/// the path loader to retain sidecar and indexed package resolution.
pub(super) fn decode_program_file_cached(
    fs: &dyn Vfs, path: &str, cache: Option<&super::ImageCache>,
) -> anyhow::Result<DecodedProgram> {
    match cache {
        Some(cache) => decode_program_bytes_cached(&fs.read(path)?, Some(cache)),
        None => decode_program_file(fs, path),
    }
}

/// Precompute block label → index mapping for all functions in the module.
/// This eliminates the O(n) HashMap construction in every exec_function call.
pub fn build_block_indices(module: &mut Module) {
    package::build_block_indices_with_fusion(module, FUSION_INPUTS)
}

impl LazyLoader {
    /// Construct a dependency loader using the host runtime configuration.
    pub fn new(
        search_dirs: Vec<PathBuf>,
        main_pool_len: usize,
        declared: Vec<(String, ZpkgCandidate)>,
        initially_loaded: Vec<String>,
    ) -> Self {
        Self::with_fusion_inputs(search_dirs, main_pool_len, declared, initially_loaded, FUSION_INPUTS)
    }
}
