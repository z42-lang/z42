/// JIT backend — compiles z42 bytecode to native machine code using Cranelift.
///
/// Architecture
/// ------------
/// * `frame.rs`     — JitFrame (register file + var slots) and JitModuleCtx
/// * `helpers/`     — `extern "C"` helper functions called by JIT code, split
///                    by `Instruction` category and registered through
///                    `helpers::registry`. See `helpers/mod.rs` for the list.
/// * `translate.rs` — Cranelift IR translation
/// * `mod.rs`       — top-level compile_module / JitModule::run

pub(crate) mod frame; // runtime-jit-tiering Phase 1.5: interp dispatch reaches JitFrame/JitModuleCtx
pub(crate) mod helpers;
/// Lazy per-function compilation state (lazy-per-function-jit, 2026-07-23).
mod lazy;
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

/// parallel-worker-jit (2026-09-01): concurrency-safety stress tests — N threads
/// sharing one `JitShared` (compile-once, run-N), the `--jobs N` worker path.
#[cfg(test)]
#[path = "parallel_tests.rs"]
mod parallel_tests;

use crate::metadata::Module;
use vm_interface::JitVm;
use anyhow::Result;
use crate::vm_context::VmContext;
use frame::{JitFrame, JitModuleCtx, JitShared};
use lazy::LazyCompiler;
use helpers::JitFn;
use std::sync::{Arc, Mutex};

// ─── Public API ─────────────────────────────────────────────────────────────

/// A z42 module wired for native execution. lazy-per-function-jit (2026-07-23):
/// functions are compiled **on first call**, not eagerly at load — `setup` only
/// builds the JIT infrastructure; `LazyCompiler::compile_one` fills each slot on
/// demand via `JitModuleCtx::resolve_fn_by_id`.
pub struct JitModule {
    /// Entry thread's per-thread dispatch shell. Its `shared: Arc<JitShared>`
    /// owns the cranelift `JITModule` (via `JitShared.lazy`) + the compiled-code
    /// table, so the machine-code pages stay valid as long as any `Arc<JitShared>`
    /// lives. parallel-worker-jit (2026-09-01): the same `Arc` is also published in
    /// `VmCore.jit_shared` so `--jobs N` workers share it (compile-once, run-N).
    ctx:   Box<JitModuleCtx>,
    // 2026-04-27 fix-static-field-access: removed `name: String` —
    // 之前用来 format `"{name}.__static_init__"`，新版扫描所有
    // `*.__static_init__` 函数，不再需要主模块名。
}

impl JitModule {
    /// Build the JIT infrastructure for `module` without compiling any user
    /// function (compile-on-first-call). Pre-sizes the per-function slot table
    /// to `module.functions.len()` and wires `ctx.lazy` at the owned mutex.
    pub fn setup(module: &Module) -> Result<Self> {
        let lazy_box: Box<Mutex<LazyCompiler>> = Box::new(Mutex::new(LazyCompiler::setup(module)?));
        let n = module.functions().len();
        let mut fn_entries_by_id = Vec::with_capacity(n);
        fn_entries_by_id.resize_with(n, std::sync::OnceLock::new);
        // runtime-jit-tiering Phase 1: per-function call counters (pre-sized, zero
        // per-call alloc) + tier-up threshold from `Z42_JIT_THRESHOLD` (default
        // 1, clamped ≥ 1; N=1 = compile-on-first-call).
        //
        // lower-jit-threshold-default (2026-08-31): default was 1000 (compile only
        // genuinely hot functions). That is right for hot-LOOP workloads — but OSR
        // (`osr_threshold`) already upgrades hot loops independently, and a large class
        // of real programs (the z42c self-compiler above all) spends its time in
        // functions each called only a HANDFUL of times (`_build` / `PackageCompile`
        // / `BuildPackageCus` / the whole codegen+serialize pipeline run ONCE per
        // build). At 1000 those never compiled → the compiler ran fully interpreted
        // (only ~18 leaf string utils crossed 1000 calls). Profiled: z42c.semantics
        // full build 34.7s→29.0s (~17%) at N=1, byte-identical, with <1% of samples
        // in Cranelift (compile overhead negligible — the cold tail is small and JIT
        // is lazy/per-function, so only REACHED functions ever compile). N≥2 cannot
        // help once-called functions at all, so 1 is the only value that captures them.
        let mut call_counts = Vec::with_capacity(n);
        call_counts.resize_with(n, std::sync::atomic::AtomicU32::default);
        let jit_threshold = std::env::var("Z42_JIT_THRESHOLD").ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1)
            .max(1);
        // add-osr-loop-tiering: back-edge count that triggers OSR of the running
        // interp activation. Default 10_000 — high enough that short loops finish in
        // the interpreter before paying a compile, low enough that a genuinely hot
        // loop (millions of iterations) upgrades within its first fraction of a %.
        let osr_threshold = std::env::var("Z42_OSR_THRESHOLD").ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(10_000)
            .max(1);
        // parallel-worker-jit (2026-09-01): the compile-once, thread-invariant
        // state lives in `JitShared` (behind an `Arc`); the per-thread shell adds
        // only `vm_ctx`. `JitShared` OWNS the `LazyCompiler` (was a raw back-pointer
        // kept alive by `JitModule._lazy`) so the code pages live as long as any
        // `Arc<JitShared>` — including the clone published in `VmCore.jit_shared`.
        let shared = Arc::new(JitShared {
            fn_entries_by_id,
            module: module as *const Module,
            lazy: lazy_box,
            // make-vm-loading-lazy: functions with id < merged_len live in the
            // pre-sized `fn_entries_by_id`; ids ≥ merged_len are synthetic slots
            // for lazily-loaded (not-yet-merged) functions in `lazy_table`.
            merged_len: n,
            lazy_table: Mutex::new(frame::LazyTable::default()),
            call_counts,
            jit_threshold,
            osr_entries: Mutex::new(std::collections::HashMap::new()),
            osr_threshold,
        });
        let ctx = Box::new(JitModuleCtx {
            shared,
            // Set by JitModule::run_fn (entry) / run_spawned_action (worker) for the
            // duration of one native execution; null outside that window.
            vm_ctx: std::ptr::null_mut(),
        });
        Ok(JitModule { ctx })
    }

    /// Run a specific entry function by name (no static-init).
    ///
    /// `ctx` is the canonical state holder; we wire its raw pointer into
    /// `JitModuleCtx.vm_ctx` for the duration of this call so JIT helpers
    /// (which receive `*const JitModuleCtx`) can reach VmContext through it.
    pub fn run_fn(&mut self, ctx: &VmContext, entry_name: &str) -> Result<()> {
        // parallel-worker-jit (2026-09-01): the entry runs on this JitModule's own
        // shell (`self.ctx`) with no args. The body is shared with `--jobs N`
        // workers via the free `run_fn_on_shell` (each worker builds its own shell
        // over the same `Arc<JitShared>`). An uncaught top-level exception is
        // formatted here (entry semantics), unchanged from before the refactor.
        match run_fn_on_shell(&mut self.ctx, ctx, entry_name, &[])? {
            crate::interp::ExecOutcome::Returned(_) => Ok(()),
            crate::interp::ExecOutcome::Thrown(val) => {
                let module = unsafe { &*self.ctx.shared.module };
                Err(anyhow::anyhow!("{}", crate::exception::format_uncaught(&val, module)))
            }
        }
    }

    /// runtime-jit-tiering Phase 1c: run a `__static_init__` on the interpreter
    /// instead of compiling it. Static initialisers execute exactly once, so a
    /// cranelift compile + native code page is pure overhead. Resolves the function
    /// (through the lazy loader for dep zpkgs, mirroring `run_fn`'s interp fallback)
    /// and runs it via `interp::exec_function`, whose tiered central divert keeps a
    /// cold one-shot on the interpreter (count 1 < threshold) while still routing any
    /// *already-compiled* callee it reaches to native. The JIT ctx forward pointer
    /// stays published for the duration so that divert can fire; static fields land
    /// in the shared `VmContext`, identical to the native path. Cleared in lockstep
    /// with `vm_ctx` on every exit path.
    fn run_static_init_interp(&mut self, ctx: &VmContext, name: &str) -> Result<()> {
        // SAFETY: module/self.ctx valid for the JitModule's lifetime; the raw
        // pointers published here are cleared before returning.
        self.ctx.vm_ctx = (ctx as *const VmContext) as *mut VmContext;
        ctx.set_jit_ctx(&*self.ctx as *const JitModuleCtx as usize);
        let module = unsafe { &*self.ctx.module };
        let outcome = if let Some(func) = module.func_index.get(name)
            .and_then(|&idx| module.functions.get(idx))
        {
            crate::interp::exec_function(ctx, module, func, &[])
        } else if let Some(func) = ctx.try_lookup_function(name) {
            // Lazily-loaded dep zpkg init not present in the merged module.
            crate::interp::exec_function(ctx, module, func.as_ref(), &[])
        } else {
            // Name came from enumerating inits, so this should be unreachable;
            // skip defensively rather than hard-fail.
            self.ctx.vm_ctx = std::ptr::null_mut();
            ctx.set_jit_ctx(0);
            return Ok(());
        };
        self.ctx.vm_ctx = std::ptr::null_mut();
        ctx.set_jit_ctx(0);
        match outcome? {
            crate::interp::ExecOutcome::Returned(_) => Ok(()),
            crate::interp::ExecOutcome::Thrown(val) =>
                Err(anyhow::anyhow!("{}", crate::exception::format_uncaught(&val, module))),
        }
    }

    /// Run with static initialisation: clears static fields, calls **all**
    /// `*.__static_init__` functions (sorted) — including imported zpkgs —
    /// then calls the given entry function.
    ///
    /// 2026-04-27 fix-static-field-access: 与 interp 的 `run_with_static_init`
    /// 对称修复。修前只跑主模块 init，导入 zpkg（如 z42.math 的
    /// `Std.Math.__static_init__`）虽然 link 但永不被调用。
    pub fn run(&mut self, ctx: &VmContext, entry_name: &str) -> Result<()> {
        ctx.static_fields_clear();

        // Collect all __static_init__ entries; sort by name for determinism.
        // fix-jit-cross-zpkg-transitive-eager (2026-06-20): enumerate from the
        // merged module (not `fn_entries`) so a `__static_init__` that was
        // skipped by `compile_module` (interp-only opcode) is still run — via
        // `run_fn`'s interp fallback. Matches interp's `init_static_fields`,
        // which also scans `module.functions`. SAFETY: see `run_fn`.
        //
        // make-vm-loading-lazy: dep zpkgs are no longer eagerly merged, so their
        // `__static_init__` functions are reachable only via the lazy loader. We
        // gather BOTH sources, then run them in ONE globally-sorted order:
        //   • eager: inits in the merged module (main + z42.core);
        //   • lazy:  inits in every declared-but-unloaded zpkg (force-loaded).
        //
        // The union MUST be sorted together (not eager-then-lazy), to reproduce
        // the pre-lazy JIT order exactly: the old eager-BFS merged every dep into
        // one `module.functions` and sorted the whole set. A two-phase order
        // (all eager before all lazy) runs a dep's init AFTER main's — breaking
        // any main-side init that reads a static field a dep init sets (observed:
        // xtask crashes `I64(0) vs Null` on the first cross-package static read).
        // `run_fn` compiles each to native (or interp-fallback if untranslatable).
        let init_names: Vec<String> = {
            let module = unsafe { &*self.ctx.module };
            let mut v: Vec<String> = module.functions.iter()
                .map(|f| f.name.clone())
                .filter(|n| n.ends_with(".__static_init__"))
                .collect();
            // Lazy inits force-load every declared zpkg; the returned names are
            // disjoint from the eager set (declared excludes initially-loaded).
            v.extend(ctx.collect_lazy_static_init_names());
            v.sort();
            v.dedup();
            v
        };
        for init_name in &init_names {
            // runtime-jit-tiering Phase 1c: `__static_init__` functions run EXACTLY
            // once (class initialization). Compiling a one-shot function is pure
            // overhead — a cranelift compile (~100µs) + a native code page paid to
            // run the body a single time, where the interpreter would have run it
            // outright. Measured: ~73% of all compiled functions on a typical
            // startup were `*.__static_init__`. Run them on the interpreter (via
            // `exec_function`, whose tiered central divert routes any *already*-hot
            // callee to native but keeps this cold one-shot on the interpreter);
            // static fields land in the shared `VmContext`, identical to native.
            // NB: this must go through the *tiered* path — `run_fn` uses the
            // non-tiered `resolve_fn_by_name`, which would compile the init anyway.
            // The entry (below) stays compiled — it may carry the program's hot loop.
            self.run_static_init_interp(ctx, init_name)?;
        }

        // runtime-jit-tiering Phase 1: the entry runs via `run_fn` →
        // `resolve_fn_by_id` (non-tiered) → compile-on-first-call. Only tiered call
        // sites (`jit_call`/vcall/closure/ctor + the interp central divert) apply
        // the tier-up threshold.
        self.run_fn(ctx, entry_name)
    }
}

/// Run function `entry_name` (with `args`) on a given per-thread JIT `shell`.
///
/// parallel-worker-jit (2026-09-01): factored out of `JitModule::run_fn` so the
/// entry thread and every `--jobs N` worker share ONE native-execution path. Each
/// caller supplies its OWN shell (`JitModuleCtx { shared, vm_ctx }`) over the same
/// `Arc<JitShared>`, so functions compile once and every thread runs the same code.
///
/// Publishes this thread's `vm_ctx` + the mixed-mode `jit_ctx` forward pointer on
/// `ctx` for the duration (so JIT helpers and any interp the run re-enters reach the
/// right VmContext + can route already-compiled callees to native), resolves +
/// compiles-on-first-call the target (interp fallback when untranslatable/absent —
/// same as the eager entry path), runs the native code, then clears the published
/// pointers in lockstep. `args` is empty for the entry, the captured-env array for a
/// worker's action.
///
/// Returns the raw [`ExecOutcome`](crate::interp::ExecOutcome) so each caller formats
/// a thrown exception its own way — the entry wraps it with `format_uncaught` (uncaught
/// top-level), a worker reads `Exception.Message` for `Std.ThreadException`. Native
/// success returns `Returned(Null)` (the value is unused by both callers).
///
/// SAFETY: `shell.shared.module` outlives this call (owned by `VmCore.module`); the
/// shell is used by only THIS thread (no cross-thread `vm_ctx` aliasing).
pub(crate) fn run_fn_on_shell(
    shell: &mut JitModuleCtx,
    ctx: &VmContext,
    entry_name: &str,
    args: &[crate::metadata::Value],
) -> Result<crate::interp::ExecOutcome> {
    use crate::interp::ExecOutcome;
    use crate::metadata::Value;
    // unify-gc-heap PR-4 (D11): scope the heap as the ambient GC heap for this run so
    // heap-less allocation sites inside JIT'd code (and any interp it re-enters) work.
    let _heap_guard = crate::gc::ambient::HeapGuard::enter(ctx.heap());
    // Wire vm_ctx BEFORE resolving so the entry's own lazy compile is counted, then
    // publish the JitModuleCtx forward pointer (type-erased) for mixed-mode routing.
    // Both are cleared in lockstep on every exit path (native code reaches vm_ctx
    // through `(*jit_ctx).vm_ctx`).
    shell.vm_ctx = (ctx as *const VmContext) as *mut VmContext;
    let shell_ptr = &*shell as *const JitModuleCtx;
    ctx.set_jit_ctx(shell_ptr as usize);
    // Resolve (and lazily compile on first call) the target function.
    let entry = match unsafe { shell.resolve_fn_by_name(entry_name) } {
        Some(e) => e.clone(),
        None => {
            // Not JIT-translatable (interp-only opcode) or absent from the merged
            // module. Run it on the interpreter instead of hard-failing — the interp
            // never re-enters JIT code, so the whole subtree runs interpreted.
            shell.vm_ctx = std::ptr::null_mut();
            ctx.set_jit_ctx(0); // keep jit_ctx in lockstep with vm_ctx
            let module = unsafe { &*shell.shared.module };
            // make-vm-loading-lazy: the target may be an untranslatable function in a
            // lazily-loaded zpkg not in the merged module — resolve via the lazy loader.
            if let Some(func) = module.func_index.get(entry_name)
                .and_then(|&idx| module.functions.get(idx))
            {
                return Ok(crate::interp::exec_function(ctx, module, func, args)?);
            }
            let func = ctx.try_lookup_function(entry_name)
                .ok_or_else(|| anyhow::anyhow!("JIT: entry `{}` not found", entry_name))?;
            return Ok(crate::interp::exec_function(ctx, module, func.as_ref(), args)?);
        }
    };
    let mut frame = JitFrame::new(entry.max_reg, args);
    let f: JitFn = unsafe { std::mem::transmute(entry.ptr) };
    // 2026-05-10 unify-frame-chain: single push enrolling this frame's regs /
    // env_arena (GC roots) + name / file (trace) in one VmFrame.
    ctx.push_frame(crate::exception::VmFrame::new(
        entry.name.clone(),
        entry.file.clone(),
        &frame.regs as *const _,
        &frame.env_arena as *const _,
    ));
    let r = unsafe { f(&mut frame, shell_ptr) };
    ctx.pop_frame();
    frame.recycle();
    shell.vm_ctx = std::ptr::null_mut();
    ctx.set_jit_ctx(0); // keep jit_ctx in lockstep with vm_ctx
    if r != 0 {
        // Native code raised: the thrown value is pending on this thread's VmContext.
        Ok(ExecOutcome::Thrown(ctx.take_exception().unwrap_or(Value::Null)))
    } else {
        // The entry / worker action's return value is unused by both callers.
        Ok(ExecOutcome::Returned(None))
    }
}

// ─── Public entry point called from vm.rs ───────────────────────────────────

/// Called by `Vm::run` when the execution mode is JIT.
///
/// lazy-per-function-jit (2026-07-23): `setup` builds the JIT infrastructure
/// without compiling any user function; each function is compiled on its first
/// call (see [`JitModuleCtx::resolve_fn_by_id`]). The counters therefore report
/// what was **actually** compiled:
/// - `jit_methods_compiled` is incremented per `compile_one` (not the module's
///   total function count) — see `resolve_fn_by_id`.
/// - `jit_compile_us_total` accumulates each lazy compile's duration — likewise.
///
/// The one-per-module `JitModuleCompiled` event still fires here, now reporting
/// the module **size** (`functions().len()`) and the **setup** time (not a
/// whole-module compile time). Per-function compile events are deferred to a
/// future observability spec (avoids one event per function).
pub fn run(ctx: &VmContext, module: &Module, entry_name: &str) -> Result<()> {
    // E1.P2 Phase 1 (2026-06-02): metadata reads routed through `JitVm`.
    let function_count = module.functions().len() as u32;
    let start = std::time::Instant::now();

    let mut jit_module = JitModule::setup(module)?;

    // parallel-worker-jit (2026-09-01): publish the shared compiled-code table so
    // `--jobs N` worker threads (spawned during the entry's execution below) run
    // their action on the SAME JIT native code (compile-once, run-N) instead of the
    // interpreter. Set-once; a second JIT entry on the same VmCore reuses it.
    let _ = ctx.core.jit_shared.set(Arc::clone(&jit_module.ctx.shared));

    let setup_us = start.elapsed().as_micros() as u64;
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::JitModuleCompiled {
        module_name:    module.module_name().to_string(),
        function_count,
        duration_us:    setup_us,
    });

    jit_module.run(ctx, entry_name)
}
