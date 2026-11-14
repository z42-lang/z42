//! Minimal same-build VM views and physical image/layout identity for components.
use crate::components::abi::ExecutionCoreAbi;
static CORE_IDENTITY: u8 = 0;
pub fn core_identity() -> *const core::ffi::c_void { core::ptr::addr_of!(CORE_IDENTITY).cast() }
pub const CORE_ABI: ExecutionCoreAbi = ExecutionCoreAbi {
    flags: (cfg!(debug_assertions) as u64)
        | ((cfg!(feature = "jit") as u64) << 1)
        | ((cfg!(feature = "native-interop") as u64) << 2)
        | ((cfg!(feature = "component-loading") as u64) << 3)
        | ((cfg!(feature = "net") as u64) << 4)
        | ((cfg!(feature = "mimalloc-alloc") as u64) << 5)
        | ((cfg!(feature = "profile-contention") as u64) << 6),
    layouts: [
        core::mem::size_of::<crate::context::VmContext>(),
        core::mem::size_of::<crate::context::VmCore>(),
        core::mem::size_of::<crate::object::Value>(),
        core::mem::size_of::<crate::metadata::Function>(),
        core::mem::size_of::<crate::metadata::Module>(),
        core::mem::size_of::<crate::object::TypeDesc>(),
        core::mem::align_of::<crate::object::Value>(),
        crate::context::VM_CONTEXT_STACK_LIMIT_OFFSET,
    ],
};

/// Every same-core component must pass this check before borrowing VM addresses.
pub(super) fn validate_core(identity: crate::components::abi::ExecutionIdentity, name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(identity.image == core_identity() && identity.abi == CORE_ABI,
        "{name} component core image or ABI mismatch; use components from the same shared-core runtime pack");
    Ok(())
}

/// Borrow object capabilities already bound to this VM. Execution components call
/// this only after their same-core identity has been validated; no registry lookup.
#[inline]
pub fn object_protocol(ctx: &crate::context::VmContext) -> &crate::object::protocol::ObjectProtocol<
    crate::context::VmContext, crate::exec::call::builtin_abi::BuiltinError,
> {
    ctx.core().object_protocol()
}
