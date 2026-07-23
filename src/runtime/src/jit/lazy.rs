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
use crate::metadata::Module;

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
            profile: std::env::var("Z42_JIT_PROFILE").is_ok(),
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
        // SAFETY: module outlives this compiler (see field docs).
        let module = unsafe { &*self.module };
        let func = module.functions.get(idx)
            .ok_or_else(|| anyhow::anyhow!("lazy JIT: function index {} out of range", idx))?;

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
        translate::translate_function(&mut self.jit, &self.helper_ids, func, max_r, func_id)?;
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
}

#[cfg(test)]
#[path = "lazy_tests.rs"]
mod lazy_tests;
