//! Decoded instructions, signatures, function bodies and module records.

pub mod reg_type;
mod exec_mode;
pub use exec_mode::ExecMode;

mod module;
mod class;
mod function;
mod insn;
mod instruction;
mod union_layout;

pub use module::*;
pub use class::*;
pub use function::*;
pub use insn::*;
pub use instruction::*;
pub use union_layout::*;

#[cfg(test)]
#[path = "ir_tests.rs"]
mod tests;
