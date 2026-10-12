/// JIT backend — compiles z42 bytecode to native machine code using Cranelift.
///
/// Architecture
/// ------------
/// * `frame.rs`        — JitFrame (register file) and FnEntry (one compiled function)
/// * `runtime.rs`      — JitRuntime: the VM's one JIT on `VmCore` (slots, compiler, OSR
///                       cache), shared by every thread; compiled code and helpers take the
///                       running thread's own `VmContext` as their context parameter
/// * `lazy.rs`         — LazyCompiler: per-function compilation on first call / OSR entries
/// * `invoke.rs`       — `call_native`: the single push_frame → native code → pop_frame sequence
/// * `helpers/`        — `extern "C"` helper functions called by JIT code, split
///                       by `Instruction` category and registered through
///                       `helpers::registry`. See `helpers/mod.rs` for the list.
/// * `translate/`      — Cranelift IR translation (one submodule per instruction category)
/// * `reg_access.rs`   — `frame.regs` slot load / store
/// * `vm_interface.rs` — read-only metadata contract between JIT and VM
/// * `mod.rs`          — `run` / `run_fn`: install the runtime (compiles nothing), run the entry

/// `[Native]` extern-wrapper short-circuit: call helpers dispatch the builtin directly.
mod forward;
pub(crate) mod frame; // runtime-jit-tiering Phase 1.5: interp dispatch reaches JitFrame
pub(crate) mod helpers;
/// The single native-call sequence (push_frame → compiled code → pop_frame), shared by
/// the JIT entry, the call helpers and the interpreter's mixed-mode diverts.
pub(crate) mod invoke;
/// Lazy per-function compilation state (lazy-per-function-jit, 2026-07-23).
mod lazy;
/// The VM's shared JIT state (`VmCore::jit`): slots, compiler, OSR cache.
pub(crate) mod runtime;
/// Centralized `frame.regs` slot access — the single load/store choke point
/// (jit-unbox-regalloc Phase 2.0; foundation for the 2B/2C register cache).
mod reg_access;
mod translate;
/// JIT↔VM read-only metadata contract — review.md Part 1 P0 / E1.P2
/// Phase 1 (2026-06-02). Compile-time path goes through this trait;
/// helpers still reach Module via raw pointer (Phase 2 territory).
pub(crate) mod vm_interface;

#[cfg(test)]
#[path = "lazy_load_tests.rs"]
mod lazy_load_tests;

use crate::metadata::Module;
use vm_interface::JitVm;
use anyhow::Result;
use crate::vm_context::VmContext;
use frame::JitFrame;
use helpers::take_exception_error;

// ─── Public API ─────────────────────────────────────────────────────────────

/// Run entry function `entry_name` on the JIT installed on `ctx`'s VM (no
/// static-init). The calling thread's `ctx` is the compiled code's context
/// parameter for the whole activation.
///
/// lazy-per-function-jit (2026-07-23): the entry is compiled on its first call
/// ([`runtime::JitRuntime::resolve_fn_by_id`]); a non-translatable entry runs on
/// the interpreter, which still routes hot callees to native code.
pub fn run_fn(ctx: &VmContext, entry_name: &str) -> Result<()> {
    let rt = ctx.jit().ok_or_else(|| anyhow::anyhow!("JIT: no JIT installed on this VM"))?;
    // Scope the heap as the ambient GC heap for this JIT run. The entry's
    // bottom-frame push installs it too (`vm_context/engine_guards.rs`); this
    // one also covers the entry's lazy compile before that push.
    let _heap_guard = crate::gc::ambient::HeapGuard::enter(ctx.heap());
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
        return match crate::interp::exec_function(ctx, module, func, &[])? {
            crate::interp::ExecOutcome::Returned(_) => Ok(()),
            crate::interp::ExecOutcome::Thrown(val) =>
                Err(anyhow::anyhow!("{}", crate::exception::format_uncaught(&val, ctx, module))),
        };
    };
    // One `VmFrame` push enrolling the entry frame's regs (GC roots)
    // + function (trace); inner calls go through the same `invoke::call_native`.
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

// ─── Public entry point called from vm.rs ───────────────────────────────────

/// Called by `Vm::run` when the execution mode is JIT.
///
/// Installs the VM's JIT ([`runtime::install`] — one per VM, shared by every
/// thread; reused when already installed) without compiling any user function;
/// each function is compiled on its first call (see
/// [`runtime::JitRuntime::resolve_fn_by_id`]). The counters therefore report what
/// was **actually** compiled:
/// - `jit_methods_compiled` is incremented per compiled function (not the module's
///   total function count) — on the shard of the thread that compiled it;
/// - `jit_compile_us_total` accumulates each lazy compile's duration — likewise.
///
/// The one-per-run `JitModuleCompiled` event still fires here, reporting the
/// module **size** (`functions().len()`) and the **setup** time (not a
/// whole-module compile time). Per-function compile events are deferred to a
/// future observability spec (avoids one event per function).
pub fn run(ctx: &VmContext, module: &Module, entry_name: &str) -> Result<()> {
    // E1.P2 Phase 1 (2026-06-02): metadata reads routed through `JitVm`.
    let function_count = module.functions().len() as u32;
    let start = std::time::Instant::now();

    runtime::install(ctx, module)?;

    let setup_us = start.elapsed().as_micros() as u64;
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::JitModuleCompiled {
        module_name:    module.module_name().to_string(),
        function_count,
        duration_us:    setup_us,
    });

    run_with_static_init(ctx, entry_name)
}
