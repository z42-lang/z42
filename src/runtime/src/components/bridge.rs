//! Minimal same-build VM views and physical image/layout identity for components.
// Component-facing VM identities and layout facts stay here when their owner
// moves internally. Export individual contracts, never whole implementation modules.
pub use crate::vm::{VmContext, VM_CONTEXT_CCTORS_OFFSET,
    VM_CONTEXT_SAFEPOINT_SKIP_OFFSET, VM_CONTEXT_STACK_LIMIT_OFFSET};
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
        core::mem::size_of::<crate::vm::VmContext>(),
        core::mem::size_of::<crate::vm::VmCore>(),
        core::mem::size_of::<crate::object::Value>(),
        core::mem::size_of::<crate::metadata::Function>(),
        core::mem::size_of::<crate::metadata::Module>(),
        core::mem::size_of::<crate::object::TypeDesc>(),
        core::mem::align_of::<crate::object::Value>(),
        crate::vm::VM_CONTEXT_STACK_LIMIT_OFFSET,
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
/// Function token resolution entry for independent backends: assembles the
/// minimal resolution view on this side of the bridge, so backends keep a
/// typed one-liner while `metadata::resolve` stays VM-free.
pub fn resolve_function_tokens(
    func: &crate::metadata::Function,
    module: &crate::metadata::Module,
    ctx: &crate::vm::VmContext,
) {
    crate::metadata::resolve::resolve_function_tokens(func, module, &ctx.resolve_services())
}

pub fn object_protocol(ctx: &crate::vm::VmContext) -> &crate::object::ObjectProtocol<
    crate::vm::VmContext, crate::exec::builtin_abi::BuiltinError,
> {
    ctx.core().object_protocol()
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub use crate::vm::jit_test_context;

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_loaded_program(ctx: std::pin::Pin<Box<VmContext>>, mode: crate::zpkg::ExecMode, entry: Option<String>) -> anyhow::Result<crate::host::LoadedProgram> {
    crate::host::LoadedProgram::new(ctx, mode, entry)
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_embedding_hello() -> Option<(&'static [u8], &'static str)> {
    #[cfg(z42_have_embedding_hello)]
    { Some((include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc")), env!("Z42_EMBEDDING_TEST_LIBS"))) }
    #[cfg(not(z42_have_embedding_hello))]
    { None }
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_host_jit_compiled() -> bool {
    crate::host::session::first_program().is_some_and(|program| {
        crate::diagnostics::capture_counters(program.context()).0.jit_methods_compiled > 0
    })
}
