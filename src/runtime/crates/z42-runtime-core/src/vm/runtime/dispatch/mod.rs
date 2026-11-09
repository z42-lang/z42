//! Dispatch cache representation and publication primitives.
pub mod ic;
pub mod vcall;
pub mod ctor;
pub mod isa;
pub mod receiver;
pub mod functions;
pub mod isa_cache;

#[cfg(test)]
mod isa_cache_tests;
