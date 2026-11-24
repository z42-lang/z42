//! Complete program assembly shared by file, byte, Rust and C hosts.
//! Availability folding precedes context construction and token resolution;
//! initializer registration and lazy-loader seeding precede any execution.

use crate::vm::VmContext;
use crate::metadata::Module;
use std::pin::Pin;

use anyhow::Result;

#[derive(Default)]
pub(crate) struct ComponentInputs {
    pub components: crate::components::api::ComponentOptions,
    pub backends: crate::components::api::BackendOptions,
    pub profiler: crate::components::api::BackendSource,
}
pub(crate) struct AssemblyOptions<'a> {
    pub native: crate::loader::NativeLoading,
    pub mode: crate::zpkg::ExecMode,
    pub jit_config: crate::exec::JitConfig,
    pub components: &'a ComponentInputs,
    pub freeze_extensions: fn(crate::vm::VmExtensions) -> crate::vm::ExtensionState,
}

/// Prepare the context before the host publishes a callable program.
/// Initialization execution remains in the invocation and execution barriers.
pub(crate) fn build(
    module: Module,
    inputs: ProgramInputs,
    options: AssemblyOptions<'_>,
    initial_events: Vec<(String, Option<u64>)>,
) -> Result<Pin<Box<VmContext>>> {
    let ctx = build_context(module, inputs, options.freeze_extensions)?;
    ctx.loader().configure_native(options.native)?;
    if let Some(module) = ctx.module() {
        resolve_tokens(&ctx, module);
    }
    crate::components::api::prepare_program(
        &ctx, options.mode, &options.components.components, &options.components.backends,
        &options.components.profiler, &options.jit_config,
    )?;
    crate::diagnostics::prepare_sampling(&ctx)?;
    let events = ctx.core().diagnostics().module_loads();
    for (name, byte_size) in initial_events {
        events.loaded(name, byte_size);
    }
    Ok(ctx)
}

/// File system and package inputs for a prepared entry module.
pub(crate) struct ProgramInputs {
    pub vfs: std::sync::Arc<dyn crate::pal::fs::Vfs>,
    pub packages: crate::loader::StartupPackages,
}

fn build_context(mut module: Module, inputs: ProgramInputs,
    freeze_extensions: fn(crate::vm::VmExtensions) -> crate::vm::ExtensionState,
) -> anyhow::Result<Pin<Box<VmContext>>> {
    // Fold before Core takes ownership and before any token is resolved.
    if let Some(avail_stats) = crate::loader::prepare_program_availability(
        &*inputs.vfs, &mut module, &inputs.packages.declared_candidates,
    ) {
        tracing::debug!("available!: {avail_stats:?}");
    }

    let string_pool_len = module.string_pool.len();
    let ctx = super::defaults::construct(
        Some(module),
        inputs.vfs,
        Default::default(),
        crate::exec::EngineRegistry::new(None),
        freeze_extensions,
    );
    // Entry types register initialization facts before any use. The lazy loader
    // does the same when additional types become visible.
    if let Some(m) = ctx.module() {
        for td in m.type_registry.values() {
            ctx.initialization().register_cctor_of(td);
        }
    }
    ctx.loader().install_startup_packages(string_pool_len, inputs.packages)?;
    Ok(ctx)
}

/// Pre-fill execution tokens after loader setup and before component attachment.
pub(crate) fn resolve_tokens(ctx: &VmContext, module: &Module) {
    crate::metadata::resolve::resolve_module(module, &ctx.resolve_services());
}
