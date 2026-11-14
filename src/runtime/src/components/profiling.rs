//! Lazy attachment of the optional same-build profiler using the shared catalog.
use crate::components::abi::*;
use super::{backends::{select, BackendSource}, options::ComponentOptions};
use crate::context::VmContext;
use anyhow::{ensure, Result};

pub(crate) fn attach(ctx: &VmContext, source: &BackendSource, hz: u32, trace: bool) -> Result<()> {
    let existing = ctx.core().components().get().filter(|c| c.ids().contains(&PROFILER_COMPONENT));
    let components = if let Some(existing) = existing {
        ensure!(matches!(source, BackendSource::Auto), "profiler is configured through both ComponentOptions and LoadOptions.profiler");
        existing.clone()
    } else {
        select(ComponentOptions::default(), source, PROFILER_COMPONENT, "profiler")?
            .create(ctx.heap().heap_epoch())?
    };
    let interface = components.lease::<ProfilerApi>(PROFILER_COMPONENT, ctx.core_arc())?
        .ok_or_else(|| anyhow::anyhow!("profiler component does not provide its profiling interface"))?;
    let identity = unsafe { (interface.core_identity)() };
    crate::components::bridge::validate_core(identity, "profiler")?;
    ctx.core().diagnostics().sampler.retain(components)?;
    let status = unsafe { (interface.attach)((ctx as *const VmContext).cast(), hz, trace as u32) };
    ensure!(status == ComponentStatus::OK, "profiler attachment failed: status {}", status.0);
    Ok(())
}
