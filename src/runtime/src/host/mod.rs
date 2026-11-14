//! Shared Rust host services and C embedding adapters.
use crate::component_abi::*;
pub use crate::components::{ComponentOptions, ComponentLease, component_interface, component_ids};
pub use crate::{object::Value, pal::fs::Vfs, context::{VmContext, VmCore}, metadata::Module};
pub mod config;
pub mod resolver;
pub(crate) mod program;
pub mod signal;
pub mod error;
pub mod entry;
pub mod module;
pub mod marshal;
pub mod ops;
pub mod state;
mod ffi;
mod app;
pub(crate) mod vm_defaults;
pub(crate) mod program_builder;
mod load;
pub(crate) mod app_state;
pub(crate) mod services;
pub use z42_zpkg::{ExecMode, image::PackageImage};
pub use ffi::*;
pub use resolver::{read_zpkg_namespaces, ZpkgResolver};
pub use program::{LoadOptions, LoadedProgram};
pub use app::{default_mode, run_app, RunOpts};
pub use app_state::{program_args, set_program_args};
pub(crate) use app::report_on_exit;
#[cfg(test)] mod host_tests;
#[cfg(test)] mod app_tests;
#[cfg(test)] pub(crate) mod program_tests;

pub use artifact::{load_artifact, load_artifact_from_bytes, build_block_indices, fold_availability};
pub use crate::components::{BackendOptions, BackendSource};

pub use crate::context::extensions::VmExtensions;

pub use z42_abi::{Z42Value, Z42_VALUE_TAG_I64};

/// Ergonomic Rust embedding over the shared host services.
pub mod rust;

/// Process entry policy shared by the dynamic launcher and static embeddings.
#[cfg(feature = "cli")]
pub mod cli;
pub use crate::components::register_static_component;
pub use crate::loader::linked::{NativeSymbolResolver, register_static_library as register_static_native_library};
#[cfg(feature = "cli")]
mod launcher;
#[cfg(feature = "cli")]
pub mod entry_api;

pub use crate::config::{RuntimeConfig, VmConfig};

mod artifact;
mod extensions;

pub(crate) mod builtins;

mod profiling;
pub use profiling::{profiler_loaded, prepare_profiler, write_heap_snapshot};
