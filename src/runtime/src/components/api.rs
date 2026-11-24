//! VM-only capability access. Provider identities and management stay inside components.
use crate::{components::abi::{ExecutionApi, ExecutionConfig, ComponentStatus}, vm::VmContext, zpkg::ExecMode};
use anyhow::Result;
pub(crate) use super::execution_roles::ExecutionRole;
pub use super::backends::{BackendOptions, BackendSource};
pub use super::instance::{ComponentLease, VmComponents};
pub use super::options::ComponentOptions;
#[cfg(test)]
pub use super::options::{component_interface, component_ids};
use super::execution_roles::ExecutionBindings;

/// Prepare once before publishing the program; execution uses its bound engines.
pub(crate) fn prepare_program(
    ctx: &VmContext, mode: ExecMode, components: &ComponentOptions,
    backends: &BackendOptions, profiler: &BackendSource, jit_config: &crate::exec::JitConfig,
) -> Result<()> {
    let selected = backends.components(mode, components, ctx.loader().native_loading())?;
    let mut roles = vec![(ExecutionRole::Baseline, crate::components::abi::INTERP_COMPONENT)];
    if mode == ExecMode::Jit {
        roles.push((ExecutionRole::Optimizing, crate::components::abi::JIT_COMPONENT));
    }
    let bindings = ExecutionBindings::new(roles)?;
    ctx.core().profiling().set_source(profiler.clone())?;
    selected.install_bound(ctx, bindings)?;
    attach_execution(ctx, mode, jit_config)?;
    Ok(())
}

/// Pure lookup: no discovery, loading, factory callbacks or instance creation.
pub(crate) fn execution(ctx: &VmContext, role: ExecutionRole) -> Result<Option<ComponentLease<ExecutionApi>>> {
    match ctx.core().components().get() {
        Some(components) => components.execution(role, ctx.core_arc()),
        None => Ok(None),
    }
}

/// Known source presence, not an ABI-validity guarantee. Preparation validates it.
pub(crate) fn execution_available(role: ExecutionRole) -> bool {
    match role {
        ExecutionRole::Baseline => super::backends::interp_available(),
        ExecutionRole::Optimizing => super::backends::jit_available(),
    }
}

/// Whether this execution mode already has a registered engine on the VM.
pub(crate) fn execution_registered(ctx: &VmContext, mode: ExecMode) -> bool {
    ctx.core().exec().engines.is_registered(mode)
}

pub(crate) use super::profiling::{ProfilingState, prepare as prepare_profiler};

pub(super) fn attach_execution(ctx: &VmContext, mode: ExecMode, cfg: &crate::exec::JitConfig) -> Result<()> {
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
        let config = ExecutionConfig { threshold: cfg.threshold, osr_threshold: cfg.osr_threshold,
            interp_tierup: cfg.interp_tierup, profile: cfg.profile as u32, debug_promote: cfg.debug_promote as u32 };
        let status = unsafe { (interface.attach)((ctx as *const VmContext).cast(), &config) };
        anyhow::ensure!(status == ComponentStatus::OK, "{name} registration failed: status {}", status.0);
    }
    anyhow::ensure!(execution_registered(ctx, ExecMode::Interp), "interpreter component did not register its execution engine");
    if mode == ExecMode::Jit {
        anyhow::ensure!(execution_registered(ctx, ExecMode::Jit), "JIT component did not register its execution engine");
    }
    Ok(())
}

pub(crate) fn engine_registry() -> crate::exec::EngineRegistry {
    super::backends::registry()
}
