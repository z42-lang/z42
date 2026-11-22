//! Complete program assembly shared by file, byte, Rust and C hosts.
//! Availability folding precedes context construction and token resolution;
//! initializer registration and lazy-loader seeding precede any execution.

use crate::vm::VmContext;
use crate::loader::lazy::ZpkgCandidate;
use crate::metadata::Module;
use std::path::PathBuf;
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
    ctx.core().loader().configure_native(options.native)?;
    if let Some(module) = ctx.module() {
        resolve_tokens(&ctx, module);
    }
    crate::components::api::prepare_program(
        &ctx, options.mode, &options.components.components, &options.components.backends,
        &options.components.profiler, &options.jit_config,
    )?;
    for (name, byte_size) in initial_events {
        ctx.core().diagnostics().runtime_observers.fire(&crate::diagnostics::RuntimeEvent::ModuleLoaded {
            name,
            byte_size,
        });
    }
    Ok(ctx)
}

/// Everything besides the merged module that the context needs to find more code later.
pub(crate) struct ProgramInputs {
    /// The file system the VM reads and writes through (the caller probed its packages on it).
    pub vfs: std::sync::Arc<dyn crate::pal::fs::Vfs>,
    /// Directories the lazy loader searches for not-yet-loaded packages.
    pub search_dirs: Vec<PathBuf>,
    /// Declared-but-not-loaded packages (namespace → candidate). Also decides `available!`.
    pub declared_candidates: Vec<(String, ZpkgCandidate)>,
    /// File names of the packages already merged into the module.
    pub initially_loaded: Vec<String>,
    /// `impl` pairs contributed by the eagerly merged artifacts (user module included).
    pub eager_impl_pairs: Vec<(String, String)>,
    /// precise-pkg-refs: DEPS lists of the eagerly merged artifacts (user module included) —
    /// their symbol lists seed the lazy loader's "which package defines this name" table.
    pub eager_deps: Vec<crate::zpkg::format::ZpkgDep>,
    /// M1 no-eager-merge track: decoded dep artifacts registered per-package
    /// through the lazy loader after construction (empty on the merged track).
    pub eager_packages: Vec<crate::loader::LoadedArtifact>,
}

/// Build the context for a fully merged module (type registry / indices already built).
fn build_context(mut module: Module, inputs: ProgramInputs,
    freeze_extensions: fn(crate::vm::VmExtensions) -> crate::vm::ExtensionState,
) -> anyhow::Result<Pin<Box<VmContext>>> {
    // Fold before Core takes ownership and before any token is resolved.
    if let Some(avail_stats) = crate::loader::prepare_program_availability(
        &*inputs.vfs, &mut module, &inputs.declared_candidates,
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
    ctx.loader().install_lazy_loader_with_deps(
        inputs.search_dirs,
        string_pool_len,
        inputs.declared_candidates,
        inputs.initially_loaded,
    );
    // Seed lazy loader with merged module's TypeDescs (cross-zpkg base classes)
    // and eagerly-loaded artifacts' impl pairs.
    if let Some(m) = ctx.module() {
        let func_names: Vec<String> = m.functions.iter().map(|f| f.name.clone()).collect();
        ctx.loader().seed_lazy_loader_types(&m.type_registry, &func_names);
    }
    // M1 no-eager-merge track: register decoded dep packages through the lazy
    // loader's own machinery BEFORE the entry's types are seeded, so entry
    // definitions win any name conflict (closest to the merged last-wins).
    for artifact in inputs.eager_packages {
        ctx.loader().register_eager_artifact(artifact)?;
    }
    ctx.loader().seed_lazy_loader_impls(&inputs.eager_impl_pairs);
    ctx.loader().seed_lazy_loader_symbol_owners(&inputs.eager_deps);
    Ok(ctx)
}

/// Pre-fill execution tokens after loader setup and before component attachment.
pub(crate) fn resolve_tokens(ctx: &VmContext, module: &Module) {
    crate::metadata::resolve::resolve_module(module, &ctx.resolve_services());
}
