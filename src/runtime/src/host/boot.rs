//! Shared startup preparation for all Rust and C hosts.
//! Availability folding precedes context construction and token resolution;
//! initializer registration and lazy-loader seeding precede any execution.

use crate::vm::loader::lazy::ZpkgCandidate;
use crate::vm::runtime::Module;
use crate::vm::context::VmContext;
use std::path::PathBuf;
use std::pin::Pin;

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
        crate::vm::loader::availability::fold_availability(&*plan.vfs, &mut module, &plan.declared_candidates);
    if !avail_stats.is_noop() {
        // 剪枝改了块集合 → 派生侧表（block_index / branch_targets）必须按剪枝后的 CFG 重建。
        crate::vm::loader::package::build_block_indices(&mut module);
        crate::vm::loader::package::build_func_index(&mut module);
        tracing::debug!("available!: {avail_stats:?}");
    }

    let string_pool_len = module.string_pool.len();
    let ctx = VmContext::with_module_and_vfs(module, plan.vfs);
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
    crate::vm::runtime::resolve::resolve_module(module, ctx);
}
