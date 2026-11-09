//! Independent interp execution backend, sharing the runtime core.
pub use z42::{context, exec, loader, metadata, diagnostics, threads, object, gc, pal, objops, zpkg};
pub mod interp;
pub use interp::*;

pub use z42::components::{core_identity, CORE_ABI};

pub use z42::components::api;
