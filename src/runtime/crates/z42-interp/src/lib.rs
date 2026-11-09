//! Independent interp execution backend, sharing the runtime core.
pub use z42_runtime::{vm, object, gc, pal, objops, zpkg};
pub mod interp;
pub use interp::*;

pub use z42_runtime::{core_identity, CORE_ABI};

pub use z42_runtime::components::api;
