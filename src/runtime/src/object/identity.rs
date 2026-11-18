//! Identity vocabulary carried by object payloads.
//!
//! `NativeData` handles and reflection payloads reference load contexts and
//! assemblies by id. The newtypes live here — with the representation that
//! carries them — so `object` does not depend on the loader domain for its
//! payload surface; `loader::contexts` re-exports them as the registry owner.

/// Identifies one load context in the loader's `ContextRegistry`.
/// `ContextId(0)` is always the永驻 root. Stable for the lifetime of one
/// `VmContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ContextId(pub u32);

impl ContextId {
    /// The永驻 root context — core/stdlib/main program. Never collectible.
    pub const ROOT: ContextId = ContextId(0);
}

/// Identifies one loaded assembly (zpkg runtime projection) in the loader's
/// `ContextRegistry`. `AssemblyId(0)` is the synthetic root assembly (the
/// merged root `Module`); it carries no owned arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct AssemblyId(pub u32);

impl AssemblyId {
    /// The synthetic root assembly — every `typeof(T)` for a root type reports
    /// this. Not collectible; owns no private arena.
    pub const ROOT: AssemblyId = AssemblyId(0);
}
