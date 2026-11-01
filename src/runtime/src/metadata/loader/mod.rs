//! Compiler-artifact loader — split into concern submodules (
//! refactor-metadata-loader-split). This hub keeps the shared imports and
//! re-exports the flat `loader::*` API so external callers are unaffected:
//!   `artifact`      — format-dispatch load (`load_artifact`) + zbc/zpkg loaders
//!   `namespace`     — namespace/dependency resolution + import extraction
//!   `type_registry` — `build_type_registry` + cross-pkg inheritance fixup
//!   `constraints`   — `verify_constraints` post-registry validation pass
//!   `indices`       — block/func index precompute + class topo sort

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

// Artifact reads go through the caller's VFS (the VM's `pal::fs::Vfs` handle), so a
// wasm host that mounts test zbcs / stdlib zpkgs into a `MemoryFs` loads them exactly
// as files on disk.
use crate::pal::fs::Vfs;

use crate::vm::runtime::Module;
use super::format::{ZpkgDep, ZBC_MAGIC, ZPKG_MAGIC};
use super::merge::merge_modules;
use super::index::name::NameIndex;
use crate::object::{FieldSlot, TypeDesc};
use super::reader::{
    parse_zbc_sidecar, parse_zpkg_sidecar, read_build_id,
    read_zbc, read_zpkg_file_entries, read_zpkg_meta,
    read_zpkg_modules,
};

mod artifact;
mod namespace;
mod type_registry;
mod constraints;
mod indices;
// add-symbol-availability-macro：available!() 的加载期折叠 + CFG 剪枝。
mod availability;

pub use self::artifact::*;
pub use self::namespace::*;
pub use self::type_registry::*;
pub use self::constraints::*;
pub use self::indices::*;
pub use self::availability::*;


#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
