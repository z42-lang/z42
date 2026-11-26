//! What the type-parameter markers of a shared generic body (`$mta:N` / `$cta:N@Owner`,
//! [`crate::zpkg::index::type_param_markers`]) mean in **one activation**.
//!
//! The carriers are the ones `typeof(T)` already reads, so a body's `o is T`, `new List<T>()`,
//! `catch (Err<T>)` and `typeof(T)` agree on what `T` is:
//! - method-level → the frame's `method_type_args`. Both tiers carry them for direct reads;
//!   marker substitution in compiled bodies still falls back (see `jit::translate::unsupported`);
//! - class-level → the receiver in reg 0, walked to the level that declares the parameter
//!   ([`class_type_arg`], which `__class_type_arg` / `__class_default` resolve through too).
//!
//! Interpreter marker substitution uses the full environment. JIT marker helpers only
//! bind class parameters until the corresponding method-level operations are supported.

use crate::zpkg::index::type_param_markers::{self, Marker};
use crate::exec::ops::object::static_defaults::zero_value_for_type;
use crate::object::TypeArgs;
use crate::object::Value;
use crate::vm::VmContext;
use std::borrow::Cow;

/// Read a method parameter as a reflection Type. Missing arguments retain the
/// synthetic T used by older bytecode.
pub fn method_type_value(ctx: &VmContext, args: &[String], index: usize) -> Value {
    match args.get(index) {
        Some(name) => (ctx.core().object_protocol().make_type_from_name)(ctx, name),
        None => (ctx.core().object_protocol().make_constructed_type)(ctx, "T", &[]),
    }
}

/// Read a method parameter's default value, or Null if the argument is missing.
pub fn method_default_value(ctx: &VmContext, args: &[String], index: usize) -> Value {
    args.get(index).map_or(Value::Null, |tag| zero_value_for_type(ctx, tag))
}

/// Resolve a class parameter at its declaring class before materializing its default.
pub fn class_default_value(ctx: &VmContext, recv: &Value, index: usize, owner: Option<&str>) -> Value {
    class_type_arg(ctx, recv, index, owner)
        .map_or(Value::Null, |tag| zero_value_for_type(ctx, &tag))
}

#[derive(Clone, Copy)]
pub struct TypeParamEnv<'a> {
    method_args: &'a [String],
    this: Option<&'a Value>,
}

impl<'a> TypeParamEnv<'a> {
    pub fn new(method_args: &'a [String], this: Option<&'a Value>) -> Self {
        Self { method_args, this }
    }

    /// The concrete argument a marker stands for here; `None` = not bound in this activation
    /// (a lifted lambda has no method arguments, a static method no receiver).
    pub(crate) fn resolve(&self, ctx: &VmContext, m: Marker<'_>) -> Option<Cow<'a, str>> {
        match m {
            Marker::Method(i) => self.method_args.get(i).map(|arg| Cow::Borrowed(arg.as_str())),
            Marker::Class { index, owner } =>
                class_type_arg(ctx, self.this?, index, Some(owner)).map(Cow::Owned),
        }
    }

    /// `name` with every bound marker replaced by its argument — the spelling `ObjNew` records
    /// for an instantiation written with concrete arguments.
    pub fn substitute(&self, ctx: &VmContext, name: &str) -> String {
        type_param_markers::substitute(name, |m| self.resolve(ctx, m).map(Cow::into_owned))
    }

    /// A type-test target (`is` / `as` / cast / `catch`) with its markers substituted. A target
    /// that *is* a type parameter bound to a built-in comes out in keyword spelling (`int`); the
    /// tests name built-ins by their class (`Std.Int32`), as z42c does for a written `o is int`.
    pub fn test_target(&self, ctx: &VmContext, name: &str) -> String {
        let t = self.substitute(ctx, name);
        match crate::zpkg::index::type_names::builtin_fqn(&t) {
            Some(fqn) => fqn.to_string(),
            None => t,
        }
    }

    /// An `ObjNew` site's type arguments for this activation: unchanged unless they carry
    /// markers (`new List<T>()` in a shared body), else substituted and interned.
    #[inline]
    pub fn instantiate(&self, ctx: &VmContext, site: TypeArgs) -> TypeArgs {
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
/// The argument at the declaring class of a class-level type parameter.
pub fn class_type_arg(ctx: &VmContext, recv: &Value, idx: usize, owner: Option<&str>) -> Option<String> {
    use crate::zpkg::index::type_names::erased;

    let (name, own_args) = receiver_parts(ctx, recv)?;
    let Some(owner) = owner else { return own_args.get(idx).cloned() };
    let owner = erased(owner);
    if erased(&name) == owner {
        return crate::object::type_args_from_name(&name).get(idx).cloned()
            .or_else(|| own_args.get(idx).cloned());
    }
    inherited_type_arg(ctx, &name, own_args.as_slice(), owner, idx)
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
        Value::StackObject { idx, frame_id } => ctx.exec.stack_arena.lock()
            .with_obj(*idx, *frame_id, |o| (o.type_desc.name.to_string(), o.type_args_handle()))
            .ok(),
        _ => None,
    }
}

/// Bind each base from the current class before reading the declaring class's argument.
fn inherited_type_arg(
    ctx: &VmContext,
    recv_name: &str,
    mut own_args: &[String],
    owner: &str,
    idx: usize,
) -> Option<String> {
    use crate::metadata::types::resolve_base_instance;
    use crate::zpkg::index::type_names::erased;

    let resolve = |name: &str| ctx.loader().resolve_type(name);
    let (mut descriptor, mut inherited_args) = resolve_base_instance(recv_name, resolve)?;
    // Keep the bound for malformed metadata cycles.
    for _ in 0..64 {
        let args = if !inherited_args.is_empty() {
            &inherited_args
        } else if !descriptor.type_args().is_empty() {
            descriptor.type_args()
        } else {
            own_args
        };
        if erased(&descriptor.name) == owner {
            return args.get(idx).cloned();
        }
        let projected = descriptor.projected_base_name(args);
        let base = projected.as_deref().or(descriptor.base_name.as_deref())?;
        (descriptor, inherited_args) = resolve_base_instance(base, resolve)?;
        own_args = &[];
    }
    None
}

#[cfg(all(test, z42_test_facade))]
#[path = "type_param_env_tests.rs"]
mod type_param_env_tests;
