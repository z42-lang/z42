//! Independent interp execution backend, sharing the runtime core.
pub use z42::{vm, object, gc, pal, objops, zpkg};
pub mod interp;
pub use interp::*;

pub use z42::{core_identity, CORE_ABI};

pub use z42::components::api;
