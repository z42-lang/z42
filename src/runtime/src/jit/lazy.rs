//! Lazy per-function JIT compilation (lazy-per-function-jit, 2026-07-23).
//!
//! Holds the cranelift `JITModule` plus helper import ids, and compiles a single
//! z42 function to native code on demand. A `LazyCompiler` is owned (behind a
//! `Mutex`) by the `JitModule`; `JitModuleCtx.lazy` points at that mutex.
//! `JitModuleCtx::resolve_fn_by_id` calls [`LazyCompiler::compile_one`] under the
//! lock, with a `OnceLock` double-check so each function compiles exactly once
//! even under concurrent first-calls.
//!
//! Replaces the former eager whole-module compile (`compile_module` translated
//! every function at load); a short-lived program now compiles only the handful
//! of functions it actually calls instead of the entire merged stdlib closure.

use anyhow::Result;
use cranelift_codegen::ir::{types, AbiParam};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module as CraneliftModule};

use super::frame::FnEntry;
use super::helpers::{self, HelperIds};
use super::translate;
use crate::metadata::{bytecode::Function, Module};

/// Mutable JIT compilation state: compiles z42 functions to native code lazily.
pub struct LazyCompiler {
    jit:        JITModule,
    helper_ids: HelperIds,
    /// Back-pointer to the bytecode module (function bodies to translate).
    /// SAFETY: the Module outlives the `JitModule` that owns this compiler.
    module:     *const Module,
    /// `Z42_JIT_PROFILE` set → print one line per lazily-compiled function.
    /// Read once at setup so `compile_one` avoids a per-call env lookup; the
    /// line count is the "compiled N functions" tally the design cites.
    profile:    bool,
}

// SAFETY: the `*const Module` is read-only; the `JITModule` (which is not `Sync`)
// is only ever touched while the wrapping `Mutex<LazyCompiler>` is held, so
// access is serialized. See design.md Decision 5.
unsafe impl Send for LazyCompiler {}

impl LazyCompiler {
    /// Build the JIT infrastructure (JITModule + helper symbols) **without**
    /// translating any user function. The caller pre-sizes `fn_entries_by_id`
    /// to `module.functions.len()`; functions are translated later, on first
    /// call, by [`compile_one`](Self::compile_one).
    pub fn setup(module: &Module) -> Result<Self> {
        // NB(perf-vm-iteration Phase 4)：实测 `opt_level=speed` 对本 VM 无收益——
        // 紧循环/派发的成本在 opaque helper call + 每 op 24B Value load/store，
        // Cranelift 无法跨 op 去箱来消除,故 speed 档零计算提升却多 ~4-5ms 冷编译
        // （arith compute 50.4→52.9ms、poly 1024→1028ms flat、startup 54→59ms）。
        // 真正的 JIT 杠杆是结构性去箱 + 内联 helper,不是让 Cranelift 更用力优化
        // 现有形状。故保留默认档。
        // （原引的 bench/results/MODE-COMPARISON.md 是 perf-vm-iteration 时期
        // compare-modes.sh 的产物，早已不在仓库；该脚本本身也随 move-bench-into-tests
        // 删除——interp/jit 对比现在是 `xtask bench --mode both`。上面括号里的
        // 数字就是当时的结论，无需再去找那份文件。）
        let isa = cranelift_native::builder()
            .map_err(|e| anyhow::anyhow!("native ISA unavailable: {}", e))?
            .finish(cranelift_codegen::settings::Flags::new(
                cranelift_codegen::settings::builder(),
            ))?;
        let mut jit_builder =
            JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        // Single source of truth for the helper set lives in `helpers::registry`.
        helpers::register_symbols(&mut jit_builder);
        let mut jit = JITModule::new(jit_builder);
        let helper_ids = helpers::declare_imports(&mut jit)?;
        Ok(LazyCompiler {
            jit,
            helper_ids,
            module: module as *const Module,
            // `Z42_JIT_PROFILE` now flows through the central RuntimeConfig
            // (de-straggler) so it appears in `--info` and the [runtime] layer.
            profile: crate::config::runtime_config().jit_profile,
        })
    }

    /// Compile `module.functions[idx]` to native code and return its `FnEntry`.
    ///
    /// z42 `Call`s route through the `jit_call` / `jit_vcall` runtime helpers
    /// (never a direct cranelift call to a sibling z42 function — see
    /// translate.rs), so a function is self-contained: it imports only the
    /// `hr_*` helper symbols. That is what lets us declare + define + finalize
    /// each function independently, on demand.
    ///
    /// Caller (`resolve_fn_by_id`) holds the mutex and has already verified the
    /// function is JIT-translatable and its slot is empty.
    pub fn compile_one(&mut self, idx: usize) -> Result<FnEntry> {
        // SAFETY: module outlives this compiler (see field docs). The `&Function`
        // comes from a raw-pointer deref (not a borrow of `self`), so passing it to
        // `compile_fn(&mut self, ...)` is sound.
        let module = unsafe { &*self.module };
        let func = module.functions.get(idx)
            .ok_or_else(|| anyhow::anyhow!("lazy JIT: function index {} out of range", idx))?;
        self.compile_fn(func)
    }

    /// Compile an arbitrary `&Function` to native code. Used for functions in the
    /// merged module (`compile_one`) and — make-vm-loading-lazy — for functions
    /// materialized by the lazy loader (`resolve_fn_by_id` compiles a not-yet-merged
    /// stdlib function here instead of falling back to the interpreter).
    pub fn compile_fn(&mut self, func: &Function) -> Result<FnEntry> {
        let ptr = self.jit.target_config().pointer_type();
        let mut sig = self.jit.make_signature();
        sig.params.push(AbiParam::new(ptr));        // frame *mut JitFrame
        sig.params.push(AbiParam::new(ptr));        // ctx   *const JitModuleCtx
        sig.returns.push(AbiParam::new(types::I8)); // 0 = ok, 1 = exception
        let func_id = self.jit.declare_function(&func.name, Linkage::Local, &sig)?;

        if self.profile {
            eprintln!("[JIT PROFILE] lazy-compile {}", func.name);
        }

        let max_r = translate::max_reg(func);
        translate::translate_function(&mut self.jit, &self.helper_ids, func, max_r, func_id, None)?;
        // Finalize just this function's definition (relocations + mprotect).
        // Earlier finalized functions keep their code pages — cranelift-jit
        // allocates each function separately, so their pointers stay valid.
        self.jit.finalize_definitions()?;

        let ptr_raw = self.jit.get_finalized_function(func_id);
        // Precompute name + file Arcs so jit_call / jit_vcall can push FrameInfo
        // without a reverse lookup (mirrors the former eager path).
        let file_str: std::sync::Arc<str> = func.line_table().first()
            .and_then(|e| e.file.as_deref())
            .unwrap_or("")
            .into();
        let frame_name: std::sync::Arc<str> =
            std::sync::Arc::from(crate::metadata::bytecode::format_frame_name(func).as_str());
        Ok(FnEntry {
            ptr:     ptr_raw as *const u8,
            max_reg: max_r,
            name:    frame_name,
            file:    file_str,
        })
    }

    /// add-osr-loop-tiering: compile an **OSR variant** of `func` whose entry runs
    /// the prologue and then jumps to z42 block `k` (the hot loop header). Declared
    /// under a distinct linkage name (`<name>$osr<k>`) so it coexists with the normal
    /// entry. Called at most once per (function, header) — the interpreter hands the
    /// running activation over to the returned native code (register state inherited
    /// via `frame.regs` memory). Structurally mirrors `compile_fn`.
    pub fn compile_fn_osr(&mut self, func: &Function, k: usize) -> Result<FnEntry> {
        let ptr = self.jit.target_config().pointer_type();
        let mut sig = self.jit.make_signature();
        sig.params.push(AbiParam::new(ptr));        // frame *mut JitFrame
        sig.params.push(AbiParam::new(ptr));        // ctx   *const JitModuleCtx
        sig.returns.push(AbiParam::new(types::I8)); // 0 = ok, 1 = exception
        let osr_name = format!("{}$osr{}", func.name, k);
        let func_id = self.jit.declare_function(&osr_name, Linkage::Local, &sig)?;

        if self.profile {
            eprintln!("[JIT PROFILE] osr-compile {} @block{}", func.name, k);
        }

        let max_r = translate::max_reg(func);
        translate::translate_function(&mut self.jit, &self.helper_ids, func, max_r, func_id, Some(k))?;
        self.jit.finalize_definitions()?;

        let ptr_raw = self.jit.get_finalized_function(func_id);
        let file_str: std::sync::Arc<str> = func.line_table().first()
            .and_then(|e| e.file.as_deref())
            .unwrap_or("")
            .into();
        let frame_name: std::sync::Arc<str> =
            std::sync::Arc::from(crate::metadata::bytecode::format_frame_name(func).as_str());
        Ok(FnEntry {
            ptr:     ptr_raw as *const u8,
            max_reg: max_r,
            name:    frame_name,
            file:    file_str,
        })
    }
}

#[cfg(test)]
#[path = "lazy_tests.rs"]
mod lazy_tests;
