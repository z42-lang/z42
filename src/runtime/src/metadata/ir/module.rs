//! Decoded module contents and loader-populated type/function indexes.

use super::*;
use crate::metadata::types::TypeDesc;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Top-level bytecode module.
/// Loaded from `.zbc` binary (or legacy `.z42ir.json`).
#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub string_pool: Vec<String>,
    pub classes: Vec<ClassDesc>,
    pub functions: Vec<Function>,
    /// Pre-built type descriptor registry — populated by the loader after
    /// deserialisation, not stored on disk.  Maps fully-qualified class name
    /// to the corresponding `TypeDesc` (field layout + vtable).
    pub type_registry: FxHashMap<String, Arc<TypeDesc>>,
    /// Pre-built function name → index mapping for O(1) call dispatch.
    /// Populated by the loader after deserialisation.
    pub func_index: FxHashMap<String, usize>,
}
