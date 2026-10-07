//! 引擎无关的对象操作层（objops）：字段、数组、静态字段、值 struct 叶子的读写语义只在这里实现一次。
//!
//! interp 的 `exec_*` 与 JIT 的 `helpers/*` 都是薄适配层：从寄存器取 `&Value`、调这里、把结果写回寄存器，
//! 再把 [`OpError`] 映射到各自的异常通道（interp：`Ok(Some(exc))` / `Err`；JIT：pending 异常 + 返回 1）。
//! 对象与数组单元格的存储表示（字节布局、引用侧表、打包基元、写屏障）对引擎不可见——改表示只改本模块
//! 与 `metadata::types`。
//!
//! 机制见 `docs/internals/src/runtime/interp-jit-semantics.md`「对象操作：objops」。

pub mod array;
pub mod array_bulk;
pub mod error;
pub mod field;
pub mod statics;
pub mod struct_leaf;

pub use error::{ArrayOp, OpError, OpResult, Throw};

#[cfg(test)]
#[path = "objops_tests.rs"]
mod objops_tests;
