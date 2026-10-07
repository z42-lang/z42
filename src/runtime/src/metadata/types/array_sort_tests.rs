use super::*;

fn arr(elems: Vec<Value>) -> ArrayObj { ArrayObj::new_leaked(elems) }

fn ints(a: &ArrayObj) -> Vec<i64> {
    a.iter_boxed().map(|v| match v { Value::I64(x) => x, other => panic!("not int: {other:?}") }).collect()
}

#[test]
fn sorts_int_prefix_only() {
    let mut a = arr(vec![Value::I64(5), Value::I64(-3), Value::I64(9), Value::I64(0), Value::I64(-7)]);
    assert!(a.sort_prims_prefix(4));
    assert_eq!(ints(&a), vec![-3, 0, 5, 9, -7], "slots past `n` (List spare capacity) are untouched");
}

#[test]
fn doubles_follow_compare_to_total_order() {
    let mut a = arr(vec![Value::F64(1.5), Value::F64(f64::NAN), Value::F64(-2.0), Value::F64(f64::NAN)]);
    assert!(a.sort_prims_prefix(4));
    let got: Vec<f64> = a.iter_boxed().map(|v| match v { Value::F64(x) => x, _ => panic!() }).collect();
    assert!(got[0].is_nan() && got[1].is_nan(), "NaN sorts first (Std.Double.CompareTo)");
    assert_eq!(&got[2..], &[-2.0, 1.5]);
}

#[test]
fn strings_sort_ordinally() {
    let s = |x: &str| Value::Str(x.into());
    let mut a = arr(vec![s("pear"), s("Apple"), s("apple"), s("é"), s("b")]);
    assert!(a.sort_prims_prefix(5));
    let got: Vec<String> = a.iter_boxed().map(|v| match v { Value::Str(x) => x.to_string(), _ => panic!() }).collect();
    assert_eq!(got, vec!["Apple", "apple", "b", "pear", "é"]);
}

#[test]
fn chars_sort_by_scalar() {
    let mut a = arr(vec![Value::Char('z'), Value::Char('A'), Value::Char('m')]);
    assert!(a.sort_prims_prefix(3));
    let got: Vec<char> = a.iter_boxed().map(|v| match v { Value::Char(c) => c, _ => panic!() }).collect();
    assert_eq!(got, vec!['A', 'm', 'z']);
}

#[test]
fn declines_mixed_or_non_primitive_and_leaves_array_untouched() {
    let mut mixed = arr(vec![Value::I64(2), Value::F64(1.0)]);
    assert!(!mixed.sort_prims_prefix(2));
    assert!(matches!(mixed.get_boxed(0), Value::I64(2)));

    let mut with_null = arr(vec![Value::I64(2), Value::Null, Value::I64(1)]);
    assert!(!with_null.sort_prims_prefix(3), "null must reach the script's CompareTo (it throws)");
    assert!(matches!(with_null.get_boxed(0), Value::I64(2)));
}

#[test]
fn out_of_range_prefix_declines() {
    let mut a = arr(vec![Value::I64(2), Value::I64(1)]);
    assert!(!a.sort_prims_prefix(3));
    assert_eq!(ints(&a), vec![2, 1]);
}
