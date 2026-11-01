//! Package locations, namespace candidates and probing directory expansion.
pub mod namespace_index;
pub mod probing;
pub(crate) mod namespace;
pub use namespace::{extract_import_namespaces, resolve_dependency, resolve_namespace};

#[cfg(test)]
mod probing_tests;
