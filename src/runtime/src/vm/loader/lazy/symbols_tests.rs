use super::*;

fn dep(file: &str, syms: &[&str]) -> ZpkgDep {
    ZpkgDep { file: file.to_string(), namespaces: vec![], symbols: syms.iter().map(|s| s.to_string()).collect() }
}

fn loader_with(deps: &[ZpkgDep]) -> LazyLoader {
    let mut l = LazyLoader::new(Vec::new(), 0, Vec::new(), Vec::new());
    l.note_symbol_owners(deps);
    l
}

#[test]
fn symbol_key_strips_generic_args_and_signature_suffix() {
    assert_eq!(symbol_key("Std.Collections.List<Std.Int32>"), "Std.Collections.List");
    assert_eq!(symbol_key("Demo.Util.F$1$Std.List<int>"), "Demo.Util.F");
    assert_eq!(symbol_key("Demo.Outer+Inner"), "Demo.Outer+Inner");
    assert_eq!(symbol_key("Plain"), "Plain");
}

#[test]
fn exact_symbol_routes_to_its_owner_even_when_the_namespace_is_shared() {
    // Two packages both provide `Std`; the reference table says which one defines what.
    let l = loader_with(&[dep("z42.toml.zpkg", &["Std.Toml.TomlValue"]), dep("z42.json.zpkg", &["Std.Json.JsonValue"])]);
    let r = l.symbol_route("Std.Toml.TomlValue").expect("routed");
    assert_eq!(r.file, "z42.toml.zpkg");
    assert!(r.exact);
    let g = l.symbol_route("Std.Toml.TomlValue<Std.Int32>").expect("routed");
    assert_eq!(g.file, "z42.toml.zpkg");
    assert!(!g.exact, "a constructed generic name is not itself a recorded symbol");
}

#[test]
fn member_names_route_through_their_owning_type_but_are_not_exact() {
    let l = loader_with(&[dep("z42.toml.zpkg", &["Std.Toml.TomlValue"])]);
    let m = l.symbol_route("Std.Toml.TomlValue.Parse$1$string").expect("routed");
    assert_eq!(m.file, "z42.toml.zpkg");
    assert!(!m.exact, "the method may come from an impl block / base class elsewhere");
    assert!(l.symbol_route("Std.Other.Thing").is_none());
}

#[test]
fn first_owner_wins_and_short_names_are_unique_or_nothing() {
    let l = loader_with(&[
        dep("a.zpkg", &["A.Widget", "A.Gadget"]),
        dep("b.zpkg", &["A.Widget", "B.Gadget"]),
    ]);
    assert_eq!(l.symbol_route("A.Widget").unwrap().file, "a.zpkg");
    assert_eq!(l.full_name_for_short("Widget").as_deref(), Some("A.Widget"));
    assert_eq!(l.full_name_for_short("Gadget"), None, "two different full names share the short name");
    assert_eq!(l.full_name_for_short("Nope"), None);
}
