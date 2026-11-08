//! Concrete component selection and shared application/host startup.
pub(crate) mod context;
pub(crate) mod engine;
pub(crate) mod extension;
pub(crate) mod builtin;
pub(crate) mod object_protocol;
pub(crate) mod loader;
pub(crate) mod boot;

#[cfg(feature = "native-interop")]
pub(crate) mod repl;
