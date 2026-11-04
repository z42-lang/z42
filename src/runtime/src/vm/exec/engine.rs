//! Per-VM backend contracts. Compilers, code pages and backend frames stay in engines.
use std::any::Any;
use std::sync::OnceLock;
use anyhow::{bail, Result};
use crate::metadata::ExecMode;
use crate::object::Value;
use crate::vm::runtime::{Function, Module};
use crate::vm_context::VmContext;
use super::call::{args::CallArgs, outcome::ExecOutcome};

pub(crate) trait Engine: Send + Sync {
    fn invoke(&self, ctx: &VmContext, module: &Module, func: &Function,
        args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome>;
    fn run(&self, ctx: &VmContext, module: &Module, entry: &Function) -> Result<()>;
}

/// A call-site resolve counts toward tier-up; an existing-function divert uses
/// the backend's configured peek policy. Keeping these distinct prevents double counting.
pub(crate) enum TierTarget<'a> { Site(usize), Existing(&'a Function) }

pub(crate) struct OsrState<'a> {
    pub regs: &'a [Value],
    pub frame_id: u32,
    pub has_ref_writebacks: bool,
}

pub(crate) trait TierBackend: Any + Send + Sync {
    fn osr_threshold(&self) -> u32;
    fn invoke(&self, ctx: &VmContext, target: TierTarget<'_>, args: CallArgs<'_>)
        -> Option<Result<ExecOutcome>>;
    fn osr(&self, ctx: &VmContext, func: &Function, header: usize, state: OsrState<'_>)
        -> Option<Result<ExecOutcome>>;
}

/// The stable, neutral header cached by contexts. The boxed backend owns its code;
/// the VM keeps this header alive until every context sharing its core has dropped.
pub(crate) struct TierApi {
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
    pub fn invoke(&self, ctx: &VmContext, target: TierTarget<'_>, args: CallArgs<'_>)
        -> Option<Result<ExecOutcome>> { self.backend.invoke(ctx, target, args) }
    pub fn osr(&self, ctx: &VmContext, func: &Function, header: usize, state: OsrState<'_>)
        -> Option<Result<ExecOutcome>> { self.backend.osr(ctx, func, header, state) }
}

pub(crate) type TierRun = fn(&VmContext, &Module, &str) -> Result<()>;

pub(crate) struct EngineRegistry {
    baseline: Option<&'static dyn Engine>,
    tier_run: Option<TierRun>,
    tier: OnceLock<TierApi>,
}

impl EngineRegistry {
    pub fn new(baseline: Option<&'static dyn Engine>, tier_run: Option<TierRun>) -> Self {
        Self { baseline, tier_run, tier: OnceLock::new() }
    }
    pub fn tier(&self) -> Option<&TierApi> { self.tier.get() }
    pub fn install_tier(&self, tier: TierApi) -> Result<(), TierApi> { self.tier.set(tier) }
    pub fn invoke(&self, ctx: &VmContext, module: &Module, func: &Function,
        args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome> {
        self.baseline.ok_or_else(|| anyhow::anyhow!("no baseline engine registered on this VM"))?
            .invoke(ctx, module, func, args, type_args)
    }
    pub fn run(&self, mode: ExecMode, ctx: &VmContext, module: &Module, entry: &Function) -> Result<()> {
        match mode {
            ExecMode::Interp => self.baseline.ok_or_else(|| anyhow::anyhow!("no baseline engine registered on this VM"))?
                .run(ctx, module, entry),
            ExecMode::Jit => match self.tier_run {
                Some(run) => run(ctx, module, &entry.name),
                None => bail!("JIT mode requested but the runtime was built without the `jit` feature. \
                    Recompile with `--features jit` (default), or run the bytecode in `--mode interp`."),
            },
            ExecMode::Aot => bail!("AOT mode is not implemented yet (planned: roadmap M9 — L3 \
                phase, LLVM/inkwell). Switch to `--mode interp` or `--mode jit`."),
        }
    }
}

pub(crate) fn invoke(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<ExecOutcome> {
    invoke_args(ctx, module, func, CallArgs::Values(args), &[])
}
pub(crate) fn invoke_args(ctx: &VmContext, module: &Module, func: &Function,
    args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome> {
    ctx.core.exec.engines.invoke(ctx, module, func, args, type_args)
}

impl VmContext {
    #[inline]
    pub(crate) fn tier(&self) -> Option<&TierApi> {
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
    pub(crate) fn publish_tier(&self) {
        #[cfg(feature = "jit")]
        if let Some(tier) = self.core.exec.engines.tier() {
            self.exec.jit_head.jit.store(tier as *const TierApi as *mut TierApi, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod engine_tests;
