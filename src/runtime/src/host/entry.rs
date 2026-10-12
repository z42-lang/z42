//! Resolved entry handle (function / static method). H1 placeholder.
//!
//! Spec: docs/spec/archive/2026-05-10-add-embedding-api/design.md D2 (generational slab).

/// Opaque pointee for `Z42EntryRef`. Real backing in H2.
#[repr(C)]
pub struct Z42Entry {
    _private: [u8; 0],
}
