//! Package locations, namespace candidates and probing directory expansion.
pub mod namespace_index;
pub mod probing;
pub mod namespace;
pub use namespace::{extract_import_namespaces, resolve_dependency, resolve_namespace};

#[cfg(all(test, z42_test_facade))]
mod probing_tests;
