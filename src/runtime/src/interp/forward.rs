//! `[Native]` stub folding at VM link time (runtime-audit P1-8) — shared by both engines.
//!
//! Every `[Native("__x")] extern` method compiles to a one-instruction wrapper:
//!
//! ```text
//! fn @Std.String.CharAt(2) -> char { %2 = builtin __str_char_at(%0, %1); ret %2 }
//! ```
//!
//! Calling it through the ordinary protocol costs a whole activation (frame, register
//! file, the wrapper's own dispatch) only to forward the arguments unchanged to the
//! builtin. When a call resolves to such a forwarder, both engines instead run the
//! builtin straight from the caller's registers (interp `exec_call::call` /
//! `exec_vcall::invoke`; JIT `jit_call` / `jit_vcall`). Observable behaviour is the
//! wrapper's: same arguments, same result (`Null` for void, as the wrapper's `ret`
//! leaves), same exception — and the same stack trace:
//!
//! - a builtin that never runs z42 code (typed without `BuiltinFlags::reenters`) runs
//!   with no frame at all; if it fails, the wrapper's row(s) are synthesized from the
//!   function ([`wrapper_rows`]), lazily, on the error path only;
//! - one that may run z42 code (a reflection invoke, a getter, a callback, a load that
//!   runs initializers) runs under a *light* frame for the wrapper (no register file,
//!   nothing executed), so whatever that code throws, and any fatal report, sees the
//!   wrapper exactly where the unfolded call would have it. Such a builtin is folded
//!   only behind a one-level wrapper (shape 1).
//!
//! The choice is made once per wrapper and kept in its fold token (the builtin id plus
//! a "framed" bit), so a call never looks the builtin up again.
//!
//! Folding happens at **run time, by function**, never in the compiler: the compiler
//! folding the stub into callers would bake a stdlib-private builtin name into user
//! packages, binding them to one VM's builtin set across package-version boundaries.
//!
//! Shapes (all one block, no exception table, no generic parameters, every parameter
//! forwarded in order):
//! 1. `%d = builtin __x(%0…); ret %d` (or `ret` void) — [`builtin_forward`];
//! 2. `%d = call Same.Type.G(%0…); ret %d` where `G` is shape 1 — [`chained_forward`]
//!    (`string`'s indexer: `get_Item` → `CharAt` → `__str_char_at`). Same type only:
//!    the caller's type-init barrier for the outer function then covers `G` too.
//!
//! The sret stub (a builtin returning a blob struct, unboxed into the caller's sret
//! slot) is not folded: its body is three instructions that depend on the frame's
//! struct arena (`as_cast` + `struct_copy`), and its one instance (`GCHandle.Alloc`) is
//! not on any hot path.

use crate::exception::{FrameSnapshot, VmFrame};
use crate::metadata::bytecode::{Function, Instruction, Terminator};
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;

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
    // The per-site token when the resolver ran, else the table by name.
    let id = func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.first()?.first()?;
            r.builtin_tokens.get(site as usize).copied()
        })
        .filter(|&id| id != UNRESOLVED)
        .or_else(|| crate::corelib::builtin_id_of(&insn.name).map(|b| b.0))?;
    (id != UNRESOLVED).then_some(id)
}

/// Shape 2: `func` is `%d = call @Same.Type.G(%0, …); ret %d` and `lookup` resolves
/// `G` to the builtin it forwards to (shape 1).
pub(crate) fn chained_forward(func: &Function, lookup: impl FnOnce(&str) -> Option<u32>) -> Option<u32> {
    let target = chained_target(func)?;
    lookup(target)
}

/// The callee name of a shape-2 forwarder.
fn chained_target(func: &Function) -> Option<&str> {
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
    Some(&insn.func)
}

/// `Ns.Type` of a function key `Ns.Type.Method[$arity$sig…]`.
fn owner_type(name: &str) -> Option<&str> {
    let base = name.split(['$', ':']).next()?;
    base.rsplit_once('.').map(|(owner, _)| owner)
}

/// The function registered under `name` as far as already loaded — `module` first, then
/// the loaded packages. Never loads anything (a chained target is in the caller's own,
/// already loaded type).
fn probe<R>(ctx: &VmContext, module: &Module, name: &str, f: impl FnOnce(&Function) -> R) -> Option<R> {
    if let Some(func) = module.func_index.get(name).and_then(|&i| module.functions.get(i)) {
        return Some(f(func));
    }
    let func = ctx.probe_loaded_function(name)?;
    Some(f(&func))
}

/// Fold token bit: the builtin may run z42 code, so the folded call keeps a light frame
/// for the wrapper. The rest of the token is the builtin id (ids stay far below it).
const FRAMED: u32 = 1 << 30;

/// How a call of `f` folds — a *fold token* (the builtin id, plus [`FRAMED`]) — computed
/// on the first call and cached on `f` (`Function::forward`). Pass the token to
/// [`run_folded`] as is.
#[inline]
pub(crate) fn forward_of(ctx: &VmContext, module: &Module, f: &Function) -> Option<u32> {
    match f.forward.get() {
        Some(v) => v,
        None => compute_forward(ctx, module, f),
    }
}

#[cold]
#[inline(never)]
fn compute_forward(ctx: &VmContext, module: &Module, f: &Function) -> Option<u32> {
    let v = match builtin_forward(f) {
        Some(id) if id < FRAMED => Some(if crate::corelib::folds_frameless(id) { id } else { id | FRAMED }),
        Some(_) => None,
        // A chained forward folds only to a frameless builtin: a framed one would need
        // the inner wrapper's `Function` on every call.
        None => chained_forward(f, |name| probe(ctx, module, name, builtin_forward).flatten())
            .filter(|&id| id < FRAMED && crate::corelib::folds_frameless(id)),
    };
    f.forward.set(v);
    v
}

/// The register file of a light wrapper frame: nothing lives there.
static NO_REGS: Vec<Value> = Vec::new();

/// Pops the light frame of a framed folded call, whatever happens.
struct LightFrame<'a>(&'a VmContext);

impl Drop for LightFrame<'_> {
    fn drop(&mut self) { self.0.pop_frame(); }
}

/// The stack-trace rows a folded call of `f` skipped, outermost first: `f` itself and,
/// for a chained forwarder, the inner wrapper — exactly what the unfolded call would
/// have on the stack when the builtin fails. A wrapper that only ran `builtin` never
/// stamped its `pc` (no row location); a chained one stamped its `call` site.
#[cold]
pub(crate) fn wrapper_rows(ctx: &VmContext, module: &Module, f: &Function) -> smallvec::SmallVec<[FrameSnapshot; 2]> {
    let mut rows = smallvec::SmallVec::new();
    let no_regs: *const Vec<Value> = std::ptr::null();
    let outer = VmFrame::new(f, no_regs);
    match chained_target(f) {
        Some(target) => {
            outer.pc.set(f.linear_offset(0, 0));
            rows.push(outer.snapshot());
            if let Some(row) = probe(ctx, module, target, |g| VmFrame::new(g, no_regs).snapshot()) {
                rows.push(row);
            }
        }
        None => rows.push(outer.snapshot()),
    }
    rows
}

/// Run a folded call of `wrapper` (`token` from [`forward_of`]): arguments `this` (a
/// receiver not in a register) then `regs[args[i]]`. `Ok(v)` = the call's value
/// (`None` = void: the caller stores `Null`, what the wrapper's `ret` would leave);
/// `Err` = a failure, which the engine turns into what to propagate with
/// [`FoldFault::raise`]. `module` (the module exceptions are built against) is only
/// asked for on a failure.
#[inline(always)]
pub(crate) fn run_folded<'m>(
    ctx: &VmContext, wrapper: &Function, token: u32,
    this: Option<Value>, regs: &[Value], args: &[u32],
    module: impl FnOnce() -> &'m Module,
) -> Result<Option<Value>, FoldFault> {
    let id = token & !FRAMED;
    if token & FRAMED == 0 {
        return match crate::corelib::invoke_builtin_id(ctx, id, this, regs, args) {
            Ok(v) => Ok(v),
            Err(fault) => Err(FoldFault(Box::new(Failure::Unraised { id, fault }))),
        };
    }
    run_framed(ctx, wrapper, id, this, regs, args, module)
}

/// The framed half of [`run_folded`]: a light frame for the wrapper while the builtin runs.
#[inline(never)]
fn run_framed<'m>(
    ctx: &VmContext, wrapper: &Function, id: u32,
    this: Option<Value>, regs: &[Value], args: &[u32],
    module: impl FnOnce() -> &'m Module,
) -> Result<Option<Value>, FoldFault> {
    ctx.push_frame(VmFrame::new(wrapper, &NO_REGS));
    let _frame = LightFrame(ctx);
    match crate::corelib::invoke_builtin_id(ctx, id, this, regs, args) {
        Ok(v) => Ok(v),
        // The exception is built (and its trace taken) while the wrapper's frame is
        // still on the stack — no synthesized rows needed.
        Err(fault) => Err(FoldFault(Box::new(Failure::Raised(crate::corelib::abi::error::raise(
            ctx, module(), crate::corelib::builtin_name(id, "?"), *fault, &[]))))),
    }
}

/// A failed folded call, before it becomes an exception. Boxed: one word keeps the
/// call's result as small as the value it carries on the hot (success) path.
pub(crate) struct FoldFault(Box<Failure>);

enum Failure {
    Unraised { id: u32, fault: crate::corelib::abi::Fault },
    Raised(crate::corelib::abi::error::Raised),
}

impl FoldFault {
    /// What the engine propagates. A frameless call's exception gets the wrapper's rows.
    #[cold]
    #[inline(never)]
    pub(crate) fn raise(self, ctx: &VmContext, module: &Module, wrapper: &Function) -> crate::corelib::abi::error::Raised {
        match *self.0 {
            Failure::Raised(r) => r,
            Failure::Unraised { id, fault } => {
                let rows = wrapper_rows(ctx, module, wrapper);
                crate::corelib::abi::error::raise(ctx, module, crate::corelib::builtin_name(id, "?"), *fault, &rows)
            }
        }
    }
}

#[cfg(test)]
#[path = "forward_tests.rs"]
mod forward_tests;
