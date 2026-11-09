//! Dispatch cache representation and publication primitives.
pub mod ic;
pub mod vcall;
pub mod ctor;
pub mod isa;
pub mod receiver;
pub mod functions;
pub mod isa_cache;

#[cfg(all(test, z42_test_facade))]
mod isa_cache_tests;
