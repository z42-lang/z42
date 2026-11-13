//! Optional analysis services exposed through the embedding facade.
use crate::context::VmContext;
pub fn profiler_loaded(ctx: &VmContext) -> bool { ctx.core().diagnostics().sampler.is_loaded() }
/// Resolve the configured profiler, validating its build/core identity first.
pub fn prepare_profiler(ctx: &VmContext) -> anyhow::Result<()> {
    ctx.core().diagnostics().sampler.provider(ctx).map(|_| ())
}
pub fn write_heap_snapshot(ctx: &VmContext, path: &str) -> anyhow::Result<i64> {
    ctx.core().diagnostics().sampler.provider(ctx)?.write_heap_snapshot(ctx, path)
}
