use super::*;
use crate::object::Value;

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn intern_dedupes_by_content_and_is_thin() {
    let a = TypeArgs::intern(&v(&["Std.Int32", "Std.String"]));
    let b = TypeArgs::intern(&v(&["Std.Int32", "Std.String"]));
    assert!(std::ptr::eq(a.as_raw(), b.as_raw()), "same content ⇒ same entry");
    assert_eq!(a, b);
    assert_eq!(a.as_slice(), &["Std.Int32".to_string(), "Std.String".to_string()][..]);
    assert_eq!(std::mem::size_of::<TypeArgs>(), 8);
    assert!(std::mem::align_of::<TypeArgsInfo>() >= 2, "ObjMeta tags the low bit");
}

#[test]
fn different_content_or_order_is_a_different_entry() {
    let a = TypeArgs::intern(&v(&["K1", "V1"]));
    let b = TypeArgs::intern(&v(&["V1", "K1"]));
    let c = TypeArgs::intern(&v(&["K1"]));
    assert_ne!(a, b);
    assert_ne!(a, c);
    assert!(!std::ptr::eq(a.as_raw(), b.as_raw()));
}

#[test]
fn empty_never_touches_the_table_and_compares_equal() {
    let e = TypeArgs::intern(&[]);
    assert!(e.is_empty());
    assert_eq!(e, TypeArgs::empty());
    assert_eq!(e, TypeArgs::default());
    assert!(std::ptr::eq(e.as_raw(), TypeArgs::empty().as_raw()));
}

#[test]
fn raw_round_trip() {
    let a = TypeArgs::intern(&v(&["T"]));
    // SAFETY: round-trips a pointer obtained from `as_raw`.
    let b = unsafe { TypeArgs::from_raw(a.as_raw()) };
    assert_eq!(a, b);
    assert_eq!(&*b, &["T".to_string()][..]);
}

/// Interning is process-global: two threads (standing in for two VMs / lazily loaded
/// packages) that intern the same instantiation get the same handle.
#[test]
fn concurrent_interning_converges_on_one_entry() {
    let hs: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| TypeArgs::intern(&v(&["Conc.Probe", "Std.Int64"])).as_raw() as usize))
        .collect();
    let ptrs: Vec<usize> = hs.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(ptrs.windows(2).all(|w| w[0] == w[1]));
}

/// M13: the interned entry precomputes each argument's packed kind (`ElemKind::of_type_param`),
/// and `elem_kind` agrees with running the match on the name again.
#[test]
fn elem_kind_cache_matches_the_names() {
    let ta = TypeArgs::intern(&v(&["int", "geometry.Point", "string", "long", "T"]));
    assert_eq!(ta.elem_kind(0), ElemKind::I32);
    assert_eq!(ta.elem_kind(1), ElemKind::Mixed); // user class → erased cells
    assert_eq!(ta.elem_kind(2), ElemKind::Mixed); // reference argument stays MIXED (M13 scope)
    assert_eq!(ta.elem_kind(3), ElemKind::I64);
    assert_eq!(ta.elem_kind(4), ElemKind::Mixed); // the open parameter's own literal name
    assert_eq!(ta.elem_kind(99), ElemKind::Mixed); // out of bounds
    assert_eq!(TypeArgs::empty().elem_kind(0), ElemKind::Mixed);
}

/// M13's vocabulary invariant: every name `default_value_for` answers with a primitive zero
/// selects a packed kind in `ElemKind::of_type_param`, and the kind's zero round-trips — the
/// two tables (packing width, default value) must not drift apart.
#[test]
fn type_param_vocabularies_agree_with_default_value_for() {
    for (name, zero, kind) in [
        ("int", Value::I64(0), ElemKind::I32),
        ("long", Value::I64(0), ElemKind::I64),
        ("short", Value::I64(0), ElemKind::I32),
        ("byte", Value::I64(0), ElemKind::Bytes),
        ("sbyte", Value::I64(0), ElemKind::I32),
        ("ushort", Value::I64(0), ElemKind::I32),
        ("uint", Value::I64(0), ElemKind::I64),
        ("ulong", Value::I64(0), ElemKind::I64),
        ("isize", Value::I64(0), ElemKind::I64),
        ("f32", Value::F64(0.0), ElemKind::F64),
        ("double", Value::F64(0.0), ElemKind::F64),
        ("bool", Value::Bool(false), ElemKind::Bool),
        ("char", Value::Char('\0'), ElemKind::Chars),
        ("Std.Int32", Value::I64(0), ElemKind::I32),
        ("Std.UInt16", Value::I64(0), ElemKind::I32),
        ("Std.UInt32", Value::I64(0), ElemKind::I64),
        ("Std.Int64", Value::I64(0), ElemKind::I64),
        ("Std.Single", Value::F64(0.0), ElemKind::F64),
        ("Std.Boolean", Value::Bool(false), ElemKind::Bool),
        ("Std.Char", Value::Char('\0'), ElemKind::Chars),
    ] {
        assert_eq!(ElemKind::of_type_param(name), kind, "packing kind for {name:?}");
        assert_eq!(crate::object::default_value_for(name), zero, "zero for {name:?}");
    }
    // Everything else stays erased.
    for name in ["", "T", "string", "object", "Std.String", "geometry.Point", "int[]"] {
        assert_eq!(ElemKind::of_type_param(name), ElemKind::Mixed, "{name:?} must stay Mixed");
        assert!(matches!(crate::object::default_value_for(name), Value::Null), "{name:?}");
    }
}

// fix-generic-subclass-of-base: an instantiation's members may live under the erased
// definition; the vtable-slot key derivation must strip that prefix too.
#[test]
fn erased_definition_name_and_member_keys() {
    assert_eq!(crate::object::TypeDesc::erased_definition_name("Demo.G<int>"), Some("Demo.G"));
    assert_eq!(crate::object::TypeDesc::erased_definition_name("Demo.G$1<Demo.P<int>>"), Some("Demo.G$1"));
    assert_eq!(crate::object::TypeDesc::erased_definition_name("Demo.G"), None);
    assert_eq!(crate::object::TypeDesc::derive_simple_method_name("Demo.G<int>", "Demo.G<int>.Who"), "Who");
    assert_eq!(crate::object::TypeDesc::derive_simple_method_name("Demo.G<int>", "Demo.G.Who$1$int"), "Who$1$int");
    assert_eq!(crate::object::TypeDesc::derive_simple_method_name("Demo.G", "Demo.G.Who"), "Who");
    assert_eq!(crate::object::TypeDesc::derive_simple_method_name("Demo.H<int>", "Demo.G.Who"), "Demo.G.Who");
}
