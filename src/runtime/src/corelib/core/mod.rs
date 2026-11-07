//! Foundation builtins shared by the standard library capability groups.

pub mod convert;
pub mod string;
pub mod math;
pub mod object;
pub mod reflection;
pub mod assemblyloadcontext;
pub mod diagnostics;
pub mod appprops;
pub mod array;
pub mod config;
pub mod char;
pub mod gc;
pub mod bench;
pub mod threading;
pub mod monitor;
pub mod crypto;
pub(crate) mod symavail;
mod sync_contention;
