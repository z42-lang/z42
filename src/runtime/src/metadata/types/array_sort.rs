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
//! boxed structs, null, mixed kinds, packed / struct backings) declines and the script sorts.
//! REF and MIXED arrays are sorted through their element cells (`get_boxed` / `set_boxed`).

use std::cmp::Ordering;

use super::{ArrayBacking, ArrayObj, FieldWrite, Value};

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
    /// `None` — array untouched — when it declines (see module doc) or `n` exceeds the length;
    /// the caller then runs the script sort. Like the script sort it replaces, it is not atomic
    /// with respect to other threads writing the array (each element store is one atomic cell
    /// write). `Some(refs)`: sorted; `refs` are the references the
    /// stores produced that the caller must hand to the write barrier (a box, when a cell of a
    /// MIXED array had fixed another primitive kind — the permutation itself only moves
    /// references the array already held).
    pub fn sort_prims_prefix(&self, n: usize) -> Option<Vec<Value>> {
        if !matches!(self.backing, ArrayBacking::Refs { .. } | ArrayBacking::Cells { .. }) || n > self.len() {
            return None;
        }
        let mut elems: Vec<Value> = (0..n).map(|i| self.get_boxed(i)).collect();
        let Some(kind) = elems.first().and_then(kind_of) else { return (n == 0).then(Vec::new) };
        if !elems.iter().all(|v| kind_of(v) == Some(kind)) {
            return None;
        }
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
        // Each store SATB-records the slot it overwrites, like any bulk write.
        let mut barrier = Vec::new();
        for (i, v) in elems.into_iter().enumerate() {
            if let FieldWrite::Boxed(b) = self.set_boxed(i, v) {
                barrier.push(Value::Array(b));
            }
        }
        Some(barrier)
    }
}

#[cfg(test)]
#[path = "array_sort_tests.rs"]
mod array_sort_tests;
