//! Optional same-build profiling implementation; hosts consume z42::host.
pub use z42::{context, diagnostics, exec, gc, metadata, object, pal, threads, zpkg};
pub mod profiler;
pub use profiler::*;
