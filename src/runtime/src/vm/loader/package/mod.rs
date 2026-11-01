//! Artifact loading, dependency resolution and execution preparation.
//!   `artifact`      — format-dispatch load (`load_artifact`) + zbc/zpkg loaders
//!   `indices`       — block/func index precompute

use std::path::Path;

use anyhow::{bail, Context, Result};

// Artifact reads go through the caller's VFS (the VM's `pal::fs::Vfs` handle), so a
// wasm host that mounts test zbcs / stdlib zpkgs into a `MemoryFs` loads them exactly
// as files on disk.
use crate::pal::fs::Vfs;

use crate::vm::runtime::Module;
use crate::vm::loader::locate::namespace::extract_import_namespaces_from_module;
use crate::vm::runtime::types::{build_type_registry, verify_constraints};
use crate::metadata::format::{ZpkgDep, ZBC_MAGIC, ZPKG_MAGIC};
use crate::vm::loader::package::merge::merge_modules;
use crate::metadata::reader::{
    parse_zbc_sidecar, parse_zpkg_sidecar, read_build_id, read_zbc, read_zpkg_file_entries,
    read_zpkg_meta, read_zpkg_modules,
};

pub mod merge;
mod artifact;
mod indices;

pub use crate::vm::loader::package::artifact::*;
pub use crate::vm::loader::package::indices::*;


#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
