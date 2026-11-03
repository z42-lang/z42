use super::*;

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn method_markers_read_the_method_arguments() {
    let ctx = VmContext::new();
    let mta = args(&["Demo.Entry", "int"]);
    let env = TypeParamEnv::new(&mta, None);
    assert_eq!(env.substitute(&ctx, "Std.Collections.Dictionary<$mta:1,$mta:0>"),
               "Std.Collections.Dictionary<int,Demo.Entry>");
    // Out of range / no receiver: the marker stays (reads as an open argument).
    assert_eq!(env.substitute(&ctx, "Demo.Box<$mta:5>"), "Demo.Box<$mta:5>");
    assert_eq!(env.substitute(&ctx, "Demo.Box<$cta:0@Demo.G>"), "Demo.Box<$cta:0@Demo.G>");
}

#[test]
fn test_target_names_builtins_by_class() {
    let ctx = VmContext::new();
    let mta = args(&["int", "object", "string", "Demo.Entry", "Std.Collections.List<int>"]);
    let env = TypeParamEnv::new(&mta, None);
    assert_eq!(env.test_target(&ctx, "$mta:0"), "Std.Int32");
    assert_eq!(env.test_target(&ctx, "$mta:1"), "Std.Object");
    assert_eq!(env.test_target(&ctx, "$mta:2"), "Std.String");
    assert_eq!(env.test_target(&ctx, "$mta:3"), "Demo.Entry");
    // Only a whole-target keyword is renamed; arguments keep the spelling instances record.
    assert_eq!(env.test_target(&ctx, "$mta:4"), "Std.Collections.List<int>");
    assert_eq!(env.test_target(&ctx, "Demo.Box<$mta:0>"), "Demo.Box<int>");
}

#[test]
fn site_args_without_markers_are_returned_as_is() {
    let ctx = VmContext::new();
    let env = TypeParamEnv::new(&[], None);
    let site = TypeArgs::intern(&args(&["int"]));
    assert!(!site.has_markers());
    assert!(env.instantiate(&ctx, site) == site);
}

#[test]
fn site_args_with_markers_are_substituted_and_interned() {
    let ctx = VmContext::new();
    let mta = args(&["Demo.Entry"]);
    let env = TypeParamEnv::new(&mta, None);
    let site = TypeArgs::intern(&args(&["$mta:0", "Std.Collections.List<$mta:0>"]));
    assert!(site.has_markers());
    let got = env.instantiate(&ctx, site);
    assert!(!got.has_markers());
    assert!(got == TypeArgs::intern(&args(&["Demo.Entry", "Std.Collections.List<Demo.Entry>"])));
}
