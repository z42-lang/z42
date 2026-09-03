

// refactor-split-bytecode（2026-09-03）：本文件只做汇总——各职责子模块在 `bytecode/` 下，
// 全量 `pub use` 使既有 `crate::metadata::bytecode::X` / `crate::metadata::X` 路径不变。
// `bytecode_tests` 经 `super::` 取这些名字（拆分前由本文件的 use 引入），保留私有 use 供其可见。
#[allow(unused_imports)]
use crate::metadata::types::{ExecMode, TypeDesc};
#[allow(unused_imports)]
use crate::metadata::tokens::TypeId;

mod module;
mod class;
mod function;
mod insn;
mod instruction;

pub use module::*;
pub use class::*;
pub use function::*;
pub use insn::*;
pub use instruction::*;

#[cfg(test)]
#[path = "bytecode_tests.rs"]
mod bytecode_tests;
