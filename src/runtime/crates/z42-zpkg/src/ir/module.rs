//! Decoded static module contents and function-name indexes.

use super::*;
use rustc_hash::FxHashMap;

/// Top-level bytecode module.
/// Loaded from `.zbc` binary (or legacy `.z42ir.json`).
#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub string_pool: Vec<String>,
    pub classes: Vec<ClassDesc>,
    pub functions: Vec<Function>,
    /// Pre-built function name → index mapping for O(1) call dispatch.
    /// Populated by the loader after deserialisation.
    pub func_index: FxHashMap<String, usize>,
}
