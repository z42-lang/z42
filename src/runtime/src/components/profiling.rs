//! Lazy attachment of the optional same-build profiler using the shared catalog.
use super::{api::*, backends::{select, BackendSource}, ComponentOptions};
use crate::context::VmContext;
use anyhow::{ensure, Result};

pub(crate) fn attach(ctx: &VmContext, source: &BackendSource, hz: u32, trace: bool) -> Result<()> {
    let options = select(ComponentOptions::default(), source, PROFILER_COMPONENT, "profiler")?;
    let components = options.create(ctx.heap().heap_epoch())?;
    let interface = components.lease::<ProfilerApi>(PROFILER_COMPONENT, ctx.core_arc())?
        .ok_or_else(|| anyhow::anyhow!("profiler component does not provide its profiling interface"))?;
    let identity = unsafe { (interface.core_identity)() };
    ensure!(identity.image == super::core_identity() && identity.abi == super::CORE_ABI,
        "profiler component core image or ABI mismatch; use the same runtime pack");
    ctx.core().diagnostics().sampler.retain(components)?;
    let status = unsafe { (interface.attach)((ctx as *const VmContext).cast(), hz, trace as u32) };
    ensure!(status == ComponentStatus::OK, "profiler attachment failed: status {}", status.0);
    Ok(())
}
