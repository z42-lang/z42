//! Compiler-artifact loader — split into concern submodules (2026-08-25,
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

// add-wasm-testhost (G6): artifact reads route through the platform fs backend
// (`native` = std::fs, byte-identical; `memory` = in-memory VFS on wasm) so a
// wasm host that mounts test zbcs / stdlib zpkgs can load them at runtime. On
// native this is exactly std::fs — no behaviour change.
use crate::corelib::fs_backend;

use super::bytecode::Module;
use super::formats::{ZpkgDep, ZBC_MAGIC, ZPKG_MAGIC};
use super::merge::merge_modules;
use super::test_index::TestEntry;
use super::name_index::NameIndex;
use super::types::{FieldSlot, TypeDesc};
use super::zbc_reader::{
    parse_zbc_sidecar, parse_zpkg_sidecar, read_build_id, read_directory_pub,
    read_test_index_resolved, read_zbc, read_zpkg_file_entries, read_zpkg_meta,
    read_zpkg_modules,
};

mod artifact;
mod namespace;
mod type_registry;
mod constraints;
mod indices;

pub use self::artifact::*;
pub use self::namespace::*;
pub use self::type_registry::*;
pub use self::constraints::*;
pub use self::indices::*;


#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
