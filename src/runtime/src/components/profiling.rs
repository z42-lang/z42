//! Lazy attachment of the optional same-build profiler using the shared catalog.
use crate::components::abi::*;
use super::{backends::{select, BackendSource}, options::ComponentOptions};
use crate::vm::VmContext;
use anyhow::{ensure, Result};

#[derive(Default)]
pub(crate) struct ProfilingState {
    source: std::sync::OnceLock<BackendSource>,
    // Owned by components, dropped after diagnostics releases its backend.
    code: std::sync::OnceLock<super::api::VmComponents>,
}
impl ProfilingState {
    pub(crate) fn set_source(&self, source: BackendSource) -> Result<()> {
        self.source.set(source).map_err(|_| anyhow::anyhow!("profiler source is already configured"))
    }
    #[cfg(test)]
    pub(crate) fn components(&self) -> Option<&super::api::VmComponents> { self.code.get() }
}

pub(crate) fn prepare(ctx: &VmContext, hz: u32, trace: bool) -> Result<()> {
    let state = ctx.core().profiling();
    attach(ctx, state.source.get_or_init(Default::default), hz, trace)
}

pub(crate) fn attach(ctx: &VmContext, source: &BackendSource, hz: u32, trace: bool) -> Result<()> {
    let existing = ctx.core().components().get().filter(|c| c.ids().contains(&PROFILER_COMPONENT));
    let components = if let Some(existing) = existing {
        ensure!(matches!(source, BackendSource::Auto), "profiler is configured through both explicit component selection and source selection");
        existing.clone()
    } else {
        let policy = ctx.loader().native_loading();
        select(ComponentOptions::default(), source, PROFILER_COMPONENT, "profiler", policy)?
            .create(ctx.heap().heap_epoch(), policy)?
    };
    let interface = components.lease::<ProfilerApi>(PROFILER_COMPONENT, ctx.core_arc())?
        .ok_or_else(|| anyhow::anyhow!("profiler component does not provide its profiling interface"))?;
    let identity = unsafe { (interface.core_identity)() };
    crate::components::bridge::validate_core(identity, "profiler")?;
    ctx.core().profiling().code.set(components)
        .map_err(|_| anyhow::anyhow!("profiler code is already retained"))?;
    let status = unsafe { (interface.attach)((ctx as *const VmContext).cast(), hz, trace as u32) };
    ensure!(status == ComponentStatus::OK, "profiler attachment failed: status {}", status.0);
    Ok(())
}
