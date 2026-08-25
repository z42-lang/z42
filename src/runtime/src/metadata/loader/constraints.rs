use super::*;

// ── L3-G3a: constraint verification pass ───────────────────────────────────

/// Run after `build_type_registry` to validate that every constraint reference
/// (base class or interface) resolves to a known class/interface in the type
/// registry, or (for `Std.*` names) is left to the lazy loader to resolve later.
///
/// Reports the first unresolved reference and aborts load, surfacing the name
/// in the error message so zbc tampering is flagged clearly.
pub fn verify_constraints(module: &Module) -> Result<()> {
    for cls in module.type_registry.values() {
        let tp_names: Vec<&str> = cls.type_params().iter().map(|s| s.as_str()).collect();
        for bundle in cls.type_param_constraints() {
            check_constraint_refs(bundle, &module.type_registry, &cls.name, &tp_names)?;
        }
    }
    for f in &module.functions {
        let tp_names: Vec<&str> = f.type_params().iter().map(|s| s.as_str()).collect();
        for bundle in f.type_param_constraints() {
            check_constraint_refs(bundle, &module.type_registry, &f.name, &tp_names)?;
        }
    }
    Ok(())
}

fn check_constraint_refs(
    b: &crate::metadata::bytecode::ConstraintBundle,
    registry: &FxHashMap<String, Arc<TypeDesc>>,
    owner: &str,
    type_params: &[&str],
) -> Result<()> {
    check_one(b.base_class.as_deref(), registry, owner)?;
    for iface in &b.interfaces {
        check_one(Some(iface), registry, owner)?;
    }
    // add-generic-func-constraint (2026-05-11): validate type-name references in
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
    check_one(Some(base), registry, owner)
}

fn check_one(
    name: Option<&str>,
    registry: &FxHashMap<String, Arc<TypeDesc>>,
    owner: &str,
) -> Result<()> {
    let Some(n) = name else { return Ok(()); };
    if registry.contains_key(n) { return Ok(()); }
    // Std.* references are resolved by the lazy zpkg loader after module load.
    if n.starts_with("Std.") { return Ok(()); }
    // Interface-only bundles may reference interfaces not yet in the type_registry
    // (which currently holds classes only). Soft-allow; strict interface tracking lands in L3-G3b.
    if n.starts_with('I') && n.chars().nth(1).is_some_and(|c| c.is_ascii_uppercase()) {
        return Ok(());
    }
    bail!("InvalidConstraintReference: `{n}` on `{owner}` not found in type registry")
}
