//! Shared startup preparation for all Rust and C hosts.
//! Availability folding precedes context construction and token resolution;
//! initializer registration and lazy-loader seeding precede any execution.

use crate::loader::lazy::ZpkgCandidate;
use crate::metadata::Module;
use crate::context::VmContext;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// Everything besides the merged module that the context needs to find more code later.
pub(crate) struct BootPlan {
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
pub(crate) fn boot_context(mut module: Module, plan: BootPlan) -> Pin<Box<VmContext>> {
    // add-symbol-availability-macro：`available!(X)` 折成常量 + 剪死分支。
    //
    // 位置是**硬约束**，不能随便挪：
    //   - 必须在 type_registry / func_index 建好之后（判定要查它们）；
    //   - 必须在 `VmContext::with_module` 之前——那之后 Module 进 Arc 就不可变了，
    //     原地 CFG 剪枝的窗口只有这里；
    //   - 必须在任何 token 解析 / 执行之前，这样被剪掉分支里的 call site 永不被解析，
    //     后续 `fix-silent-symbol-resolution` 的急切校验也就不会对它抛出。
    //     `available!` 正是那条校验的唯一显式豁免通道。
    let avail_stats =
        crate::host::artifact::fold_availability(&*plan.vfs, &mut module, &plan.declared_candidates);
    if !avail_stats.is_noop() {
        // 剪枝改了块集合 → 派生侧表（block_index / branch_targets）必须按剪枝后的 CFG 重建。
        crate::host::artifact::build_block_indices(&mut module);
        crate::loader::package::build_func_index(&mut module);
        tracing::debug!("available!: {avail_stats:?}");
    }

    let string_pool_len = module.string_pool.len();
    let ctx = crate::host::boot::construct(Some(module), plan.vfs, Default::default(),
        crate::exec::engine::EngineRegistry::new(None));
    // add-static-constructors：急切合并进来的类型（主程序 + stdlib + eager deps）在此登记
    // 静态构造器。跨包惰性加载的类型由 `try_lookup_type` 登记——两处合起来保证
    // 「登记早于使用」，这是 cctor 屏障那个 `pending` 门成立的前提。
    if let Some(m) = ctx.module() {
        for td in m.type_registry.values() {
            ctx.register_cctor_of(td);
        }
    }
    ctx.install_lazy_loader_with_deps(
        plan.search_dirs,
        string_pool_len,
        plan.declared_candidates,
        plan.initially_loaded,
    );
    // Seed lazy loader with merged module's TypeDescs (cross-zpkg base classes)
    // and eagerly-loaded artifacts' impl pairs.
    if let Some(m) = ctx.module() {
        let func_names: Vec<String> = m.functions.iter().map(|f| f.name.clone()).collect();
        ctx.seed_lazy_loader_types(&m.type_registry, &func_names);
    }
    ctx.seed_lazy_loader_impls(&plan.eager_impl_pairs);
    ctx.seed_lazy_loader_symbol_owners(&plan.eager_deps);
    ctx
}

/// Per-module execution setup that must precede running any function of `module`.
/// Idempotent, but meant to run once per module.
pub(crate) fn prepare_execution(ctx: &VmContext, module: &Module) {
    // introduce-method-token Phase 3 : pre-resolve dispatch
    // tokens for every Function. Idempotent — safe if hot paths run
    // before Phase 4 hookups consume the cache (they fall back to
    // string lookup until Phase 4 lands).
    crate::metadata::resolve::resolve_module(module, ctx);
}

impl VmContext {
    /// Standard test entry: constructs a VmContext with `VmCore.module = None`.
    /// Cargo unit tests use this — they don't need a real Module for
    /// builtin / static-field / alloc tests. Production paths use
    /// [`with_module`](Self::with_module).
    pub fn new() -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, crate::pal::fs::default_vfs(), Default::default())
    }

    /// [`new`](Self::new) on the file system `vfs` instead of the default one.
    pub fn with_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, vfs, Default::default())
    }

    /// Production entry: constructs a VmContext with the user's compiled
    /// Module wrapped in `Arc` for cross-thread sharing. Required by any
    /// path that may invoke `__thread_spawn` (which dispatches into the
    /// shared module from the spawned thread).
    pub fn with_module(module: crate::metadata::Module) -> std::pin::Pin<Box<Self>> {
        Self::with_module_and_vfs(module, crate::pal::fs::default_vfs())
    }

    /// [`with_module`](Self::with_module) on the file system `vfs`: every path-based file
    /// builtin and every package load of this VM goes through it.
    pub fn with_module_and_vfs(
        module: crate::metadata::Module,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(Some(module), vfs, Default::default())
    }

    /// Create a VM with owned extension registrations, frozen for its entire lifetime.
    /// `None` omits the entry module, as in [`Self::new`]. Module linkage is reset
    /// exactly as in [`Self::with_module_and_vfs`]. Worker contexts share this state.
    pub fn with_extensions(
        module: Option<crate::metadata::Module>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
        extensions: crate::context::extensions::VmExtensions,
    ) -> std::pin::Pin<Box<Self>> {
        construct(module, vfs, extensions, crate::components::backends::registry())
    }

    /// Instantiate shared package definitions with fresh per-VM types and caches.
    pub fn with_image(
        image: Arc<crate::zpkg::image::PackageImage>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        Self::with_image_and_vfs(image, crate::pal::fs::default_vfs())
    }

    /// Instantiate an image using this VM's file system for subsequent loads.
    pub fn with_image_and_vfs(
        image: Arc<crate::zpkg::image::PackageImage>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        let mut module = crate::metadata::Module::from_image(image);
        crate::metadata::types::build_type_registry(&mut module);
        crate::metadata::types::verify_constraints(&module)?;
        crate::host::artifact::build_block_indices(&mut module);
        Ok(Self::with_module_and_vfs(module, vfs))
    }

}

#[cfg(test)]
#[path = "boot_tests.rs"]
mod tests;

/// Construct a context with explicit engine bindings; high-level hosts attach
/// selected components only after loading and metadata preparation have succeeded.
pub(crate) fn construct(module: Option<crate::metadata::Module>, vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::context::extensions::VmExtensions, engines: crate::exec::engine::EngineRegistry) -> std::pin::Pin<Box<VmContext>> {
        let module = module.map(|mut module| {
            module.clear_link_state();
            Arc::new(module)
        });
        let ctx = with_config(module, vfs, crate::host::extensions::freeze(extensions),
            crate::config::runtime_config().vm_config(), crate::corelib::object_protocol(), engines);
        #[cfg(feature = "native-interop")]
        if let Err(e) = crate::interop::ext::load_all(&ctx) { tracing::warn!("native ext loader: {e:#}"); }
        ctx
}

pub(crate) fn with_config(module: Option<Arc<crate::metadata::Module>>, vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::context::extensions::ExtensionState, config: crate::config::VmConfig,
    protocol: crate::exec::object_protocol::ObjectProtocol, engines: crate::exec::engine::EngineRegistry) -> std::pin::Pin<Box<VmContext>> {
    let backend = match config.sample_hz {
        Some(hz) => crate::profiler::cpu::Sampler::start(hz, config.trace_timeline),
        None => crate::profiler::cpu::Sampler::disabled(),
    };
    VmContext::with_runtime(module, vfs, crate::context::RuntimeInputs {
        extensions, config, object_protocol: protocol, engines,
        builtins: crate::corelib::builtins::BUILTINS,
        catalog: &crate::corelib::builtins::CATALOG,
        fusion: crate::host::artifact::FUSION_INPUTS,
        sampler: crate::diagnostics::sampling::Sampling::new(backend.enabled(), backend),
    })
}
