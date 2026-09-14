//! Merged module → ready-to-run `VmContext`: the boot steps shared by every entry path.
//!
//! fix-host-static-init (2026-09-14): there are two ways to run z42 code —
//! [`crate::app::run`] (the `z42vm` binary, `z42_run_app`, wasm `runTestApp`) and the
//! embedding host API (`z42_host_load_zbc` → `z42_host_invoke`, in `host/ops.rs`). Both
//! merge the user module with its dependencies the same way, but the host kept a
//! hand-copied, older version of what happens *after* the merge. Every boot step added
//! since then landed only in `app::run`, and the host silently fell behind:
//!
//! | step | why it matters | host before |
//! |---|---|---|
//! | `fold_availability` | `available!(X)` folded to a constant, dead branch pruned | missing |
//! | `VmContext::with_module` | module shared through the ctx (`__thread_spawn` needs it) | `new()` |
//! | `register_cctor_of` | static-constructor barrier knows the eagerly merged types | missing |
//! | `seed_lazy_loader_{types,impls}` | lazy packages see merged base classes / impls | missing |
//! | `alloc_func_ref_slots` + `resolve_module` | `LoadFnCached` slots, dispatch tokens | missing |
//! | `init_static_fields` | runs `__static_init__` of merged packages | **missing** |
//!
//! The last one was visible: a static field with an initializer read back as its type
//! default (`OSKind.Wasm` → 0, so `Platform.IsWasm()` was always false) on every
//! embedding, first noticed on wasm. So these steps live here, once, and both paths call
//! them — a new boot step added here reaches both.
//!
//! Static-field initialization itself stays with the caller: `Vm::run` runs it right
//! before the entry; the host runs it once per module on the first invoke (inside the
//! host stdout sink, so initializer output reaches the host).

use crate::metadata::lazy_loader::ZpkgCandidate;
use crate::metadata::Module;
use crate::vm_context::VmContext;
use std::path::PathBuf;
use std::pin::Pin;

/// Everything besides the merged module that the context needs to find more code later.
pub(crate) struct BootPlan {
    /// Directories the lazy loader searches for not-yet-loaded packages.
    pub search_dirs: Vec<PathBuf>,
    /// Declared-but-not-loaded packages (namespace → candidate). Also decides `available!`.
    pub declared_candidates: Vec<(String, ZpkgCandidate)>,
    /// File names of the packages already merged into the module.
    pub initially_loaded: Vec<String>,
    /// `impl` pairs contributed by the eagerly merged artifacts (user module included).
    pub eager_impl_pairs: Vec<(String, String)>,
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
        crate::metadata::loader::fold_availability(&mut module, &plan.declared_candidates);
    if !avail_stats.is_noop() {
        // 剪枝改了块集合 → 派生侧表（block_index / branch_targets）必须按剪枝后的 CFG 重建。
        crate::metadata::loader::build_block_indices(&mut module);
        crate::metadata::loader::build_func_index(&mut module);
        tracing::debug!("available!: {avail_stats:?}");
    }

    let string_pool_len = module.string_pool.len();
    let ctx = VmContext::with_module(module);
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
        ctx.seed_lazy_loader_types(&m.type_registry);
    }
    ctx.seed_lazy_loader_impls(&plan.eager_impl_pairs);
    ctx
}

/// Per-module execution setup that must precede running any function of `module`.
/// Idempotent, but meant to run once per module.
pub(crate) fn prepare_execution(ctx: &VmContext, module: &Module) {
    // 2026-05-02 add-method-group-conversion (D1b): pre-allocate the FuncRef
    // cache slots needed by `LoadFnCached` instructions for this module's
    // global slot range.
    ctx.alloc_func_ref_slots(module.func_ref_cache_slots);

    // introduce-method-token Phase 3 (2026-05-08): pre-resolve dispatch
    // tokens for every Function. Idempotent — safe if hot paths run
    // before Phase 4 hookups consume the cache (they fall back to
    // string lookup until Phase 4 lands).
    crate::metadata::resolver::resolve_module(module, ctx);
}
