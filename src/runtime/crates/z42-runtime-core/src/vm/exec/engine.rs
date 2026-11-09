//! Per-VM backend contracts. Compilers, code pages and backend frames stay in engines.
use std::any::Any;
use std::sync::OnceLock;
use anyhow::{bail, Result};
use crate::zpkg::ExecMode;
use crate::object::Value;
use crate::vm::runtime::{Function, Module};
use crate::vm::context::VmContext;
use super::call::{args::CallArgs, outcome::ExecOutcome};

pub trait Engine: Send + Sync {
    fn invoke(&self, ctx: &VmContext, module: &Module, func: &Function,
        args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome>;
    fn run(&self, ctx: &VmContext, module: &Module, entry: &Function) -> Result<()>;
}

/// A call-site resolve counts toward tier-up; an existing-function divert uses
/// the backend's configured peek policy. Keeping these distinct prevents double counting.
pub enum TierTarget<'a> { Site(usize), Existing(&'a Function) }

pub struct OsrState<'a> {
    pub regs: &'a [Value],
    pub method_type_args: &'a [String],
    pub frame_id: u32,
    pub has_ref_writebacks: bool,
}

pub trait TierBackend: Any + Send + Sync {
    fn osr_threshold(&self) -> u32;
    fn invoke(&self, ctx: &VmContext, target: TierTarget<'_>, args: CallArgs<'_>, method_type_args: &[String])
        -> Option<Result<ExecOutcome>>;
    fn osr(&self, ctx: &VmContext, func: &Function, header: usize, state: OsrState<'_>)
        -> Option<Result<ExecOutcome>>;
}

/// The stable, neutral header cached by contexts. The boxed backend owns its code;
/// the VM keeps this header alive until every context sharing its core has dropped.
pub struct TierApi {
    pub osr_threshold: u32,
    backend: Box<dyn TierBackend>,
}

impl TierApi {
    pub fn new<T: TierBackend>(backend: T) -> Self {
        Self { osr_threshold: backend.osr_threshold(), backend: Box::new(backend) }
    }
    #[inline]
    pub fn backend<T: TierBackend>(&self) -> Option<&T> {
        (&*self.backend as &dyn Any).downcast_ref()
    }
    #[inline]
    pub fn invoke(&self, ctx: &VmContext, target: TierTarget<'_>, args: CallArgs<'_>, method_type_args: &[String])
        -> Option<Result<ExecOutcome>> { self.backend.invoke(ctx, target, args, method_type_args) }
    pub fn osr(&self, ctx: &VmContext, func: &Function, header: usize, state: OsrState<'_>)
        -> Option<Result<ExecOutcome>> { self.backend.osr(ctx, func, header, state) }
}

pub struct EngineRegistry {
    baseline: OnceLock<&'static dyn Engine>,
    jit_engine: OnceLock<&'static dyn Engine>,
    jit_config: OnceLock<JitConfig>,
    tier: OnceLock<TierApi>,
    // Backend code must survive the trait objects and compiled-code owners above.
    code_leases: parking_lot::Mutex<Vec<std::sync::Arc<dyn Send + Sync>>>,
}

impl EngineRegistry {
    pub fn new(baseline: Option<&'static dyn Engine>) -> Self {
        let baseline_cell = OnceLock::new();
        if let Some(engine) = baseline { let _ = baseline_cell.set(engine); }
        Self { baseline: baseline_cell, jit_engine: OnceLock::new(),
            jit_config: OnceLock::new(), tier: OnceLock::new(), code_leases: Default::default() }
    }
    pub fn register_baseline(&self, engine: &'static dyn Engine) -> Result<()> {
        self.baseline.set(engine).map_err(|_| anyhow::anyhow!("interpreter already registered"))
    }
    pub fn register_jit(&self, engine: &'static dyn Engine, config: JitConfig) -> Result<()> {
        self.jit_config.set(config).map_err(|_| anyhow::anyhow!("JIT policy already registered"))?;
        self.jit_engine.set(engine).map_err(|_| anyhow::anyhow!("JIT already registered"))
    }
    pub fn has_jit(&self) -> bool { self.jit_engine.get().is_some() }
    pub fn has_baseline(&self) -> bool { self.baseline.get().is_some() }
    pub fn jit_config(&self) -> Result<JitConfig> {
        self.jit_config.get().copied().ok_or_else(|| anyhow::anyhow!("JIT policy not registered"))
    }
    pub fn retain_code(&self, code: std::sync::Arc<dyn Send + Sync>) { self.code_leases.lock().push(code); }
    pub fn invoke_mode(&self, mode: ExecMode, ctx: &VmContext, module: &Module,
        func: &Function, args: &[Value]) -> Result<ExecOutcome> {
        match mode {
            ExecMode::Interp => self.invoke(ctx, module, func, CallArgs::Values(args), &[]),
            ExecMode::Jit => self.jit_engine.get().ok_or_else(|| anyhow::anyhow!("JIT backend unavailable on this VM"))?
                .invoke(ctx, module, func, CallArgs::Values(args), &[]),
            ExecMode::Aot => bail!("AOT mode is not implemented yet"),
        }
    }
    pub fn tier(&self) -> Option<&TierApi> { self.tier.get() }
    pub fn install_tier(&self, tier: TierApi) -> Result<(), TierApi> { self.tier.set(tier) }
    pub fn invoke(&self, ctx: &VmContext, module: &Module, func: &Function,
        args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome> {
        self.baseline.get().ok_or_else(|| anyhow::anyhow!("no baseline engine registered on this VM"))?
            .invoke(ctx, module, func, args, type_args)
    }
    pub fn run(&self, mode: ExecMode, ctx: &VmContext, module: &Module, entry: &Function) -> Result<()> {
        match mode {
            ExecMode::Interp => self.baseline.get().ok_or_else(|| anyhow::anyhow!("no baseline engine registered on this VM"))?
                .run(ctx, module, entry),
            ExecMode::Jit => self.jit_engine.get().ok_or_else(|| anyhow::anyhow!("JIT backend unavailable on this VM"))?
                .run(ctx, module, entry),
            ExecMode::Aot => bail!("AOT mode is not implemented yet (planned: roadmap M9 — L3 \
                phase, LLVM/inkwell). Switch to `--mode interp` or `--mode jit`."),
        }
    }
}

pub fn invoke(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<ExecOutcome> {
    invoke_args(ctx, module, func, CallArgs::Values(args), &[])
}
pub fn invoke_args(ctx: &VmContext, module: &Module, func: &Function,
    args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome> {
    ctx.core.exec.engines.invoke(ctx, module, func, args, type_args)
}

impl VmContext {
    #[inline]
    pub fn tier(&self) -> Option<&TierApi> {
        #[cfg(feature = "jit")]
        {
            let p = self.exec.jit_head.jit.load(std::sync::atomic::Ordering::Relaxed);
            // Only the owner thread refreshes this pointer. Publication came through
            // OnceLock, and this context keeps the owning core (and tier) alive.
            unsafe { p.as_ref() }
        }
        #[cfg(not(feature = "jit"))]
        { self.core.exec.engines.tier() }
    }
    pub fn publish_tier(&self) {
        #[cfg(feature = "jit")]
        if let Some(tier) = self.core.exec.engines.tier() {
            self.exec.jit_head.jit.store(tier as *const TierApi as *mut TierApi, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod engine_tests;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JitConfig {
    pub threshold: u32,
    pub osr_threshold: u32,
    pub interp_tierup: u32,
    pub profile: bool,
    pub debug_promote: bool,
}
