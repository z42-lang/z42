//! Module（zbc 模块：函数表 / 类表 / 字符串池 / 类型注册表）。refactor-split-bytecode（2026-09-03）：从 1334 行的 `bytecode.rs` 按职责拆出，
//! 对外路径不变（`metadata::bytecode::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{ExecMode, TypeDesc};
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
