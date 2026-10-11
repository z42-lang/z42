//! Loaded `.zbc` module handle. H1 placeholder — real loading lands in H2.
//!
//! Spec: 变更 add-embedding-api D2 (generational slab).

/// Opaque pointee for `Z42ModuleRef`. Defined as a zero-sized type so the
/// pointer is purely a token; the real backing store lives in
/// `HostState::modules` once H2 introduces it.
#[repr(C)]
pub struct Z42Module {
    _private: [u8; 0],
}
