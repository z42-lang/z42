use super::*;

#[test]
fn detects_markers() {
    assert!(has_markers("$mta:0"));
    assert!(has_markers("Std.Collections.List<$cta:1@Demo.Bag>"));
    assert!(!has_markers("Std.MulticastException$1"));   // arity mangle is not a marker
    assert!(!has_markers("Demo.Entry"));
    assert!(has_method_markers("Demo.Box<$mta:2>"));
    assert!(!has_method_markers("Demo.Box<$cta:0@Demo.G>"));
}

#[test]
fn parses_both_kinds() {
    assert_eq!(parse_marker("$mta:12>"), Some((Marker::Method(12), 7)));
    assert_eq!(
        parse_marker("$cta:0@Demo.Box$1,int>"),
        Some((Marker::Class { index: 0, owner: "Demo.Box$1" }, "$cta:0@Demo.Box$1".len())),
    );
    assert_eq!(parse_marker("$cta:0"), None);       // owner is required
    assert_eq!(parse_marker("$cta:0@>"), None);
    assert_eq!(parse_marker("$mta:x"), None);
    assert_eq!(parse_marker("$1"), None);
}

#[test]
fn substitutes_nested_and_keeps_unresolved() {
    let r = substitute("Std.Collections.Dictionary<$mta:0,Demo.Box<$cta:1@Demo.G>>", |m| match m {
        Marker::Method(0) => Some("string".to_string()),
        Marker::Class { index: 1, owner: "Demo.G" } => Some("Demo.Entry".to_string()),
        _ => None,
    });
    assert_eq!(r, "Std.Collections.Dictionary<string,Demo.Box<Demo.Entry>>");
    // Unresolvable markers stay as they are; plain `$` (arity mangles) pass through.
    assert_eq!(substitute("Demo.Pair$2<$mta:3>", |_| None), "Demo.Pair$2<$mta:3>");
    assert_eq!(substitute("$mta:0[]", |_| Some("int".into())), "int[]");
}
