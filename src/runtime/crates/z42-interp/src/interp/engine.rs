//! Baseline engine registration; frame construction remains interpreter-owned.
use anyhow::Result;
use crate::exec::call::{args::CallArgs, outcome::ExecOutcome};
use crate::exec::engine::Engine;
use crate::metadata::{Function, Module};
use crate::context::VmContext;

pub static ENGINE: Interpreter = Interpreter;
pub struct Interpreter;

impl Engine for Interpreter {
    fn invoke(&self, ctx: &VmContext, module: &Module, func: &Function,
        args: CallArgs<'_>, type_args: &[String]) -> Result<ExecOutcome> {
        match args {
            CallArgs::Values(values) if type_args.is_empty() => super::run::exec_function(ctx, module, func, values),
            CallArgs::Values(values) => super::run::exec_function_with_type_args(ctx, module, func, values, type_args),
            CallArgs::Registers { regs, indices } => super::run::exec_function_from_regs(ctx, module, func, regs, indices, type_args),
            CallArgs::Receiver { receiver, regs, indices } =>
                super::run::exec_function_from_receiver_regs(ctx, module, func, &receiver, regs, indices, type_args),
        }
    }
    fn run(&self, ctx: &VmContext, module: &Module, entry: &Function) -> Result<()> {
        super::run_with_static_init(ctx, module, entry)
    }
}
