//! Complete program assembly shared by file, byte, Rust and C hosts.
//! Availability folding precedes context construction and token resolution;
//! initializer registration and lazy-loader seeding precede any execution.

use crate::context::VmContext;
use crate::loader::lazy::ZpkgCandidate;
use crate::metadata::Module;
use std::path::PathBuf;
use std::pin::Pin;

use super::program::{LoadOptions, LoadedProgram};
use anyhow::Result;

/// Publish a callable program only after metadata and component setup succeeds.
/// Initialization execution remains in the invocation and execution barriers.
pub(crate) fn build(
    module: Module,
    inputs: ProgramInputs,
    options: &LoadOptions,
    entry_hint: Option<String>,
    initial_events: Vec<(String, Option<u64>)>,
) -> Result<LoadedProgram> {
    let ctx = build_context(module, inputs);
    if let Some(module) = ctx.module() {
        resolve_tokens(&ctx, module);
    }
    let components = options
        .backends
        .components(options.mode, &options.components)?;
    ctx.core().diagnostics().sampler.set_source(options.profiler.clone())?;
    components.install(&ctx)?;
    crate::components::backends::attach(&ctx, options.mode)?;
    if ctx.core().diagnostics().sampler.enabled() {
        ctx.core().diagnostics().sampler.provider(&ctx)?;
    }
    for (name, byte_size) in initial_events {
        ctx.fire_runtime_event(&crate::diagnostics::observer::RuntimeEvent::ModuleLoaded {
            name,
            byte_size,
        });
    }
    LoadedProgram::new(ctx, options.mode, entry_hint)
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
}

/// Build the context for a fully merged module (type registry / indices already built).
fn build_context(mut module: Module, inputs: ProgramInputs) -> Pin<Box<VmContext>> {
    // Availability needs built type/function indices and a mutable module. Fold
    // before Core takes ownership and before any token is resolved, so unavailable
    // branches never expose unresolved call sites to execution preparation.
    let avail_stats = crate::host::artifact::fold_availability(
        &*inputs.vfs,
        &mut module,
        &inputs.declared_candidates,
    );
    if !avail_stats.is_noop() {
        // 剪枝改了块集合 → 派生侧表（block_index / branch_targets）必须按剪枝后的 CFG 重建。
        crate::host::artifact::build_block_indices(&mut module);
        crate::loader::package::build_func_index(&mut module);
        tracing::debug!("available!: {avail_stats:?}");
    }

    let string_pool_len = module.string_pool.len();
    let ctx = super::vm_defaults::construct(
        Some(module),
        inputs.vfs,
        Default::default(),
        crate::exec::engine::EngineRegistry::new(None),
    );
    // Entry types register initialization facts before any use. The lazy loader
    // does the same when additional types become visible.
    if let Some(m) = ctx.module() {
        for td in m.type_registry.values() {
            ctx.register_cctor_of(td);
        }
    }
    ctx.install_lazy_loader_with_deps(
        inputs.search_dirs,
        string_pool_len,
        inputs.declared_candidates,
        inputs.initially_loaded,
    );
    // Seed lazy loader with merged module's TypeDescs (cross-zpkg base classes)
    // and eagerly-loaded artifacts' impl pairs.
    if let Some(m) = ctx.module() {
        let func_names: Vec<String> = m.functions.iter().map(|f| f.name.clone()).collect();
        ctx.seed_lazy_loader_types(&m.type_registry, &func_names);
    }
    ctx.seed_lazy_loader_impls(&inputs.eager_impl_pairs);
    ctx.seed_lazy_loader_symbol_owners(&inputs.eager_deps);
    ctx
}

/// Pre-fill execution tokens after loader setup and before component attachment.
pub(crate) fn resolve_tokens(ctx: &VmContext, module: &Module) {
    crate::metadata::resolve::resolve_module(module, ctx);
}
