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

mod frame;
pub(crate) mod helpers;
/// Lazy per-function compilation state (lazy-per-function-jit, 2026-07-23).
mod lazy;
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
use frame::{JitFrame, JitModuleCtx};
use lazy::LazyCompiler;
use helpers::{take_exception_error, JitFn};
use std::sync::Mutex;

// ─── Public API ─────────────────────────────────────────────────────────────

/// A z42 module wired for native execution. lazy-per-function-jit (2026-07-23):
/// functions are compiled **on first call**, not eagerly at load — `setup` only
/// builds the JIT infrastructure; `LazyCompiler::compile_one` fills each slot on
/// demand via `JitModuleCtx::resolve_fn_by_id`.
pub struct JitModule {
    /// Mutex-guarded lazy compiler; owns the cranelift `JITModule` so the
    /// machine-code pages stay valid for the whole run. Read only through the
    /// raw pointer stashed in `ctx.lazy` (heap-stable via `Box`, so the pointer
    /// never dangles) — the field itself just keeps it alive, hence `_lazy`.
    _lazy: Box<Mutex<LazyCompiler>>,
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
        // per-call alloc) + tier-up threshold from `Z42_JIT_THRESHOLD` (default 2,
        // clamped ≥ 1; N=1 = compile-on-first-call = pre-tiering behavior).
        let mut call_counts = Vec::with_capacity(n);
        call_counts.resize_with(n, std::sync::atomic::AtomicU32::default);
        let jit_threshold = std::env::var("Z42_JIT_THRESHOLD").ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(2)
            .max(1);
        let ctx = Box::new(JitModuleCtx {
            // review.md C3 Phase 1 (2026-06-03): copy the pre-interned Arc<str>
            // pool (cheap — Arc::clone per slot) so `jit_const_str` avoids the
            // prior two-alloc path.
            string_pool: module.interned_strings.clone(),
            fn_entries_by_id,
            module: module as *const Module,
            lazy: &*lazy_box as *const Mutex<LazyCompiler>,
            // make-vm-loading-lazy: functions with id < merged_len live in the
            // pre-sized `fn_entries_by_id`; ids ≥ merged_len are synthetic slots
            // for lazily-loaded (not-yet-merged) functions in `lazy_table`.
            merged_len: n,
            lazy_table: Mutex::new(frame::LazyTable::default()),
            // Set by JitModule::run for the duration of an entry call; null
            // outside that window.
            vm_ctx: std::ptr::null_mut(),
            call_counts,
            jit_threshold,
        });
        Ok(JitModule { _lazy: lazy_box, ctx })
    }

    /// Run a specific entry function by name (no static-init).
    ///
    /// `ctx` is the canonical state holder; we wire its raw pointer into
    /// `JitModuleCtx.vm_ctx` for the duration of this call so JIT helpers
    /// (which receive `*const JitModuleCtx`) can reach VmContext through it.
    pub fn run_fn(&mut self, ctx: &VmContext, entry_name: &str) -> Result<()> {
        // Cast `&VmContext` (immutable ref) to a `*mut VmContext` for the
        // JIT ABI. The JIT extern-C bridge expects a `*mut` pointer for
        // historical compatibility (the helper functions reach VmContext
        // through `(*jit_ctx).vm_ctx`), but they only ever call `&self`
        // methods on it. add-vmcontext-registry (2026-05-20) converted
        // the caller signature to `&VmContext`, so the cast goes via
        // `*const _` first to satisfy the strict pointer-cast rules.
        // lazy-per-function-jit (2026-07-23): wire this BEFORE resolving the
        // entry so the entry's own lazy compile is counted (resolve reaches the
        // counters through `vm_ctx`).
        self.ctx.vm_ctx = (ctx as *const VmContext) as *mut VmContext;
        // Resolve (and lazily compile on first call) the entry function.
        // SAFETY: module/lazy valid for the JitModule's lifetime.
        let entry = match unsafe { self.ctx.resolve_fn_by_name(entry_name) } {
            Some(e) => e.clone(),
            None => {
                // Not JIT-translatable (contains an interp-only opcode such as
                // `LoadLocalAddr`) or absent from the module. Run it on the
                // interpreter instead of hard-failing — covers a skipped
                // entry-point or `__static_init__`. The interpreter never
                // re-enters JIT code, so the whole call subtree runs
                // interpreted. SAFETY: `module` outlives the JitModule.
                self.ctx.vm_ctx = std::ptr::null_mut();
                let module = unsafe { &*self.ctx.module };
                // make-vm-loading-lazy: the entry may be an untranslatable
                // function in a lazily-loaded zpkg (e.g. a dep's
                // `__static_init__`) that is NOT in the merged module — resolve
                // it through the lazy loader, mirroring interp's path.
                if let Some(func) = module.func_index.get(entry_name)
                    .and_then(|&idx| module.functions.get(idx))
                {
                    return match crate::interp::exec_function(ctx, module, func, &[])? {
                        crate::interp::ExecOutcome::Returned(_) => Ok(()),
                        crate::interp::ExecOutcome::Thrown(val) =>
                            Err(anyhow::anyhow!("{}", crate::exception::format_uncaught(&val, module))),
                    };
                }
                let func = ctx.try_lookup_function(entry_name)
                    .ok_or_else(|| anyhow::anyhow!("JIT: entry `{}` not found", entry_name))?;
                return match crate::interp::exec_function(ctx, module, func.as_ref(), &[])? {
                    crate::interp::ExecOutcome::Returned(_) => Ok(()),
                    crate::interp::ExecOutcome::Thrown(val) =>
                        Err(anyhow::anyhow!("{}", crate::exception::format_uncaught(&val, module))),
                };
            }
        };
        let mut frame = JitFrame::new(entry.max_reg, &[]);
        let f: JitFn = unsafe { std::mem::transmute(entry.ptr) };
        // 2026-05-10 unify-frame-chain: single push enrolling this entry
        // frame's regs / env_arena (GC roots) + name / file (trace) in
        // one VmFrame. Inner JIT calls are wrapped by jit_call / jit_vcall
        // / jit_call_indirect / jit_obj_new / jit_to_str on the same
        // unified API.
        ctx.push_frame(crate::exception::VmFrame::new(
            entry.name.clone(),
            entry.file.clone(),
            &frame.regs as *const _,
            &frame.env_arena as *const _,
        ));
        let r = unsafe { f(&mut frame, &*self.ctx) };
        ctx.pop_frame();
        frame.recycle();
        self.ctx.vm_ctx = std::ptr::null_mut();
        if r != 0 {
            // SAFETY: ctx.module set in compile_module from a &Module that
            // outlives the JitModule (caller-owned). Deref is safe here.
            let module = unsafe { &*self.ctx.module };
            return Err(take_exception_error(ctx, module));
        }
        Ok(())
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
            self.run_fn(ctx, init_name)?;
        }

        // runtime-jit-tiering Phase 1: the entry (and static-inits) run via
        // `run_fn` → `resolve_fn_by_id` (non-tiered) → compile-on-first-call, so no
        // threshold exemption is needed. Only `jit_call` (static/free calls from
        // within JIT'd code) applies the tier-up threshold.
        self.run_fn(ctx, entry_name)
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

    let setup_us = start.elapsed().as_micros() as u64;
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::JitModuleCompiled {
        module_name:    module.module_name().to_string(),
        function_count,
        duration_us:    setup_us,
    });

    jit_module.run(ctx, entry_name)
}
