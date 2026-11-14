//! Optional same-build profiling implementation; hosts consume z42::host.
use z42::{context, diagnostics, gc, object, pal};
#[cfg(test)]
use z42::{exec, metadata, zpkg};
pub mod profiler;
pub use profiler::*;
