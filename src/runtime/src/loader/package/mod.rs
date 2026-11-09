//! Artifact loading, dependency resolution and execution preparation.
//!   `artifact`      — format-dispatch load (`load_artifact`) + zbc/zpkg loaders
//!   `indices`       — block/func index precompute

use std::path::Path;

use anyhow::{bail, Context, Result};

// Artifact reads go through the caller's VFS (the VM's `pal::fs::Vfs` handle), so a
// wasm host that mounts test zbcs / stdlib zpkgs into a `MemoryFs` loads them exactly
// as files on disk.
use crate::pal::fs::Vfs;
use super::fusion_inputs::FusionInputSource;

use crate::metadata::Module;
use crate::loader::locate::namespace::extract_import_namespaces_from_module;
use crate::metadata::types::{build_type_registry, verify_constraints};
use crate::zpkg::format::{ZpkgDep, ZBC_MAGIC, ZPKG_MAGIC};
use crate::loader::package::merge::merge_modules;
use crate::zpkg::reader::{
    parse_zbc_sidecar, parse_zpkg_sidecar, read_build_id, read_zbc, read_zpkg_file_entries,
    read_zpkg_meta, read_zpkg_modules,
};

pub mod merge;
mod artifact;
mod indices;

pub use artifact::LoadedArtifact;
pub use indices::build_func_index;
pub use artifact::{load_artifact_with_fusion, load_artifact_from_bytes_with_fusion};
pub use indices::build_block_indices_with_fusion;


#[cfg(all(test, z42_test_facade))]
#[path = "loader_tests.rs"]
mod tests;
