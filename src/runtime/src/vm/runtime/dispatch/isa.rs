//! Pure registered-type name/shape predicates. No loading or execution.

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
pub(crate) fn erased_instance_matches(def: &str, inst_args: &[String], target: &str) -> Option<bool> {
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
pub(crate) fn type_name_matches(actual: &str, target: &str) -> bool {
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
pub(crate) fn is_open_type_arg_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['.', '<', '[', ','])
        && !is_keyword_type_name(name)
}

/// `"a,B<c,d>,e"` → `["a", "B<c,d>", "e"]` (commas nested in `<…>` do not split).
pub(crate) fn split_top_level_args(s: &str) -> Vec<&str> {
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
pub(crate) fn is_open_type_arg(arg: &str) -> bool {
    arg.split(|c: char| matches!(c, '<' | '>' | ',' | '[' | ']' | '?' | ' '))
        .filter(|t| !t.is_empty())
        .any(|t| !t.contains('.') && !is_keyword_type_name(t))
}

/// The keyword spellings of built-in types (`type_names::canonical_arg`) — the only dot-free
/// names a canonical type argument can carry.
pub(crate) fn is_keyword_type_name(t: &str) -> bool {
    crate::metadata::index::type_names::builtin_keyword(t) == Some(t)
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
pub(crate) fn erased_name_matches(full: &str, target: &str) -> bool {
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
pub(crate) fn erased_either(a: &str, b: &str) -> bool {
    erased_name_matches(a, b) || erased_name_matches(b, a)
}

/// Number of top-level type arguments in `"<a,b<c,d>>"` (nested `<…>` do not count).
pub(crate) fn top_level_arg_count(args: &str) -> usize {
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

