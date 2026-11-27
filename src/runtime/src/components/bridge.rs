//! Minimal same-build VM views and physical image/layout identity for components.
//!
//! ```
//! use z42::components::bridge::{install_profiler_backend, register_baseline_engine, ProfilerBackend};
//! use z42::components::bridge::{poll_safepoint_slow, sample_frame_names};
//! let _: fn(&z42::vm::VmContext) = poll_safepoint_slow;
//! use z42::exec::Engine;
//! let _: fn(&z42::vm::VmContext) -> Vec<std::sync::Arc<str>> = sample_frame_names;
//! let _: unsafe fn(&z42::vm::VmContext, &'static dyn Engine) -> _ = register_baseline_engine;
//! use z42::vm::VmContext;
//! let _: unsafe fn(&VmContext, Box<dyn ProfilerBackend>) -> _ = install_profiler_backend;
//! ```
//! ```compile_fail
//! fn sampler(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.core().diagnostics().sampler;
//! }
//! ```
//! ```compile_fail
//! use z42::{components::bridge::PausedHeap, gc::Heap};
//! fn inspect(heap: &Heap) -> PausedHeap<'_> {
//!     unsafe { PausedHeap::new(heap) }
//! }
//! ```
// Component-facing VM identities and layout facts stay here when their owner
// moves internally. Export individual contracts, never whole implementation modules.
pub use crate::vm::{VmContext, VM_CONTEXT_CCTORS_OFFSET,
    VM_CONTEXT_SAFEPOINT_SKIP_OFFSET, VM_CONTEXT_STACK_LIMIT_OFFSET};
use crate::components::abi::ExecutionCoreAbi;
use crate::exec::{Engine, JitConfig, TierApi, TierBackend};
pub use crate::diagnostics::sampling::{HeapSnapshotWriter, ProfilerBackend, SampleBackend};
pub use crate::gc::walk::PausedHeap;
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

/// Handle the slow branch reached by a native inlined safepoint poll.
#[inline]
pub fn poll_safepoint_slow(ctx: &VmContext) {
    crate::threads::safepoint::check_safepoint_slow(ctx);
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

/// Register the engine supplied by the VM's validated baseline component.
///
/// # Safety
/// The engine and all of its code must outlive the VM. Component attachment
/// retains the validated component's code before calling this function.
pub unsafe fn register_baseline_engine(ctx: &VmContext, engine: &'static dyn Engine) -> anyhow::Result<()> {
    ctx.core().exec().engines.register_baseline(engine)
}

/// Register the engine and configuration supplied by the VM's validated JIT component.
///
/// # Safety
/// The engine and all of its code must outlive the VM. Component attachment
/// retains the validated component's code before calling this function.
pub unsafe fn register_jit_engine(
    ctx: &VmContext, engine: &'static dyn Engine, config: JitConfig,
) -> anyhow::Result<()> {
    ctx.core().exec().engines.register_jit(engine, config)
}

/// Read the configuration captured at JIT attachment.
pub fn jit_config(ctx: &VmContext) -> anyhow::Result<JitConfig> {
    ctx.core().exec().engines.jit_config()
}

/// Borrow the tier installed on this VM, even before this thread's cache is refreshed.
pub fn installed_tier(ctx: &VmContext) -> Option<&TierApi> {
    ctx.core().exec().engines.tier()
}

/// Publish the tier, returning false if another backend already owns the slot.
///
/// # Safety
/// The backend's code and destructor must remain loaded until the VM releases
/// it. Component attachment retains the JIT code before execution begins.
pub unsafe fn install_tier(ctx: &VmContext, backend: impl TierBackend) -> bool {
    ctx.core().exec().engines.install_tier(TierApi::new(backend)).is_ok()
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_engine_registered(ctx: &VmContext, mode: crate::zpkg::ExecMode) -> bool {
    crate::components::api::execution_registered(ctx, mode)
}

/// Capture the current thread's frame names, outermost first, for sampling.
/// An empty stack or a stack owned by another thread yields no names.
pub fn sample_frame_names(ctx: &VmContext) -> Vec<std::sync::Arc<str>> {
    ctx.exec().sample_frame_names()
}

/// Install the backend supplied by a validated same-build profiler component.
///
/// # Safety
/// The component containing the backend's vtable, methods and destructor must
/// remain loaded until the VM releases it. Component attachment retains that
/// code before calling this function.
pub unsafe fn install_profiler_backend(
    ctx: &VmContext,
    backend: Box<dyn ProfilerBackend>,
) -> anyhow::Result<()> {
    crate::diagnostics::install_profiler_backend(ctx, backend)
}

/// Inspect a loaded program in the packaged-component and JIT verification harnesses.
#[cfg(any(feature = "shared-core", feature = "jit-test-support"))]
#[doc(hidden)]
pub fn verification_context(program: &crate::vm::program::LoadedProgram) -> &VmContext {
    program.context()
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub use crate::vm::jit_test_context;

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_safepoint_counter(ctx: &VmContext) -> u32 {
    ctx.threads().safepoint_counter_for_test()
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_request_auto_collect(ctx: &VmContext) {
    ctx.core().threads().request_auto_collect_for_test();
}

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn test_loaded_program(ctx: std::pin::Pin<Box<VmContext>>, mode: crate::zpkg::ExecMode, entry: Option<String>) -> anyhow::Result<crate::host::LoadedProgram> {
    crate::host::LoadedProgram::new(ctx, mode, entry)
}

/// Bind test factories through the same role validation and attach path as a loaded program.
///
/// # Safety
/// Factories must satisfy the static component contract and be linked into the test binary.
#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub unsafe fn test_bind_execution(
    ctx: &VmContext,
    baseline: (super::abi::ComponentId, super::abi::ComponentInit),
    optimizing: (super::abi::ComponentId, super::abi::ComponentInit),
) -> anyhow::Result<()> {
    use super::{api::ExecutionRole, execution_roles::ExecutionBindings};

    let bindings = ExecutionBindings::new([
        (ExecutionRole::Baseline, baseline.0),
        (ExecutionRole::Optimizing, optimizing.0),
    ])?;
    let components = unsafe {
        use crate::loader::NativeOrigin::StaticLink;

        super::api::ComponentOptions::default()
            .with_linked(baseline.0, baseline.1, StaticLink)
            .with_linked(optimizing.0, optimizing.1, StaticLink)
    };
    components.install_bound(ctx, bindings)?;
    super::api::attach_execution(
        ctx, crate::zpkg::ExecMode::Jit, &crate::config::runtime_config_snapshot().jit_config(),
    )
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
