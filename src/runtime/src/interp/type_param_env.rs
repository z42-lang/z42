//! What the type-parameter markers of a shared generic body (`$mta:N` / `$cta:N@Owner`,
//! [`crate::metadata::index::type_param_markers`]) mean in **one activation**.
//!
//! The carriers are the ones `typeof(T)` already reads, so a body's `o is T`, `new List<T>()`,
//! `catch (Err<T>)` and `typeof(T)` agree on what `T` is:
//! - method-level → the frame's `method_type_args` (interp only — a JIT frame has none, so a
//!   body with a method-level marker never JITs; see `jit::translate::unsupported`);
//! - class-level → the receiver in reg 0, walked to the level that declares the parameter
//!   ([`class_type_arg`], which `__class_type_arg` / `__class_default` resolve through too).
//!
//! Both engines go through this one type: interp builds it from its `Frame`
//! (`Frame::type_param_env`), the JIT helpers from their frame's reg 0 (no method arguments).

use crate::metadata::index::type_param_markers::{self, Marker};
use crate::object::TypeArgs;
use crate::object::Value;
use crate::vm_context::VmContext;

#[derive(Clone, Copy)]
pub(crate) struct TypeParamEnv<'a> {
    method_args: &'a [String],
    this: Option<&'a Value>,
}

impl<'a> TypeParamEnv<'a> {
    pub(crate) fn new(method_args: &'a [String], this: Option<&'a Value>) -> Self {
        Self { method_args, this }
    }

    /// The concrete argument a marker stands for here; `None` = not bound in this activation
    /// (a lifted lambda has no method arguments, a static method no receiver).
    fn resolve(&self, ctx: &VmContext, m: Marker<'_>) -> Option<String> {
        match m {
            Marker::Method(i) => self.method_args.get(i).cloned(),
            Marker::Class { index, owner } => class_type_arg(ctx, self.this?, index, Some(owner)),
        }
    }

    /// `name` with every bound marker replaced by its argument — the spelling `ObjNew` records
    /// for an instantiation written with concrete arguments.
    pub(crate) fn substitute(&self, ctx: &VmContext, name: &str) -> String {
        type_param_markers::substitute(name, |m| self.resolve(ctx, m))
    }

    /// A type-test target (`is` / `as` / cast / `catch`) with its markers substituted. A target
    /// that *is* a type parameter bound to a built-in comes out in keyword spelling (`int`); the
    /// tests name built-ins by their class (`Std.Int32`), as z42c does for a written `o is int`.
    pub(crate) fn test_target(&self, ctx: &VmContext, name: &str) -> String {
        let t = self.substitute(ctx, name);
        match crate::metadata::index::type_names::builtin_fqn(&t) {
            Some(fqn) => fqn.to_string(),
            None => t,
        }
    }

    /// An `ObjNew` site's type arguments for this activation: unchanged unless they carry
    /// markers (`new List<T>()` in a shared body), else substituted and interned.
    #[inline]
    pub(crate) fn instantiate(&self, ctx: &VmContext, site: TypeArgs) -> TypeArgs {
        if !site.has_markers() {
            return site;
        }
        self.instantiate_slow(ctx, site)
    }

    #[inline(never)]
    fn instantiate_slow(&self, ctx: &VmContext, site: TypeArgs) -> TypeArgs {
        if let Some(own) = self.receivers_own_args(site) {
            return own;
        }
        let args: Vec<String> = site.iter().map(|a| self.substitute(ctx, a)).collect();
        TypeArgs::intern(&args)
    }

    /// The common shape — `new Node<T>()` / `new ListEnumerator<T>(this)` inside `Owner<T>`'s
    /// own body: the site's arguments are exactly the receiver's class parameters in order, and
    /// the receiver is an `Owner` itself (not a subclass, whose parameters are its own). Then
    /// the receiver's interned arguments *are* the answer — no walk, no formatting, no lookup.
    fn receivers_own_args(&self, site: TypeArgs) -> Option<TypeArgs> {
        let Some(Value::Object(rc)) = self.this else { return None };
        let obj = rc.borrow();
        let own = obj.type_args_handle();
        if own.len() != site.len() {
            return None;
        }
        let name: &str = &obj.type_desc.name;
        let erased = name.split('<').next().unwrap_or(name);
        for (i, a) in site.iter().enumerate() {
            match type_param_markers::parse_marker(a) {
                Some((Marker::Class { index, owner }, len)) if len == a.len() && index == i && owner == erased => {}
                _ => return None,
            }
        }
        Some(own)
    }
}

/// complete-generic-class-identity（邻线收尾）: resolve a **class-level** type parameter to the
/// concrete argument bound at the level that **declares** it.
///
/// Two carriers, and the order between them is the whole point:
///
/// 1. **The declaring level's name.** `class DInt : Box<int> {}` has an empty `type_args` of
///    its own — the argument lives in the *base name*, which is `Demo.Box<int>` only since P1
///    stopped stripping closed generic bases. Walk the chain to the level whose erased name is
///    `owner`, then read the argument off that name.
/// 2. **The receiver's own `type_args`.** Still needed for cross-package generics, which get no
///    identity name (so their descriptor name carries no arguments) and rely on the per-instance
///    carrier `ObjNew` fills in.
///
/// 🔴 **(2) must not run when the declaring class is a *base*.** `class DG<U> : Box<int>` has
/// `type_args = [U's argument]`, so reading index 0 off the receiver answers with `DG`'s
/// parameter while the question was about `Box`'s — a plausible-but-wrong type, which is worse
/// than the placeholder. Measured: `DG<string>` reported `Std.String` for `Box`'s `T`.
pub(crate) fn class_type_arg(ctx: &VmContext, recv: &Value, idx: usize, owner: Option<&str>) -> Option<String> {
    let (name, own_args) = receiver_parts(ctx, recv)?;
    if let Some(o) = &owner {
        if let Some(n) = inherited_type_arg(ctx, &name, o, idx) {
            return Some(n);
        }
    }
    let own_is_declaring = match &owner {
        None => true,   // old bytecode: keep the previous behaviour verbatim
        Some(o) => erased_of(&name) == erased_of(o),
    };
    if own_is_declaring {
        return own_args.get(idx).cloned();
    }
    None
}

/// The receiver's descriptor name and own type arguments. An escape-analysis **stack** object
/// (`StackObject`, `new Box<int>(..)` that never leaves its frame) is a receiver like a heap one:
/// its methods' `typeof(T)` / `o is T` read the same two carriers.
fn receiver_parts(ctx: &VmContext, recv: &Value) -> Option<(String, crate::object::TypeArgs)> {
    match recv {
        Value::Object(rc) => {
            let o = rc.borrow();
            Some((o.type_desc.name.to_string(), o.type_args_handle()))
        }
        Value::StackObject { idx, frame_id } => ctx.stack_arena.lock()
            .with_obj(*idx, *frame_id, |o| (o.type_desc.name.to_string(), o.type_args_handle()))
            .ok(),
        _ => None,
    }
}

/// Erased prefix of a (possibly instantiated) type name: `Demo.Box<int>` → `Demo.Box`.
fn erased_of(name: &str) -> &str {
    match name.find('<') {
        Some(i) => &name[..i],
        None => name,
    }
}

/// Walk the receiver's base chain to the level declaring `owner`, and read the `idx`-th
/// type argument **off that level's name**. `None` when the chain has no such level or it
/// carries no arguments (both degrade to today's placeholder).
fn inherited_type_arg(
    ctx: &VmContext,
    recv_name: &str,
    owner: &str,
    idx: usize,
) -> Option<String> {
    let mut cur = recv_name.to_string();
    let want = erased_of(owner).to_string();
    // Bounded: a class hierarchy is finite, but a corrupt `base_name` cycle must not hang us.
    for _ in 0..64 {
        if erased_of(&cur) == want {
            let targs = crate::object::type_args_from_name(&cur);
            return targs.get(idx).cloned();
        }
        let td = ctx.try_lookup_type(&cur)?;
        cur = td.base_name.clone()?;
    }
    None
}

#[cfg(test)]
#[path = "type_param_env_tests.rs"]
mod type_param_env_tests;
