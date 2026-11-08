//! Type-test execution and on-demand resolution; pure predicates live in runtime/dispatch.
use crate::vm::runtime::Module;
use crate::object::{TypeDesc, Value};
use crate::vm::context::VmContext;
use crate::vm::runtime::dispatch::isa::*;

// ── Subclass check ───────────────────────────────────────────────────────────

/// Returns true if `derived` equals `target`, is a subclass, or (when `target`
/// is an interface) implements it — checked against the TypeDesc registry.
///
/// Name-only entry (reflection, debug assertions): finds `derived`'s descriptor (main module's
/// `type_registry` first, then `ctx.try_lookup_type`, so lazily loaded classes such as
/// `Std.TestFailure` in z42.harness participate) and answers through the same id-keyed caches as
/// [`isa_td`]; the target's key is looked up by name each call (cold path).
///
/// add-reflection-assignable-from: at each level the type's declared interfaces (FQ-named) are
/// compared against `target` — so `circle is IShape` / `as IShape` / `IsAssignableFrom` work for
/// interfaces; transitive interfaces too (`iface_reaches_td`).
pub fn is_subclass_or_eq_td(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    derived: &str,
    target: &str,
) -> bool {
    if derived == target {
        return true;
    }
    let Some(td) = registry.get(derived).cloned().or_else(|| ctx.try_lookup_type(derived)) else {
        return constructed_name_isa(ctx, registry, derived, target);
    };
    if !td.id.is_resolved() {
        return is_subclass_or_eq_td_walk(ctx, registry, derived, target);
    }
    let key = target_key(ctx, registry, target);
    isa_keyed(ctx, registry, &td, target, key)
}

/// [`is_subclass_or_eq_td`] for a constructed name no package declares a descriptor for
/// (`Std.Collections.List<int>` — an imported generic instantiated elsewhere): the type an
/// instance carrying the definition's descriptor plus these arguments has, answered the way
/// [`isa_object`] answers for such an instance.
fn constructed_name_isa(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    derived: &str,
    target: &str,
) -> bool {
    let def = crate::metadata::index::type_names::erased(derived);
    if def.len() == derived.len() {
        return false;
    }
    let args = crate::object::type_args_from_name(derived);
    if let Some(v) = erased_instance_matches(def, &args, target) {
        return v;
    }
    is_subclass_or_eq_td(ctx, registry, def, target)
}

/// Type test for a **receiver descriptor** — the single entry used by interp `is` / `as` /
/// typed `catch` and their JIT helpers. `key` is the site's cached target key
/// ([`TypeKeyCell`](crate::vm::runtime::ids::tokens::TypeKeyCell): instruction / exception-table row /
/// a `static`), resolved here on first use. Then: `ctx.runtime.isa_cache` keyed by
/// `(td.id, key)` (one load on a hit) → `subclass_memo` → base/interface chain walk.
///
/// Descriptors without an id (`UNRESOLVED`: transient fallbacks, native-handle singletons)
/// are answered by the walk and never cached.
#[inline]
pub fn isa_td(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    td: &TypeDesc,
    target: &str,
    key: &crate::vm::runtime::ids::tokens::TypeKeyCell,
) -> bool {
    if !td.id.is_resolved() {
        return td.name == target || is_subclass_or_eq_td_walk(ctx, registry, &td.name, target);
    }
    let k = match key.get() {
        Some(k) => k,
        None => {
            let k = target_key(ctx, registry, target);
            key.set(k);
            k
        }
    };
    isa_keyed(ctx, registry, td, target, k)
}

/// Type test for an **object** receiver: [`isa_td`] plus the one thing a descriptor alone cannot
/// answer — the per-instance type arguments of an *erased* generic instance.
///
/// A generic instantiated in its own package gets an identity descriptor (`Demo.Box<Demo.Entry>`)
/// and [`isa_td`] compares names. A generic **imported** from another package (`new List<Entry>()`
/// in user code) has no such descriptor: the object carries the definition's descriptor
/// (`Std.Collections.List`) plus its type arguments (`["Demo.Entry"]`) on the instance. The test
/// `o is List<Entry>` (target `Std.Collections.List<Demo.Entry>`) must read those arguments, or
/// it is false for every instantiation — `o as List<Entry>` was always null.
#[inline]
pub fn isa_object(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    td: &TypeDesc,
    inst_args: &[String],
    target: &str,
    key: &crate::vm::runtime::ids::tokens::TypeKeyCell,
) -> bool {
    if !inst_args.is_empty() {
        if let Some(v) = erased_instance_matches(&td.name, inst_args, target) {
            return v;
        }
    }
    isa_td(ctx, registry, td, target, key)
}

/// The extra matches a type test against a **bound type parameter** gives (`o is T` / `(T)o` in a
/// shared generic body, target substituted from the activation) — values whose runtime form does
/// not carry the type the parameter is bound to, so the ordinary test would answer "no" for a value
/// that *is* a `T`:
/// - a callable, `T` bound to a delegate type ([`callable_isa`]);
/// - an integer, `T` bound to an enum type ([`integer_as_enum_isa`]).
///
/// `false` = no extra match; the ordinary test decides.
pub fn bound_param_extra_isa(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    v: &Value,
    class_name: &str,
) -> bool {
    match v {
        Value::Closure(_) | Value::FuncRef(_) => callable_isa(ctx, registry, class_name),
        Value::I64(_) => integer_as_enum_isa(ctx, registry, class_name),
        Value::BoxedStruct(b) => b.borrow().boxed_prim_i64().is_some() && integer_as_enum_isa(ctx, registry, class_name),
        _ => false,
    }
}

/// A callable (`Closure` / `FuncRef`) against a **type parameter** bound to some type
/// (`(TD)closure` in `WeakRef<TD>`; see [`bound_param_extra_isa`]). A callable carries no delegate type — converting a lambda or
/// method group to `Action<int>` is erased — so which delegate type it is cannot be decided; it is
/// answered like the erased test: a match unless the bound type is a known non-delegate type
/// (a primitive, an array, a registered class / struct / interface that is not a delegate).
fn callable_isa(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    class_name: &str,
) -> bool {
    if class_name == "Std.Object" || class_name == "Object" {
        return true;
    }
    if class_name.ends_with("[]") || crate::metadata::index::type_names::builtin_fqn(class_name).is_some() {
        return false;
    }
    let erased = class_name.split('<').next().unwrap_or(class_name);
    match registry.get(erased).cloned().or_else(|| ctx.try_lookup_type(erased)) {
        Some(td) => td.is_delegate(),
        None => true,
    }
}

/// An integer (raw `I64` or an integer wrapper box) against a **type parameter** bound to an enum
/// type. z42 backs every enum by an `i64`, and the enum APIs hand values out as plain integers
/// (`Enum.Parse` / `GetValues` return `long`), so generic code that holds an enum value that way
/// (`CliOption<Plat>.Value` over a parsed option) has no other spelling of it: a match, as integer
/// widths already are for a raw integer (`prim_isa`).
fn integer_as_enum_isa(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    class_name: &str,
) -> bool {
    registry.get(class_name).cloned().or_else(|| ctx.try_lookup_type(class_name)).is_some_and(|td| td.is_enum())
}

/// An **unboxed** value struct (`StructRef` — a `T`-typed value in a generic body, a struct that
/// reached an `object` slot unboxed) against a target: its precise type comes from the arena slot,
/// the rest of the answer from that type's descriptor (interfaces, `Std.Object`).
pub(crate) fn struct_ref_isa(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<crate::object::TypeDesc>>,
    idx: u32, frame_id: u32, class_name: &str,
) -> bool {
    if class_name == "Std.Object" || class_name == "Object" {
        return true;
    }
    let Ok(name) = ctx.exec.struct_arena.lock().with(idx, frame_id, |s| s.type_name.clone()) else { return false };
    &*name == class_name || is_subclass_or_eq_td(ctx, registry, &name, class_name)
}

/// Front cache, then the memo / walk. `td.id` is resolved; `key` is `target`'s key.
#[inline]
fn isa_keyed(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    td: &TypeDesc,
    target: &str,
    key: u32,
) -> bool {
    match ctx.runtime.isa_cache.get(td.id.0, key) {
        Some(v) => v,
        None => isa_slow(ctx, registry, td, target, key),
    }
}

#[inline(never)]
fn isa_slow(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    td: &TypeDesc,
    target: &str,
    key: u32,
) -> bool {
    let recv = td.id.0;
    let pair = crate::vm::runtime::dispatch::isa_cache::pair_key(recv, key);
    // optimize-subclass-check: the (type, target) verdict is a global, monotonic fact — a
    // loaded type's base/interface chain never changes and lazy loading only ADDs types — so it
    // is memoised. Cleared on explicit module (re)load (REPL) — see `load_module_*`.
    let memo = ctx.runtime.subclass_memo.lock().get(&pair).copied();
    let v = match memo {
        Some(v) => v,
        None => {
            // `recv == key`: the key is the target's own TypeId and `td` is (a version of) it.
            let v = recv == key || td.name == target
                || is_subclass_or_eq_td_walk(ctx, registry, &td.name, target);
            ctx.runtime.subclass_memo.lock().insert(pair, v);
            v
        }
    };
    ctx.runtime.isa_cache.put(recv, key, v);
    v
}

/// The type-test key for target name `target` (cold: once per site, see `TypeKeyCell`): the
/// `TypeId` of the type it names if one is registered (main module first, then the lazy
/// loader — without loading anything), else the id the VM's `TypeTable` reserves for the name.
pub(crate) fn target_key(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    target: &str,
) -> u32 {
    if let Some(td) = registry.get(target).filter(|td| td.id.is_resolved()) {
        return td.id.0;
    }
    if let Some(id) = ctx.loaded_type_id(target) {
        return id.0;
    }
    ctx.types().name_key(target)
}

/// Alloc-free base+interface chain walk backing [`isa_td`] / [`is_subclass_or_eq_td`]. Caller
/// has already handled `derived == target` and the caches. Holds the current `Arc<TypeDesc>` across
/// iterations and follows `base_name` by `&str` (no per-level `String` allocation).
fn is_subclass_or_eq_td_walk(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    derived: &str,
    target: &str,
) -> bool {
    let mut cur_td = match registry
        .get(derived)
        .cloned()
        .or_else(|| ctx.try_lookup_type(derived))
    {
        Some(td) => td,
        None => return false,
    };
    loop {
        // complete-generic-class-identity P2: **the erased name is a fallback, never an identity.**
        //
        // z42 lets you write `x is GBox` with no type arguments (C# cannot name an open generic),
        // and it means "any instantiation of GBox". Once every instantiation is its own runtime
        // type, the string compare `derived == target` no longer sees it: the receiver reports
        // `Demo.GBox<int>` while the test asks for `Demo.GBox`. Measured fallout before this arm:
        // `catch (MulticastException e)` stopped catching `Std.MulticastException<bool>`.
        //
        // Only the bare prefix matches — `GBox<int>` against target `GBox<string>` stays false,
        // which is the whole point of giving instantiations identity.
        if erased_either(&cur_td.name, target) {
            return true;
        }
        // add-reflection-transitive-interfaces: a declared interface matches `target`
        // directly OR transitively (interface-extends-interface).
        if cur_td
            .interfaces()
            .iter()
            .any(|i| iface_reaches_td(ctx, registry, i, target))
        {
            return true;
        }
        let base = match cur_td.base_name.as_deref() {
            Some(b) => b,
            None => return false,
        };
        if base == target || erased_either(base, target) {
            return true;
        }
        let next = match registry
            .get(base)
            .cloned()
            .or_else(|| ctx.try_lookup_type(base))
        {
            Some(td) => td,
            None => return false,
        };
        cur_td = next;
    }
}

/// add-reflection-transitive-interfaces: true if `iface` equals `target` or
/// reaches it through its transitive base-interface chain (BFS over each
/// interface's own `interfaces()`). Used by `is`/`as`/`IsAssignableFrom` so an
/// indirectly-inherited interface (`class C : IB`, `interface IB : IA` → `c is IA`)
/// matches.
fn iface_reaches_td(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    iface: &str,
    target: &str,
) -> bool {
    let mut queue: Vec<String> = vec![iface.to_string()];
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(name) = queue.pop() {
        // Erased on either side — same rule as the class chain (see `erased_either`).
        if name == target || erased_either(&name, target) { return true; }
        if !seen.insert(name.clone()) { continue; }
        let td = registry.get(name.as_str()).cloned()
            .or_else(|| ctx.try_lookup_type(name.as_str()));
        if let Some(t) = td {
            for bi in t.interfaces() { queue.push(bi.to_string()); }
        }
    }
    false
}


#[cfg(test)]
#[path = "cast_tests.rs"]
mod cast_tests;
