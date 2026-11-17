//! C embedding adapter for the z42_host ABI (`include/z42_host_api.h`).
//!
//! Config validation, the process session, marshaling and the `extern "C"`
//! entry points live here; the Rust embedding surface stays at the host root
//! and reaches this group only through its re-exports.
pub(crate) mod config;
pub(crate) mod ffi;
pub(crate) mod marshal;
pub(crate) mod ops;
pub(crate) mod state;
