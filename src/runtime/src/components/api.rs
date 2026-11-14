//! VM-only capability access. Provider identities and management stay inside components.
use crate::{components::abi::{ExecutionApi, ExecutionConfig, ComponentStatus}, context::VmContext, zpkg::ExecMode};
use anyhow::Result;
pub(crate) use super::execution_roles::ExecutionRole;
pub use super::backends::{BackendOptions, BackendSource};
pub use super::instance::{ComponentLease, VmComponents};
pub use super::options::{ComponentOptions, component_interface, component_ids};
pub use super::linked::register_static_component;
use super::execution_roles::ExecutionBindings;

/// Prepare once before publishing the program; execution uses its bound engines.
pub(crate) fn prepare_program(
    ctx: &VmContext, mode: ExecMode, components: &ComponentOptions,
    backends: &BackendOptions, profiler: &BackendSource,
) -> Result<()> {
    let selected = backends.components(mode, components)?;
    let mut roles = vec![(ExecutionRole::Baseline, crate::components::abi::INTERP_COMPONENT)];
    if mode == ExecMode::Jit {
        roles.push((ExecutionRole::Optimizing, crate::components::abi::JIT_COMPONENT));
    }
    let bindings = ExecutionBindings::new(roles)?;
    ctx.core().diagnostics().sampler.set_source(profiler.clone())?;
    selected.install_bound(ctx, bindings)?;
    attach_execution(ctx, mode)?;
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

pub(super) fn attach_execution(ctx: &VmContext, mode: ExecMode) -> Result<()> {
    for role in [ExecutionRole::Baseline, ExecutionRole::Optimizing] {
        if role == ExecutionRole::Optimizing && mode != ExecMode::Jit { continue; }
        let name = if role == ExecutionRole::Baseline { "interpreter" } else { "JIT" };
        let components = ctx.core().components().get()
            .ok_or_else(|| anyhow::anyhow!("execution components are not prepared"))?;
        let interface = execution(ctx, role)?
            .ok_or_else(|| anyhow::anyhow!("{name} component does not provide its execution interface"))?;
        let identity = unsafe { (interface.core_identity)() };
        crate::components::bridge::validate_core(identity, name)?;
        let code = components.execution_code(role)
            .ok_or_else(|| anyhow::anyhow!("{name} component has no code lease"))?;
        // Acquire code lifetime before installing any backend-owned trait objects.
        ctx.core().exec().engines.retain_code(code);
        let cfg = crate::config::runtime_config().jit_config();
        let config = ExecutionConfig { threshold: cfg.threshold, osr_threshold: cfg.osr_threshold,
            interp_tierup: cfg.interp_tierup, profile: cfg.profile as u32, debug_promote: cfg.debug_promote as u32 };
        let status = unsafe { (interface.attach)((ctx as *const VmContext).cast(), &config) };
        anyhow::ensure!(status == ComponentStatus::OK, "{name} registration failed: status {}", status.0);
    }
    anyhow::ensure!(ctx.core().exec().engines.has_baseline(), "interpreter component did not register its execution engine");
    if mode == ExecMode::Jit {
        anyhow::ensure!(ctx.core().exec().engines.has_jit(), "JIT component did not register its execution engine");
    }
    Ok(())
}

pub(crate) fn engine_registry() -> crate::exec::engine::EngineRegistry {
    super::backends::registry()
}

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) fn native_paths() -> Vec<std::path::PathBuf> {
    super::backends::native_paths()
}
