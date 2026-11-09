//! Shared runtime mechanisms. Concrete backends and host policy are supplied by the facade.
pub use z42_zpkg as zpkg;
pub mod vm;
pub mod object;
pub mod gc;
pub mod pal;
pub use vm::exec::ops::object as objops;
mod identity;
pub use identity::{core_identity, CORE_ABI};

pub use vm::components;
