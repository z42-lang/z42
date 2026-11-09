//! Cranelift JIT backend: compilation, instruction translation and native activation.
//! The facade registers this backend; every compiled call uses its running thread's context.

mod engine;
pub(crate) mod compile;
pub(crate) mod runtime;
pub(crate) mod helpers;
mod translate;

#[cfg(all(test, z42_test_facade))]
#[path = "lazy_load_tests.rs"]
mod lazy_load_tests;
#[cfg(all(test, z42_test_facade))]
#[path = "generic_call_tests.rs"]
mod generic_call_tests;

use crate::vm::runtime::Module;
use compile::vm_interface::JitVm;
use anyhow::Result;
use crate::vm::context::VmContext;
use runtime::{frame::JitFrame, invoke};
use helpers::take_exception_error;

// ─── Public API ─────────────────────────────────────────────────────────────

/// Run entry function `entry_name` on the JIT installed on `ctx`'s VM (no
/// static-init). The calling thread's `ctx` is the compiled code's context
/// parameter for the whole activation.
///
/// lazy-per-function-jit : the entry is compiled on its first call
/// ([`compile::state::JitRuntime::resolve_fn_by_id`]); a non-translatable entry runs on
/// the interpreter, which still routes hot callees to native code.
pub fn run_fn(ctx: &VmContext, entry_name: &str) -> Result<()> {
    let rt = crate::jit::compile::state::runtime(&ctx).ok_or_else(|| anyhow::anyhow!("JIT: no JIT installed on this VM"))?;
    // Scope the heap as the ambient GC heap for this JIT run. The entry's
    // bottom-frame push installs it too (`vm/exec/call/engine_guards.rs`); this
    // one also covers the entry's lazy compile before that push.
    let _heap_guard = crate::vm::exec::ambient::HeapGuard::enter(ctx.heap());
    let module = rt.module();
    // Resolve (and lazily compile on first call) the entry function.
    let Some(entry) = rt.resolve_fn_by_name(ctx, entry_name) else {
        // Not JIT-translatable (contains an interp-only opcode such as
        // `LoadLocalAddr`) or absent from the module. Run it on the interpreter
        // instead of hard-failing — covers a skipped entry-point or
        // `__static_init__`.
        // make-vm-loading-lazy: the entry may be an untranslatable function in a
        // lazily-loaded zpkg (e.g. a dep's `__static_init__`) that is NOT in the
        // merged module — resolve it through the lazy loader, mirroring interp's
        // path.
        let holder;
        let func = match module.func_index.get(entry_name).and_then(|&idx| module.functions.get(idx)) {
            Some(f) => f,
            None => {
                holder = ctx.try_lookup_function(entry_name)
                    .ok_or_else(|| anyhow::anyhow!("JIT: entry `{}` not found", entry_name))?;
                holder.as_ref()
            }
        };
        return match crate::vm::exec::engine::invoke(ctx, module, func, &[])? {
            crate::vm::exec::call::outcome::ExecOutcome::Returned(_) => Ok(()),
            crate::vm::exec::call::outcome::ExecOutcome::Thrown(val) =>
                Err(anyhow::anyhow!("{}", crate::vm::exec::exception::format_uncaught(&val, ctx, module))),
        };
    };
    // One `VmFrame` push enrolling the entry frame's regs (GC roots)
    // + function (trace); inner calls go through the same `invoke::enter_compiled`.
    let outcome = unsafe { invoke::call_entry(ctx, entry, JitFrame::new(ctx, entry.max_reg, &[])) };
    if let invoke::NativeOutcome::Threw = outcome {
        return Err(take_exception_error(ctx, module));
    }
    Ok(())
}

/// Run with static initialisation, then the given entry function.
///
/// unify-static-init-into-cctor（7.4）：静态字段初始化器已全部并入**每类型的类型初始化器**、
/// 按首次使用惰性触发（读依赖包静态字段时屏障先跑完它的初始化器）。这里只清空静态字段、
/// 排空「待加载类型」队列（T3），让屏障在访问点有类型可查，然后跑入口。
///
/// runtime-jit-tiering Phase 1: the entry runs via `run_fn` → `resolve_fn_by_id`
/// (non-tiered) → compile-on-first-call. Only tiered call sites
/// (`jit_call`/vcall/closure/ctor + the interp central divert) apply the tier-up
/// threshold.
fn run_with_static_init(ctx: &VmContext, entry_name: &str) -> Result<()> {
    ctx.static_fields_clear();
    ctx.run_pending_static_inits();
    run_fn(ctx, entry_name)
}

// ─── Public entry point called from vm/mod.rs ───────────────────────────────────

/// Called by `Vm::run` when the execution mode is JIT.
///
/// Installs the VM's JIT ([`compile::state::install`] — one per VM, shared by every
/// thread; reused when already installed) without compiling any user function;
/// each function is compiled on its first call (see
/// [`compile::state::JitRuntime::resolve_fn_by_id`]). The counters therefore report what
/// was **actually** compiled:
/// - `jit_methods_compiled` is incremented per compiled function (not the module's
///   total function count) — on the shard of the thread that compiled it;
/// - `jit_compile_us_total` accumulates each lazy compile's duration — likewise.
///
/// The one-per-run `JitModuleCompiled` event still fires here, reporting the
/// module **size** (`functions().len()`) and the **setup** time (not a
/// whole-module compile time). Per-function compile events are deferred to a
/// future observability spec (avoids one event per function).
pub fn run(ctx: &VmContext, module: &Module, entry_name: &str, config: config::JitConfig) -> Result<()> {
    // E1.P2 Phase 1 : metadata reads routed through `JitVm`.
    let function_count = module.functions().len() as u32;
    let start = std::time::Instant::now();

    compile::state::install(ctx, module, config)?;

    let setup_us = start.elapsed().as_micros() as u64;
    ctx.fire_runtime_event(&crate::vm::diagnostics::observer::RuntimeEvent::JitModuleCompiled {
        module_name:    module.module_name().to_string(),
        function_count,
        duration_us:    setup_us,
    });

    run_with_static_init(ctx, entry_name)
}

pub mod config;

/// Execute a host call with arguments and a returned value on this VM's JIT.
pub fn invoke_host(ctx: &VmContext, module: &Module, func: &crate::vm::runtime::Function,
    args: &[crate::object::Value], config: config::JitConfig) -> Result<crate::vm::exec::call::outcome::ExecOutcome> {
    let first_install = crate::jit::compile::state::runtime(&ctx).is_none();
    let start = std::time::Instant::now();
    compile::state::install(ctx, module, config)?;
    if first_install {
        ctx.fire_runtime_event(&crate::vm::diagnostics::observer::RuntimeEvent::JitModuleCompiled {
            module_name: module.module_name().to_owned(), function_count: module.functions().len() as u32,
            duration_us: start.elapsed().as_micros() as u64,
        });
    }
    let rt = crate::jit::compile::state::runtime(&ctx).ok_or_else(|| anyhow::anyhow!("JIT was not installed"))?;
    if let Some(entry) = rt.resolve_fn_by_name(ctx, &func.name) {
        let frame = JitFrame::new(ctx, entry.max_reg, args);
        Ok(unsafe { invoke::call_entry(ctx, entry, frame) }.into_exec(ctx))
    } else {
        crate::vm::exec::engine::invoke(ctx, module, func, args)
    }
}

pub mod component;

/// Internal mode-entry contract used by static host assembly.
pub fn execution_engine() -> &'static dyn crate::vm::exec::engine::Engine { &engine::ENGINE }
