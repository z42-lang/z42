//! perf-collections-sort: native stable sort of an array prefix whose elements are all
//! one primitive kind — the fast path behind `List<T>.Sort()` / `Array.Sort<T>(T[])`.
//!
//! The script-side merge sort pays one virtual `CompareTo` call (plus boxed
//! `ArrayGet`/`ArraySet` helpers) per comparison. When every element is the same
//! primitive `Value` variant, the `CompareTo` that call would reach is fixed — primitives
//! are sealed, and the VM dispatches a `Value::I64` / `F64` / `Char` / `Str` receiver to
//! `Std.Int32` / `Std.Double` / `Std.Char` / `Std.String` (`interp::exec_vcall::
//! primitive_class_name`) — so sorting natively with the **same** order and a **stable**
//! algorithm yields exactly the permutation the script would. Anything else (objects,
//! boxed structs, null, mixed kinds, packed backings) declines and the script sorts.

use std::cmp::Ordering;

use super::{ArrayBacking, ArrayObj, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrimKind { Int, Double, Char, Str }

fn kind_of(v: &Value) -> Option<PrimKind> {
    match v {
        Value::I64(_) => Some(PrimKind::Int),
        Value::F64(_) => Some(PrimKind::Double),
        Value::Char(_) => Some(PrimKind::Char),
        Value::Str(_) => Some(PrimKind::Str),
        _ => None,
    }
}

/// `Std.Double.CompareTo`'s order: a total order where NaN sorts before every
/// non-NaN and equals itself; otherwise `<` / `>` (so `-0.0 == 0.0`).
fn double_compare_to(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

impl ArrayObj {
    /// Stable-sort elements `[0, n)` in place when they are all one primitive kind
    /// (int-like / double / char / string), ordering exactly as their `CompareTo`.
    /// Returns `false` — array untouched — when it declines (see module doc) or
    /// `n` exceeds the length; the caller then runs the script sort.
    pub fn sort_prims_prefix(&mut self, n: usize) -> bool {
        let ArrayBacking::Boxed { block, len } = &self.backing else { return false };
        if n > *len {
            return false;
        }
        // SAFETY: `&mut self` = exclusive access to a live ArrayValue block of `len`
        // `Value`s; `n <= len`.
        let elems = unsafe { &mut Self::slice_of_mut::<Value>(block, *len)[..n] };
        let Some(kind) = elems.first().and_then(kind_of) else { return n == 0 };
        if !elems.iter().all(|v| kind_of(v) == Some(kind)) {
            return false;
        }
        // A permutation moves references around without dropping any, but it does
        // overwrite slots — report them to the SATB barrier like any bulk write.
        crate::gc::satb::record_overwrite_all(elems);
        match kind {
            PrimKind::Int => elems.sort_by_key(|v| match v { Value::I64(x) => *x, _ => 0 }),
            PrimKind::Char => elems.sort_by_key(|v| match v { Value::Char(c) => *c, _ => '\0' }),
            PrimKind::Double => elems.sort_by(|a, b| match (a, b) {
                (Value::F64(x), Value::F64(y)) => double_compare_to(*x, *y),
                _ => Ordering::Equal,
            }),
            PrimKind::Str => elems.sort_by(|a, b| match (a, b) {
                (Value::Str(x), Value::Str(y)) => (**x).cmp(&**y),
                _ => Ordering::Equal,
            }),
        }
        true
    }
}

#[cfg(test)]
#[path = "array_sort_tests.rs"]
mod array_sort_tests;
