//! Dispatch cache representation and publication primitives.
pub mod ic;
pub(crate) mod vcall;
pub(crate) mod ctor;
pub(crate) mod isa;
pub(crate) mod receiver;
pub(crate) mod functions;
pub(crate) mod isa_cache;

#[cfg(test)]
mod isa_cache_tests;
