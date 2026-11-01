use super::*;

#[test]
fn last_top_dot_ignores_dots_in_args() {
    let n = "Demo.Pair<Demo.A.P,int>.Get";
    assert_eq!(&n[..last_top_dot(n).unwrap()], "Demo.Pair<Demo.A.P,int>");
    let s = "Demo.C.Echo:Demo.A.P";
    assert_eq!(&s[..last_top_dot(s).unwrap()], "Demo.C");
    assert_eq!(last_top_dot("main"), None);
    let t = "Demo.Pair<Demo.A.P>";
    assert_eq!(&t[..last_top_dot(t).unwrap()], "Demo");
}

#[test]
fn last_top_plus_ignores_args() {
    let n = "Demo.Box<Demo.Outer+In>";
    assert_eq!(last_top(n, '+'), None);
    let m = "Demo.Outer+In";
    assert_eq!(&m[..last_top(m, '+').unwrap()], "Demo.Outer");
}

#[test]
fn simple_and_unqualified_strip_args_recursively() {
    assert_eq!(simple("Demo.Pair<Demo.A.P,int>"), "Pair<P,int>");
    assert_eq!(simple("Demo.Pair<Demo.Pair<Demo.B.P,long>,string>"), "Pair<Pair<P,long>,string>");
    assert_eq!(simple("Nest.Outer+Inner"), "Inner");
    assert_eq!(unqualified("Nest.Outer+Inner"), "Outer+Inner");
    assert_eq!(simple("Foo"), "Foo");
    assert_eq!(simple("Std.MulticastException$1<bool>"), "MulticastException$1<bool>");
    assert_eq!(simple("Demo.P[]"), "P[]");
}

#[test]
fn builtin_spellings_map_to_one_keyword_and_fqn() {
    for s in ["int", "i32", "Std.Int32"] {
        assert_eq!(builtin_keyword(s), Some("int"));
        assert_eq!(builtin_fqn(s), Some("Std.Int32"));
    }
    assert_eq!(builtin_keyword("Std.Object"), Some("object"));
    assert_eq!(builtin_fqn("object"), Some("Std.Object"));
    assert_eq!(builtin_keyword("str"), Some("string"));
    assert_eq!(builtin_fqn("float"), Some("Std.Single"));
    assert_eq!(builtin_keyword("Demo.Entry"), None);
    assert_eq!(builtin_keyword("Int32"), None);
}

#[test]
fn canonical_spells_args_as_keywords_and_top_level_as_fqn() {
    assert_eq!(canonical("int"), "Std.Int32");
    assert_eq!(canonical("object"), "Std.Object");
    assert_eq!(canonical("Std.Collections.List<Std.Int32>"), "Std.Collections.List<int>");
    assert_eq!(canonical("Demo.Box<Std.Object>"), "Demo.Box<object>");
    assert_eq!(canonical("Std.Func<Std.Int32, Std.String>"), "Std.Func<int,string>");
    assert_eq!(
        canonical("Std.Collections.Dictionary<Std.String,Demo.Box<Std.Int32[]>>"),
        "Std.Collections.Dictionary<string,Demo.Box<int[]>>"
    );
    assert_eq!(canonical("Std.Int32[]"), "int[]");
    assert_eq!(canonical("Demo.Box<int>"), "Demo.Box<int>");
    assert_eq!(canonical_arg("Std.Int32"), "int");
    assert_eq!(canonical_arg("Std.Collections.List<Std.Int32>"), "Std.Collections.List<int>");
}

#[test]
fn display_spells_every_builtin_as_fqn() {
    assert_eq!(display("Std.Collections.List<int>"), "Std.Collections.List<Std.Int32>");
    assert_eq!(display("Demo.Box<object>"), "Demo.Box<Std.Object>");
    assert_eq!(display("object"), "Std.Object");
    assert_eq!(display("int[]"), "Std.Int32[]");
    assert_eq!(canonical(&display("Demo.Pair<int,Demo.Box<string>>")), "Demo.Pair<int,Demo.Box<string>>");
}

#[test]
fn constructed_and_substitute_produce_canonical_names() {
    let args = vec!["Std.Int32".to_string(), "Demo.Entry".to_string()];
    assert_eq!(constructed("Demo.Pair", &args), "Demo.Pair<int,Demo.Entry>");
    assert_eq!(constructed("Demo.Plain", &[]), "Demo.Plain");
    let params = vec!["T".to_string(), "U".to_string()];
    assert_eq!(substitute("T", &params, &args), "Std.Int32");
    assert_eq!(substitute("T[]", &params, &args), "int[]");
    assert_eq!(substitute("Std.Collections.List<T>", &params, &args), "Std.Collections.List<int>");
    assert_eq!(substitute("Std.Func<T,U>", &params, &args), "Std.Func<int,Demo.Entry>");
    // Whole words only: an identifier that merely contains a parameter name is left alone.
    assert_eq!(substitute("Demo.TArray", &params, &args), "Demo.TArray");
    assert_eq!(erased("Demo.Box<int>"), "Demo.Box");
}
