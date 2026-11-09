//! Independent interp execution backend, sharing the runtime core.
pub use z42_runtime_core::{vm, object, gc, pal, objops, zpkg};
pub mod interp;
pub use interp::*;

pub use z42_runtime_core::{core_identity, CORE_ABI};

pub use z42_runtime_core::components::api;
