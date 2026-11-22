//! Package loading, dependency discovery and load-context lifetime.
pub mod package;
pub mod locate;
pub mod lazy;
pub mod contexts;
pub mod availability;
pub use package::LoadedArtifact;
pub use package::merge::merge_modules;
pub use locate::{extract_import_namespaces, resolve_dependency, resolve_namespace};
pub use locate::namespace_index::ZpkgCandidate;
pub use lazy::LazyLoader;

pub mod context_gc;
pub mod fusion_inputs;

mod state;
pub use state::LoaderState;

pub mod image_cache;
pub mod native;
pub mod linked;
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) mod code;
pub(crate) mod native_paths;

mod runtime;
pub use runtime::RuntimeLoader;

mod artifact;
mod dependency_candidates;
pub(crate) use dependency_candidates::{build_declared_candidates, version_mismatch_hint};
pub use artifact::{load_artifact, load_artifact_from_bytes, build_block_indices, fold_availability};
pub(crate) use artifact::{load_artifact_cached, load_artifact_from_bytes_cached, FUSION_INPUTS};

mod native_policy;
pub use native_policy::NativeLoading;
pub use image_cache::{ImageCache, content_id};
pub(crate) use native_policy::NativeOrigin;

mod ffi;
