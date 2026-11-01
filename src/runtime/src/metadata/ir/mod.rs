//! Decoded instructions, signatures, function bodies and module records.

pub mod reg_type;

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
#[path = "ir_tests.rs"]
mod tests;
