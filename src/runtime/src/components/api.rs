//! VM-only capability access. Provider identities and management stay inside components.
use crate::{component_abi::ExecutionApi, context::VmContext, zpkg::ExecMode};
use anyhow::Result;
pub(crate) use super::execution_roles::ExecutionRole;
pub(crate) use super::{BackendSource, ComponentLease, VmComponents};
use super::{BackendOptions, ComponentOptions, execution_roles::ExecutionBindings};

/// Prepare once before publishing the program; execution uses its bound engines.
pub(crate) fn prepare_program(
    ctx: &VmContext, mode: ExecMode, components: &ComponentOptions,
    backends: &BackendOptions, profiler: &BackendSource,
) -> Result<()> {
    let selected = backends.components(mode, components)?;
    let mut roles = vec![(ExecutionRole::Baseline, crate::component_abi::INTERP_COMPONENT)];
    if mode == ExecMode::Jit {
        roles.push((ExecutionRole::Optimizing, crate::component_abi::JIT_COMPONENT));
    }
    let bindings = ExecutionBindings::new(roles)?;
    ctx.core().diagnostics().sampler.set_source(profiler.clone())?;
    selected.install_bound(ctx, bindings)?;
    super::backends::attach(ctx, mode)?;
    if ctx.core().diagnostics().sampler.enabled() {
        profiler_service(ctx)?;
    }
    Ok(())
}

/// Pure lookup: no discovery, loading, factory callbacks or instance creation.
pub(crate) fn execution(ctx: &VmContext, role: ExecutionRole) -> Result<Option<ComponentLease<ExecutionApi>>> {
    match ctx.core().components().get() {
        Some(components) => components.execution(role, ctx.core_arc()),
        None => Ok(None),
    }
}

pub(crate) fn execution_supported(role: ExecutionRole) -> bool {
    match role {
        ExecutionRole::Baseline => true,
        ExecutionRole::Optimizing => super::backends::jit_supported(),
    }
}

/// Known source presence, not an ABI-validity guarantee. Preparation validates it.
pub(crate) fn execution_available(role: ExecutionRole) -> bool {
    match role {
        ExecutionRole::Baseline => super::backends::interp_available(),
        ExecutionRole::Optimizing => super::backends::jit_available(),
    }
}

pub(crate) fn profiler_service(ctx: &VmContext) -> Result<&dyn crate::diagnostics::sampling::ProfilerBackend> {
    ctx.core().diagnostics().sampler.provider(ctx)
}

pub(crate) fn attach_profiler(ctx: &VmContext, source: &BackendSource, hz: u32, trace: bool) -> Result<()> {
    super::profiling::attach(ctx, source, hz, trace)
}

pub(crate) fn engine_registry() -> crate::exec::engine::EngineRegistry {
    super::backends::registry()
}

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) fn native_paths() -> Vec<std::path::PathBuf> {
    super::backends::native_paths()
}
