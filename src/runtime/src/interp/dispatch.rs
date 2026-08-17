/// Object dispatch helpers — vtable resolution, ToString protocol, type checks.
///
/// Static-field storage moved to `VmContext::static_fields` (consolidate-vm-state,
/// 2026-04-28). Call sites now use `ctx.static_get(field)` / `ctx.static_set(...)`.

use crate::metadata::{ClassDesc, FieldSlot, Function, Module, TypeDesc, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

pub use crate::corelib::convert::value_to_str;

// ── Subclass check ───────────────────────────────────────────────────────────

/// Returns true if `derived` equals `target`, is a subclass, or (when `target`
/// is an interface) implements it — checked against the TypeDesc registry.
///
/// Walks the base-class chain via the main module's `type_registry` first;
/// when a link is missing there falls back to `ctx.try_lookup_type` so
/// classes loaded lazily from imported zpkgs (e.g. `Std.TestFailure` in
/// z42.test) participate in cross-zpkg subclass checks. Without the
/// fallback `catch (Exception e)` failed to match a `TestFailure` thrown
/// across the z42.core → z42.test boundary.
///
/// add-reflection-assignable-from: at each level the type's declared interfaces
/// (now FQ-named, zbc 1.20) are compared against `target` — so `circle is IShape`
/// / `as IShape` / `IsAssignableFrom` work for interfaces (previously the chain
/// only followed `base_name`, so interface targets never matched). Transitive
/// interfaces (interface-extends-interface) are not yet covered.
pub fn is_subclass_or_eq_td(
    ctx: &VmContext,
    registry: &rustc_hash::FxHashMap<String, std::sync::Arc<TypeDesc>>,
    derived: &str,
    target: &str,
) -> bool {
    let mut cur: String = derived.to_string();
    loop {
        if cur == target { return true; }
        let td = registry.get(cur.as_str()).cloned()
            .or_else(|| ctx.try_lookup_type(cur.as_str()));
        let Some(td) = td else { return false; };
        // add-reflection-transitive-interfaces: a declared interface matches `target`
        // directly OR transitively (interface-extends-interface).
        if td.interfaces().iter().any(|i| iface_reaches_td(ctx, registry, i, target)) {
            return true;
        }
        match td.base_name.clone() {
            Some(base) => cur = base,
            None => return false,
        }
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
        if name == target { return true; }
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

/// Convert a value to its string representation, respecting `ToString()` overrides on objects.
///
/// For `Value::Object` we try to dispatch `ToString` via the vtable. If the class has no
/// `ToString` method (e.g. it inherits the default from `Std.Object`) we fall back to the
/// `__obj_to_str` builtin (simple name). All other value types use `value_to_str` directly.
pub fn obj_to_string(ctx: &VmContext, module: &Module, val: &Value) -> Result<String> {
    if let Value::Object(rc) = val {
        let type_desc = rc.type_desc_arc().clone();
        // Try vtable first (O(1))
        let func_name_opt = type_desc.vtable_index.get("ToString")
            .map(|&slot| type_desc.vtable[slot].1.clone());
        if let Some(func_name) = func_name_opt {
            let callee = module.func_index.get(func_name.as_str())
                .and_then(|&idx| module.functions.get(idx));
            if let Some(callee) = callee {
                let outcome = super::exec_function(ctx, module, callee, &[val.clone()])?;
                return match outcome {
                    super::ExecOutcome::Returned(Some(Value::Str(s))) => Ok(s.to_string()),
                    super::ExecOutcome::Returned(Some(other))         => Ok(value_to_str(&other)),
                    super::ExecOutcome::Returned(None)                => Ok(String::new()),
                    super::ExecOutcome::Thrown(v)                     => Ok(format!("<exception: {}>", value_to_str(&v))),
                };
            }
        }
        // Fallback: builtin obj_to_str (unqualified type name)
        return crate::corelib::exec_builtin(
                ctx,
                crate::metadata::well_known_names::BUILTIN_OBJ_TO_STR,
                &[val.clone()])
            .map(|v| match v { Value::Str(s) => s.to_string(), other => value_to_str(&other) });
    }
    Ok(value_to_str(val))
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
            own_fields: own_fields.into(),
            ..Default::default()
        })),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    }
}
