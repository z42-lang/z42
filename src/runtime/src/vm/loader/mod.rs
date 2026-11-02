//! Package loading, dependency discovery and load-context lifetime.
pub mod package;
pub mod locate;
pub mod lazy;
pub mod contexts;
pub mod availability;
pub use package::{load_artifact, load_artifact_from_bytes, LoadedArtifact};
pub use package::merge::merge_modules;
pub use locate::{extract_import_namespaces, resolve_dependency, resolve_namespace};
pub use locate::namespace_index::ZpkgCandidate;
pub use lazy::LazyLoader;
