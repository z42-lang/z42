//! Builtin-forwarder short-circuit (perf-strings-json).
//!
//! Every `[Native("__x")] extern` method compiles to a one-instruction wrapper:
//!
//! ```text
//! fn @Std.String.CharAt(2) -> char { %2 = builtin __str_char_at(%0, %1); ret %2 }
//! ```
//!
//! Calling it through the ordinary protocol costs a whole activation — pooled
//! register file, `push_frame` / `pop_frame`, the wrapper's own machine-code
//! prologue — only to forward the arguments unchanged to `jit_builtin`. On
//! string-heavy code (`s.Length` / `s[i]` / `Substring` in a loop) that frame is
//! most of the cost of the call.
//!
//! A compiled entry whose function is such a pure forwarder records the
//! builtin's id ([`FnEntry::forward`](super::frame::FnEntry)); the call helpers
//! (`jit_call` after its barriers, `jit_vcall`'s `invoke_entry`) then dispatch the
//! builtin straight from the caller's registers. Observable behaviour is the same
//! as running the wrapper — same arguments, same result / void-as-`Null`, same
//! exception wrapping — except that a stack trace captured inside the builtin no
//! longer shows the extern wrapper's own row.

use smallvec::SmallVec;

use crate::metadata::bytecode::{Function, Instruction, Terminator};
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::Value;

use super::frame::{JitFrame, JitModuleCtx};
use super::helpers::call::builtin_error_into_exception;
use super::helpers::vm_ctx_ref;

/// The builtin id `func` forwards to, if `func` is exactly
/// `%d = builtin __x(%0, …, %{n-1}); ret %d` (or `ret` void) over all of its
/// parameters in order, with no exception table and no generic parameters.
pub(crate) fn builtin_forward(func: &Function) -> Option<u32> {
    if func.blocks.len() != 1 || !func.exception_table().is_empty() || !func.type_params().is_empty() {
        return None;
    }
    let block = &func.blocks[0];
    let [Instruction::Builtin(insn)] = block.instructions.as_slice() else { return None };
    match block.terminator {
        Terminator::Ret { reg: Some(r) } if r == insn.dst => {}
        Terminator::Ret { reg: None } => {}
        _ => return None,
    }
    if insn.args.len() != func.param_count
        || insn.args.iter().enumerate().any(|(i, &r)| r as usize != i) {
        return None;
    }
    // Same resolution as the `Builtin` translation (`translate/call.rs`): the
    // per-site token when the resolver ran, else the static table by name.
    let id = func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.first()?.first()?;
            r.builtin_tokens.get(site as usize).copied()
        })
        .filter(|&id| id != UNRESOLVED)
        .or_else(|| crate::corelib::builtin_id_of(&insn.name).map(|b| b.0))?;
    (id != UNRESOLVED).then_some(id)
}

/// One level of chaining: `func` is `%d = call @Same.Type.G(%0, …); ret %d` and
/// `G` is itself a builtin forwarder (`lookup` resolves `G`'s name to its
/// [`builtin_forward`] id). `string`'s indexer is exactly this — `get_Item` →
/// `CharAt` → `__str_char_at` — so `s[i]` / `foreach (char c in s)` skip both
/// activations. The target must belong to the same type as `func`: the caller's
/// static-constructor barrier for `func`'s type then already covers it.
pub(crate) fn chained_forward(func: &Function, lookup: impl FnOnce(&str) -> Option<u32>) -> Option<u32> {
    if func.blocks.len() != 1 || !func.exception_table().is_empty() || !func.type_params().is_empty() {
        return None;
    }
    let block = &func.blocks[0];
    let [Instruction::Call(insn)] = block.instructions.as_slice() else { return None };
    match block.terminator {
        Terminator::Ret { reg: Some(r) } if r == insn.dst => {}
        Terminator::Ret { reg: None } => {}
        _ => return None,
    }
    if !insn.method_type_args.is_empty()
        || insn.args.len() != func.param_count
        || insn.args.iter().enumerate().any(|(i, &r)| r as usize != i)
        || owner_type(&insn.func) != owner_type(&func.name) {
        return None;
    }
    lookup(&insn.func)
}

/// `Ns.Type` of a function key `Ns.Type.Method[$arity$sig…]`.
fn owner_type(name: &str) -> Option<&str> {
    let base = name.split('$').next()?;
    base.rsplit_once('.').map(|(owner, _)| owner)
}

/// Run forwarded builtin `id` with `this` (vcall receiver, if any) followed by
/// the caller's `arg_regs`, storing the result into `regs[dst]` (`Null` for a
/// void builtin — what the wrapper's `ret` would have produced). Returns the
/// JIT helper status: 0 = ok, 1 = exception pending.
///
/// # Safety
/// `frame` / `ctx` as for every JIT helper; `arg_regs` index `frame.regs`.
pub(crate) unsafe fn call_forward(
    frame: &mut JitFrame, ctx: *const JitModuleCtx, dst: u32, id: u32,
    this: Option<Value>, arg_regs: &[u32],
) -> u8 {
    let mut args: SmallVec<[Value; 4]> = SmallVec::new();
    if let Some(t) = this { args.push(t); }
    for &r in arg_regs { args.push(frame.regs[r as usize].clone()); }
    let vm = unsafe { vm_ctx_ref(ctx) };
    match crate::corelib::exec_builtin_by_id(vm, crate::metadata::tokens::BuiltinId(id), &args) {
        Ok(v) => { frame.regs[dst as usize] = v.unwrap_or(Value::Null); 0 }
        Err(e) => unsafe { builtin_error_into_exception(vm, ctx, e) },
    }
}

#[cfg(test)]
#[path = "forward_tests.rs"]
mod forward_tests;
