/// Object dispatch helpers — vtable resolution, ToString protocol, type checks.
///
/// Static-field storage moved to `VmContext::static_fields` (consolidate-vm-state).
/// Call sites now use `ctx.static_get(field)` / `ctx.static_set(...)`.

use crate::metadata::{ClassDesc, FieldSlot, Function, Module, TypeDesc, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

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
    let def = crate::metadata::type_names::erased(derived);
    if def.len() == derived.len() {
        return false;
    }
    let args = crate::metadata::types::type_args_from_name(derived);
    if let Some(v) = erased_instance_matches(def, &args, target) {
        return v;
    }
    is_subclass_or_eq_td(ctx, registry, def, target)
}

/// Type test for a **receiver descriptor** — the single entry used by interp `is` / `as` /
/// typed `catch` and their JIT helpers. `key` is the site's cached target key
/// ([`TypeKeyCell`](crate::metadata::tokens::TypeKeyCell): instruction / exception-table row /
/// a `static`), resolved here on first use. Then: `ctx.isa_cache` keyed by
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
    key: &crate::metadata::tokens::TypeKeyCell,
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
    key: &crate::metadata::tokens::TypeKeyCell,
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
    if class_name.ends_with("[]") || crate::metadata::type_names::builtin_fqn(class_name).is_some() {
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
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<crate::metadata::TypeDesc>>,
    idx: u32, frame_id: u32, class_name: &str,
) -> bool {
    if class_name == "Std.Object" || class_name == "Object" {
        return true;
    }
    let Ok(name) = ctx.struct_arena.lock().with(idx, frame_id, |s| s.type_name.clone()) else { return false };
    &*name == class_name || is_subclass_or_eq_td(ctx, registry, &name, class_name)
}

/// `Some(verdict)` when `target` names an instantiation of the erased definition `def` (that is,
/// `target == "{def}<…>"` and `def` itself is not an instantiation); `None` = not this case.
///
/// Arguments compare by spelling: both sides are in the canonical spelling
/// (`type_names::canonical_arg`) — the compiler emits it for the instance's arguments (`ObjNew`)
/// and for the target, and every runtime producer of instance arguments (reflection) goes through
/// that function. An instance argument that is still an **unsubstituted
/// type parameter** (`new List<T>()` inside a generic body records `"T"`) cannot be decided: it
/// matches, which is the erased-generic answer the test had before instantiations had identity.
#[inline(never)]
fn erased_instance_matches(def: &str, inst_args: &[String], target: &str) -> Option<bool> {
    let inner = target.strip_prefix(def)?.strip_prefix('<')?.strip_suffix('>')?;
    if def.contains('<') {
        return None;
    }
    let targs = split_top_level_args(inner);
    if targs.len() != inst_args.len() {
        return Some(false);
    }
    Some(inst_args.iter().zip(targs.iter()).all(|(a, t)| a == t || is_open_type_arg(a)))
}

/// Type test for an **array** receiver whose element type is `elem` — the single entry used by
/// interp and JIT `is` / `as` / checked casts.
///
/// An array is-a `Std.Array` and `Std.Object`, and is-a `X[]` exactly when its element type is
/// `X`: arrays are invariant (no covariance — `Dog[]` is not an `Animal[]`). The compiler spells
/// the element of an array target and the element recorded by `ArrayNew` with the same function,
/// so the comparison is by name; [`type_name_matches`] adds the cases a name cannot decide.
/// An element-less array (`""`: built by the runtime without an element type) matches any `X[]`.
pub fn array_isa(elem: &str, target: &str) -> bool {
    if matches!(target, "Array" | "Object" | "Std.Array" | "Std.Object") {
        return true;
    }
    match target.strip_suffix("[]") {
        Some(t) => elem.is_empty() || type_name_matches(elem, t),
        None => false,
    }
}

/// Does the runtime type name `actual` match the test target `target`? Equal names match; so does
/// an `actual` that is (or contains, at the same position) an unsubstituted type parameter
/// (`new T[n]` / `new Box<T>[n]` in a generic body records `T`) — the erased answer
/// [`erased_instance_matches`] gives for the same situation.
fn type_name_matches(actual: &str, target: &str) -> bool {
    if actual == target || is_open_type_arg_name(actual) {
        return true;
    }
    if let (Some(a), Some(t)) = (actual.strip_suffix("[]"), target.strip_suffix("[]")) {
        return type_name_matches(a, t);
    }
    let (Some(al), Some(tl)) = (actual.find('<'), target.find('<')) else {
        return false;
    };
    if actual[..al] != target[..tl] || !actual.ends_with('>') || !target.ends_with('>') {
        return false;
    }
    let aargs = split_top_level_args(&actual[al + 1..actual.len() - 1]);
    let targs = split_top_level_args(&target[tl + 1..target.len() - 1]);
    aargs.len() == targs.len() && aargs.iter().zip(targs.iter()).all(|(a, t)| type_name_matches(a, t))
}

/// A whole name that is a bare type parameter (`T`): dot-free, not a keyword, no structure.
fn is_open_type_arg_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['.', '<', '[', ','])
        && !is_keyword_type_name(name)
}

/// `"a,B<c,d>,e"` → `["a", "B<c,d>", "e"]` (commas nested in `<…>` do not split).
fn split_top_level_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// Does a recorded type argument still contain an unsubstituted type parameter? Runtime type
/// names are namespace-qualified (`Demo.Entry`) or primitive keywords (`int`); a bare identifier
/// that is neither is a type parameter (`T`).
fn is_open_type_arg(arg: &str) -> bool {
    arg.split(|c: char| matches!(c, '<' | '>' | ',' | '[' | ']' | '?' | ' '))
        .filter(|t| !t.is_empty())
        .any(|t| !t.contains('.') && !is_keyword_type_name(t))
}

/// The keyword spellings of built-in types (`type_names::canonical_arg`) — the only dot-free
/// names a canonical type argument can carry.
fn is_keyword_type_name(t: &str) -> bool {
    crate::metadata::type_names::builtin_keyword(t) == Some(t)
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
    match ctx.isa_cache.get(td.id.0, key) {
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
    let pair = crate::vm_context::isa_cache::pair_key(recv, key);
    // optimize-subclass-check: the (type, target) verdict is a global, monotonic fact — a
    // loaded type's base/interface chain never changes and lazy loading only ADDs types — so it
    // is memoised. Cleared on explicit module (re)load (REPL) — see `load_module_*`.
    let memo = ctx.subclass_memo.lock().get(&pair).copied();
    let v = match memo {
        Some(v) => v,
        None => {
            // `recv == key`: the key is the target's own TypeId and `td` is (a version of) it.
            let v = recv == key || td.name == target
                || is_subclass_or_eq_td_walk(ctx, registry, &td.name, target);
            ctx.subclass_memo.lock().insert(pair, v);
            v
        }
    };
    ctx.isa_cache.put(recv, key, v);
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

/// complete-generic-class-identity P2: does the instantiated type name `full` match a test
/// written against the *erased* generic?
///
/// Two spellings of the erased name reach us, and both are legitimate:
///   * `Demo.GBox`      — a local generic referred to by its bare name;
///   * `Demo.GBox$1`    — arity-mangled, how an **imported** generic is spelled in metadata
///                        (see `StmtEmitter`'s `catch_type`: imported generic exceptions get
///                        `$<arity>`). Missing this one meant `catch (MulticastException<bool>)`
///                        stopped catching `Std.MulticastException<bool>` (measured).
///
/// Only the erased prefix matches — `GBox<int>` against `GBox<string>` stays false, which is the
/// whole point of giving instantiations identity.
fn erased_name_matches(full: &str, target: &str) -> bool {
    let lt = match full.find('<') {
        Some(i) => i,
        None => return false,
    };
    let erased = &full[..lt];
    if erased == target {
        return true;
    }
    let dollar = match target.rfind('$') {
        Some(i) => i,
        None => return false,
    };
    if &target[..dollar] != erased {
        return false;
    }
    match target[dollar + 1..].parse::<usize>() {
        Ok(n) => n == top_level_arg_count(&full[lt..]),
        Err(_) => false,
    }
}

/// [`erased_name_matches`] in either direction: one name is an instantiation, the other its
/// erased definition.
///
/// The second direction (the **descriptor** is the erased one, the **target** is instantiated)
/// is the descriptor not knowing its arguments, not a wildcard in the test: a non-generic class's
/// interface list is recorded erased (`class Bar : IFoo<int>` lists `Demo.IFoo`), as are the base
/// and interfaces of a generic *definition*. `bar is IFoo<int>` cannot be decided from that and is
/// answered as the erased test — the only answer such a descriptor ever gave. Instantiation
/// descriptors list their bases / interfaces instantiated, and those compare exactly.
fn erased_either(a: &str, b: &str) -> bool {
    erased_name_matches(a, b) || erased_name_matches(b, a)
}

/// Number of top-level type arguments in `"<a,b<c,d>>"` (nested `<…>` do not count).
fn top_level_arg_count(args: &str) -> usize {
    let mut depth = 0usize;
    let mut n = 0usize;
    for ch in args.chars() {
        match ch {
            '<' => {
                depth += 1;
                if depth == 1 {
                    n = 1; // the first argument; commas below add the rest
                }
            }
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 1 => n += 1,
            _ => {}
        }
    }
    n
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

// ── ToString protocol ────────────────────────────────────────────────────────

/// The `ToString` protocol: stringify `val`, respecting `ToString()` overrides, and hand
/// the text to `f` as a `&str` (perf-str-concat-direct: nothing is materialised).
///
/// For `Value::Object` we try to dispatch `ToString` via the vtable. If the class has no
/// `ToString` method (e.g. it inherits the default from `Std.Object`) we fall back to the
/// `__obj_to_str` builtin (simple name). Boxed receivers go through `resolve_vcall`. All
/// other value types use `value_to_str` rules. A user `ToString`'s returned GC string is passed
/// through as-is (no copy); scalars are formatted into a stack buffer
/// ([`with_value_str`](crate::corelib::convert::with_value_str)) — no heap allocation.
/// `"k" + i` / `$"{x}"` build their result straight from this view. Same dispatch, same
/// result text and exception propagation for every caller (concat, interpolation, `WriteLine`).
pub fn with_obj_str<R>(
    ctx: &VmContext, module: &Module, val: &Value, f: impl FnOnce(&str) -> R,
) -> Result<R> {
    use crate::corelib::convert::with_value_str;
    let out = match tostring_result(ctx, module, val)? {
        ToStringResult::Value(Value::Str(s)) => f(&s),
        ToStringResult::Value(other) => with_value_str(&other, f),
        ToStringResult::Empty => f(""),
        ToStringResult::NotDispatched => with_value_str(val, f),
    };
    Ok(out)
}

/// perf-str-concat-direct: [`with_obj_str`] as a GC string (`ToStr` / `$"{x}"`). A user
/// `ToString`'s returned string is used as-is — no copy (the JIT's native `jit_to_str` path
/// does the same); everything else is formatted straight into one fresh GC block.
pub fn obj_to_gc_str(ctx: &VmContext, module: &Module, val: &Value) -> Result<crate::metadata::vstr::Str> {
    use crate::corelib::convert::with_value_str;
    let heap = ctx.heap();
    Ok(match tostring_result(ctx, module, val)? {
        ToStringResult::Value(Value::Str(s)) => s,
        ToStringResult::Value(other) => with_value_str(&other, |t| heap.alloc_str(t)),
        ToStringResult::Empty => heap.alloc_str(""),
        ToStringResult::NotDispatched => with_value_str(val, |t| heap.alloc_str(t)),
    })
}

/// The raw outcome of the `ToString` protocol for one receiver (see [`with_obj_str`]).
enum ToStringResult {
    /// What the dispatched `ToString` (or the `__obj_to_str` / boxed fallback) returned.
    Value(Value),
    /// A `ToString` that returned nothing.
    Empty,
    /// No dispatch applies — stringify the receiver itself (`value_to_str` rules).
    NotDispatched,
}

fn tostring_result(ctx: &VmContext, module: &Module, val: &Value) -> Result<ToStringResult> {
    fn outcome(ctx: &VmContext, o: super::ExecOutcome) -> Result<ToStringResult> {
        match o {
            super::ExecOutcome::Returned(Some(v)) => Ok(ToStringResult::Value(v)),
            super::ExecOutcome::Returned(None)    => Ok(ToStringResult::Empty),
            super::ExecOutcome::Thrown(v)         => Err(tostring_threw(ctx, v)),
        }
    }
    if let Value::Object(rc) = val {
        let type_desc = rc.type_desc_arc().clone();
        // Try vtable first (O(1))
        let func_name_opt = type_desc.vtable_index.get("ToString")
            .map(|&slot| type_desc.vtable[slot].1.clone());
        if let Some(func_name) = func_name_opt {
            let callee = module.func_index.get(func_name.as_str())
                .and_then(|&idx| module.functions.get(idx));
            if let Some(callee) = callee {
                return outcome(ctx, super::exec_function(ctx, module, callee, &[val.clone()])?);
            }
        }
        // Fallback: the Rust default ToString (unqualified type name), as the
        // `__obj_to_str` builtin behind the vtable slot would answer.
        return crate::corelib::object::obj_to_str(val)
            .map(|name| ToStringResult::Value(Value::Str(name.into())))
            .map_err(|e| e.into_anyhow(ctx));
    }
    // dispatch-tostring-in-native-stringify: **装箱**接收者（值 struct 装箱 / 基元装箱 / enum 盒）。
    // 复用 `resolve_vcall` 的整套判据 —— 它已经把三种盒各自的正确答案都定好了：
    // enum → 成员名、基元 → 标量、struct → 自身槽位的 `ToString`（没有才短类型名），
    // 且**刻意不回落 `Std.Object.ToString`**（那个 builtin 收装箱 struct 直接抛
    // `__obj_to_str: expected an object`）。自己再写一份判据必然与它漂移。
    // P1-2 PR 8: a callable holds its target's `FnId`; the display names the function
    // (`<fn N>` / `<closure N>`, the text `value_to_str` printed when the value held the name).
    if let Some(id) = val.callable_id() {
        if let Some(f) = super::exec_call::fn_by_id(ctx, module, id.0) {
            let kind = if matches!(val, Value::FuncRef(_)) { "fn" } else { "closure" };
            let text = format!("<{kind} {}>", f.name);
            return Ok(ToStringResult::Value(Value::Str(ctx.heap().alloc_str(&text))));
        }
        return Ok(ToStringResult::NotDispatched);
    }
    if matches!(val, Value::BoxedStruct(_)) {
        let r = super::vcall_resolve::resolve_vcall(ctx, module, val, "ToString", 0, None)?;
        return match r.target {
            super::vcall_resolve::VCallTarget::Immediate(v) => Ok(ToStringResult::Value(v)),
            super::vcall_resolve::VCallTarget::Thrown(v) => Err(tostring_threw(ctx, v)),
            super::vcall_resolve::VCallTarget::Local { func, .. } =>
                outcome(ctx, super::exec_function(ctx, module, func, &[r.this.clone()])?),
        };
    }
    Ok(ToStringResult::NotDispatched)
}

/// A user `ToString` threw while stringifying (concatenation, interpolation,
/// `Console.WriteLine(obj)`). The exception propagates with its own type, like
/// any other call's: the value goes into `pending_thrown` — the same channel
/// callback builtins use — and the returned error tells the caller to take it.
/// Every caller of [`with_obj_str`] / [`obj_to_gc_str`] / [`stringify_dispatch`] must therefore
/// check `take_pending_thrown()` on `Err` (interp `Add` / `ToStr`, JIT
/// `jit_add` / `jit_to_str`; builtins get it from `exec_call::builtin`).
///
/// Earlier the exception was swallowed into an `<exception: …>` string and the
/// program carried on.
fn tostring_threw(ctx: &VmContext, thrown: Value) -> anyhow::Error {
    ctx.set_pending_thrown(thrown);
    anyhow::anyhow!("ToString threw an exception")
}

/// dispatch-tostring-in-native-stringify: `with_obj_str` 的 **ctx-only** 包装 —— 给手里只有
/// `&VmContext` 的 native 落点用（`Console.WriteLine` 一族 builtin、字符串拼接的混合臂）。
///
/// 此前这些落点直接用无 ctx 的 `value_to_str` ⇒ 对象一律打 `类型名{...}`，于是同一个对象
/// 「插值对、`WriteLine` 错」。`module` 从 `ctx.core.module` 取（与 `reflection/invoke.rs`
/// 的取法同源）；取不到（未装载模块的宿主场景）就回落 `value_to_str`，不为展示路径制造失败。
pub fn stringify_dispatch(ctx: &VmContext, val: &Value) -> Result<String> {
    with_stringify_dispatch(ctx, val, |s| s.to_string())
}

/// perf-str-concat-direct: [`stringify_dispatch`] as a GC string (see [`obj_to_gc_str`]).
pub fn stringify_dispatch_gc(ctx: &VmContext, val: &Value) -> Result<crate::metadata::vstr::Str> {
    match ctx.core.module.as_ref() {
        Some(m) => {
            let m = m.clone();
            obj_to_gc_str(ctx, m.as_ref(), val)
        }
        None => Ok(crate::corelib::convert::with_value_str(val, |t| ctx.heap().alloc_str(t))),
    }
}

/// perf-str-concat-direct: [`stringify_dispatch`] as a `&str` view (see [`with_obj_str`]).
pub fn with_stringify_dispatch<R>(ctx: &VmContext, val: &Value, f: impl FnOnce(&str) -> R) -> Result<R> {
    match ctx.core.module.as_ref() {
        Some(m) => {
            let m = m.clone();
            with_obj_str(ctx, m.as_ref(), val, f)
        }
        None => Ok(crate::corelib::convert::with_value_str(val, f)),
    }
}

// ── Virtual method resolution (fallback) ─────────────────────────────────────

/// Fallback linear walk used when TypeDesc is missing (e.g. stdlib stubs).
pub fn resolve_virtual<'m>(module: &'m Module, class_name: &str, method: &str) -> Result<&'m Function> {
    let mut cur = class_name;
    loop {
        let qualified = format!("{}.{}", cur, method);
        if let Some(f) = module.func_index.get(qualified.as_str()).and_then(|&i| module.functions.get(i)) {
            return Ok(f);
        }
        match module.classes.iter().find(|c| c.name == cur).and_then(|c| c.base_class.as_deref()) {
            Some(base) => cur = base,
            None => bail!("VCall: no implementation of `{}` in hierarchy of `{}`", method, class_name),
        }
    }
}

// ── Fallback TypeDesc ────────────────────────────────────────────────────────

/// Build a minimal TypeDesc from the ClassDesc chain — used when the registry
/// is absent (merged stdlib modules arrive without pre-built TypeDesc).
pub fn make_fallback_type_desc(module: &Module, class_name: &str) -> TypeDesc {
    let mut fields: Vec<FieldSlot> = Vec::new();
    let mut base_name: Option<String> = None;
    let mut cur = class_name;
    let mut chain: Vec<&ClassDesc> = Vec::new();
    loop {
        if let Some(desc) = module.classes.iter().find(|c| c.name == cur) {
            chain.push(desc);
            match &desc.base_class {
                Some(b) => { base_name = Some(b.clone()); cur = b.as_str(); }
                None    => break,
            }
        } else {
            break;
        }
    }
    for desc in chain.iter().rev() {
        for f in &desc.fields {
            if !fields.iter().any(|s: &FieldSlot| &*s.name == f.name.as_str()) {
                fields.push(FieldSlot {
                    name: f.name.clone().into_boxed_str(),
                    type_tag: f.type_tag.clone().into_boxed_str(),
                    visibility: f.visibility,
                });
            }
        }
    }
    let field_index = fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    // Fallback type — there's no separate "own vs inherited" split because
    // `chain` walked the inheritance chain by-name within this module. Mark
    // all fields as own (cross-zpkg fixup won't re-process this entry since
    // base resolution above is already complete).
    let own_fields = fields.clone();
    // Like the loader's fallback for a class without an object block: a synthesized layout
    // (references in type-parameter cells), so field access and GC walks read a stored layout
    // instead of synthesizing one per access.
    let composed = (!fields.is_empty())
        .then(|| std::sync::Arc::new(crate::metadata::types::synthesize_object_layout(&fields)));
    TypeDesc {
        name: class_name.to_string(),
        base_name,
        class_flags: 0,  // fallback TypeDesc — no class-shape info
        visibility: 0,
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(crate::metadata::types::TypeDescCold {
            composed_object_layout: composed,
            own_fields: own_fields.into(),
            ..Default::default()
        })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    }
}

#[cfg(test)]
#[path = "dispatch_isa_tests.rs"]
mod dispatch_isa_tests;
