//! Resolved entry handle (function / static method). H1 placeholder.
//!
//! Spec: 变更 add-embedding-api D2 (generational slab).

/// Opaque pointee for `Z42EntryRef`. Real backing in H2.
#[repr(C)]
pub struct Z42Entry {
    _private: [u8; 0],
}
