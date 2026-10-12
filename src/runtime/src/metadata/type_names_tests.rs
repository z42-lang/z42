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
