use super::*;

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
