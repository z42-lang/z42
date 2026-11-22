use anyhow::{bail, Result};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use crate::object::TypeDesc;
use crate::metadata::Module;

// ── L3-G3a: constraint verification pass ───────────────────────────────────

/// Validate constraint references after `build_type_registry`. A base-class
/// constraint on a type must resolve unless it is supplied by `Std.*`.
/// Method constraints and interface references may name lazy dependencies.
pub fn verify_constraints(module: &Module) -> Result<()> {
    for cls in module.type_registry.values() {
        let tp_names: Vec<&str> = cls.type_params().iter().map(|s| s.as_str()).collect();
        for bundle in cls.type_param_constraints() {
            check_one(bundle.base_class.as_deref(), &module.type_registry, &cls.name, false)?;
            check_constraint_refs(bundle, &module.type_registry, &cls.name, &tp_names)?;
        }
    }
    // A method constraint can name a base class from a lazy dependency.
    // Its satisfaction is checked at compile time.
    for f in &module.functions {
        let tp_names: Vec<&str> = f.type_params().iter().map(|s| s.as_str()).collect();
        for bundle in f.type_param_constraints() {
            check_constraint_refs(bundle, &module.type_registry, &f.name, &tp_names)?;
        }
    }
    Ok(())
}

fn check_constraint_refs(
    b: &crate::zpkg::ir::ConstraintBundle,
    registry: &FxHashMap<String, Arc<TypeDesc>>,
    owner: &str,
    type_params: &[&str],
) -> Result<()> {
    // fix-runtime-constraint-unresolved-refs (C1): interface refs are soft.
    // `verify_constraints` runs before the program context installs its lazy loader, so a
    // constraint naming an interface in a not-yet-loaded dependency zpkg is legitimately
    // unresolvable here — exactly like `Std.*`. Real resolution happens at the use
    // site, where the interpreter triggers the lazy loader. Interfaces ARE now in the
    // registry (minimal TYPE entries since zbc 1.19), so a same-module / statically-
    // merged interface resolves via `contains_key` above; only genuine lazy-dep
    // interfaces hit the soft branch. This replaces the old `I<Upper>` name heuristic,
    // which hard-rejected any non-`IFoo`-named interface (`Comparable`/`Iterable`/…).
    for iface in &b.interfaces {
        check_one(Some(iface), registry, owner, /*soft_allow_unresolved=*/ true)?;
    }
    // add-generic-func-constraint : validate type-name references in
    // the function signature constraint. Primitives / Std.* / owner's type-params
    // don't need registry lookup; user classes do.
    if let Some(sig) = &b.func_signature {
        for p in &sig.params {
            check_signature_type_ref(p, registry, owner, type_params)?;
        }
        check_signature_type_ref(&sig.ret, registry, owner, type_params)?;
    }
    Ok(())
}

/// Check a type-name reference inside a func-constraint signature.
/// Accepts primitives, "void", arrays (T[]), Std.* names, and type-param names
/// of the owning decl without registry lookup; for other names, falls back to `check_one`.
fn check_signature_type_ref(
    name: &str,
    registry: &FxHashMap<String, Arc<TypeDesc>>,
    owner: &str,
    type_params: &[&str],
) -> Result<()> {
    if matches!(name,
        "void" | "int" | "long" | "short" | "byte" | "sbyte"
        | "uint" | "ulong" | "ushort"
        | "float" | "double" | "bool" | "char" | "string" | "object"
    ) {
        return Ok(());
    }
    // Strip nullable / array / generic decoration before lookup (best-effort);
    // strict structural checking is the C# TypeChecker's job. VM verify is a
    // sanity gate against tampered zbc.
    let base = name
        .trim_end_matches('?')
        .trim_end_matches("[]")
        .split('<').next().unwrap_or(name);
    if type_params.iter().any(|tp| *tp == base) {
        return Ok(());
    }
    // Func-signature type refs may name interfaces (e.g. `where T: Func<ISink,void>`)
    // and are compile-time-validated + unused in production source; treat unresolved
    // as soft (best-effort) rather than re-introducing the name heuristic here.
    check_one(Some(base), registry, owner, /*soft_allow_unresolved=*/ true)
}

fn check_one(
    name: Option<&str>,
    registry: &FxHashMap<String, Arc<TypeDesc>>,
    owner: &str,
    soft_allow_unresolved: bool,
) -> Result<()> {
    let Some(n) = name else { return Ok(()); };
    if registry.contains_key(n) { return Ok(()); }
    // Std.* references are resolved by the lazy zpkg loader after module load.
    if n.starts_with("Std.") { return Ok(()); }
    // Interface / func-sig refs may name a not-yet-loaded lazy-dep type; defer to
    // the use site (the interpreter triggers the lazy loader). Base-class refs are strict.
    if soft_allow_unresolved { return Ok(()); }
    bail!("InvalidConstraintReference: `{n}` on `{owner}` not found in type registry")
}
