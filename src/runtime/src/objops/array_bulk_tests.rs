//! `array_bulk` 单测：值 struct 数组（`StructBytes`）上的批量拷贝语义、类型不符的异常类、写屏障。
//! 装箱 / 拆箱两条路要真实的 struct 类型描述符，由 golden `src/tests/types/struct_array_natives.z42` 覆盖。

use super::*;
use crate::gc::{GcMode, MagrGC};
use crate::metadata::types::{StructTypeLayout, STRUCT_REF_GCREF};
use crate::objops::error::{ARRAY_TYPE_MISMATCH_EXC, INVALID_CAST_EXC};
use crate::objops::Throw;
use std::sync::Arc;

/// `struct S { int N; object O; }`：`N` 打包在字节（offset 0），`O` 是引用叶子（offset 8）。
fn layout() -> Arc<StructTypeLayout> {
    Arc::new(StructTypeLayout {
        size: 16,
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([STRUCT_REF_GCREF]),
        fields: Box::new([]),
    })
}

fn struct_arr(ctx: &VmContext, elem: &str, len: usize) -> GcRef<ArrayObj> {
    let heap = ctx.heap();
    let a = ArrayObj::struct_backed(heap, ElemType::intern(elem), len, layout());
    let Value::Array(gc) = heap.alloc_array_obj(a) else { panic!("alloc") };
    gc
}

/// 元素 `i` ← `{ N = n, O = o }`。
fn put(arr: &GcRef<ArrayObj>, i: usize, n: i32, o: Value) {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&n.to_le_bytes());
    arr.borrow_mut().write_struct_elem(i, &bytes, &[o]);
}

fn n_of(arr: &GcRef<ArrayObj>, i: usize) -> i32 {
    let a = arr.borrow();
    let (b, _) = a.struct_elem(i).expect("struct[]");
    i32::from_le_bytes(b[..4].try_into().unwrap())
}

fn o_of(arr: &GcRef<ArrayObj>, i: usize) -> Value {
    arr.borrow().struct_elem(i).expect("struct[]").1[0]
}

fn marker(ctx: &VmContext, v: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(v)])
}

fn marker_val(v: &Value) -> i64 {
    let Value::Array(gc) = v else { panic!("expected a marker array, got {v:?}") };
    match gc.borrow().get_boxed(0) { Value::I64(n) => n, other => panic!("{other:?}") }
}

fn thrown(e: OpError) -> Throw {
    match e {
        OpError::Throw(t) => *t,
        other => panic!("expected a catchable exception, got {other:?}"),
    }
}

/// 源 `[0..len)` 元素 `{N = 10*i, O = marker(i)}`。
fn filled(ctx: &VmContext, elem: &str, len: usize) -> GcRef<ArrayObj> {
    let a = struct_arr(ctx, elem, len);
    for i in 0..len { put(&a, i, 10 * i as i32, marker(ctx, i as i64)); }
    a
}

#[test]
fn struct_copy_moves_bytes_and_reference_leaves() {
    let ctx = VmContext::new();
    let src = filled(&ctx, "Demo.S", 4);
    let dst = struct_arr(&ctx, "Demo.S", 4);
    copy_range(&ctx, &src, 1, &dst, 2, 2).expect("copy");
    assert_eq!(n_of(&dst, 0), 0);
    assert!(matches!(o_of(&dst, 1), Value::Null), "outside the range: untouched");
    assert_eq!(n_of(&dst, 2), 10);
    assert_eq!(n_of(&dst, 3), 20);
    assert_eq!(marker_val(&o_of(&dst, 2)), 1);
    assert_eq!(marker_val(&o_of(&dst, 3)), 2);
    // A reference leaf is copied shallowly (the same object), the bytes by value.
    let (Value::Array(a), Value::Array(b)) = (o_of(&src, 1), o_of(&dst, 2)) else { panic!() };
    assert!(GcRef::ptr_eq(&a, &b));
    put(&dst, 2, 99, Value::Null);
    assert_eq!(n_of(&src, 1), 10, "the source element is independent of the copy");
}

#[test]
fn struct_copy_within_one_array_is_a_memmove_both_ways() {
    let ctx = VmContext::new();
    let up = filled(&ctx, "Demo.S", 5);
    copy_range(&ctx, &up, 0, &up, 1, 4).expect("copy up");
    let got: Vec<(i32, i64)> = (0..5).map(|i| (n_of(&up, i), marker_val(&o_of(&up, i)))).collect();
    assert_eq!(got, vec![(0, 0), (0, 0), (10, 1), (20, 2), (30, 3)]);

    let down = filled(&ctx, "Demo.S", 5);
    copy_range(&ctx, &down, 2, &down, 0, 3).expect("copy down");
    let got: Vec<(i32, i64)> = (0..5).map(|i| (n_of(&down, i), marker_val(&o_of(&down, i)))).collect();
    assert_eq!(got, vec![(20, 2), (30, 3), (40, 4), (30, 3), (40, 4)]);
}

#[test]
fn different_struct_types_raise_array_type_mismatch_and_leave_the_destination() {
    let ctx = VmContext::new();
    let src = filled(&ctx, "Demo.S", 2);
    let dst = filled(&ctx, "Demo.Other", 2);
    let t = thrown(copy_range(&ctx, &src, 0, &dst, 0, 2).unwrap_err());
    assert_eq!(t.class, ARRAY_TYPE_MISMATCH_EXC);
    assert_eq!(t.msg, "cannot copy Demo.S[] elements into a Demo.Other[] array");
    assert_eq!(n_of(&dst, 1), 10, "destination untouched");
}

#[test]
fn struct_and_primitive_arrays_never_copy_into_each_other() {
    let ctx = VmContext::new();
    let s = filled(&ctx, "Demo.S", 2);
    let Value::Array(ints) = ctx.heap().alloc_array_typed("int", vec![Value::I64(7), Value::I64(8)]) else { panic!() };
    let t = thrown(copy_range(&ctx, &ints, 0, &s, 0, 2).unwrap_err());
    assert_eq!((t.class, t.msg.as_str()), (ARRAY_TYPE_MISMATCH_EXC, "cannot copy int[] elements into a Demo.S[] array"));
    let t = thrown(copy_range(&ctx, &s, 0, &ints, 0, 2).unwrap_err());
    assert_eq!(t.class, ARRAY_TYPE_MISMATCH_EXC);
    assert!(matches!(ints.borrow().get_boxed(0), Value::I64(7)));
}

/// 引用数组 → struct[]：任何一个元素不是该 struct 的装箱就整段拒绝（`InvalidCastException`），目的地不动。
#[test]
fn unboxing_a_non_struct_element_raises_invalid_cast_before_writing_anything() {
    let ctx = VmContext::new();
    let dst = filled(&ctx, "Demo.S", 2);
    for bad in [Value::Null, Value::Str("x".into()), Value::I64(1)] {
        let Value::Array(src) = ctx.heap().alloc_array(vec![bad, bad]) else { panic!() };
        let t = thrown(copy_range(&ctx, &src, 0, &dst, 0, 2).unwrap_err());
        assert_eq!(t.class, INVALID_CAST_EXC, "{bad:?}");
        assert_eq!(n_of(&dst, 1), 10);
    }
}

#[test]
fn copy_range_out_of_bounds_is_an_argument_exception() {
    let ctx = VmContext::new();
    let a = filled(&ctx, "Demo.S", 2);
    let b = struct_arr(&ctx, "Demo.S", 2);
    assert_eq!(thrown(copy_range(&ctx, &a, 1, &b, 0, 2).unwrap_err()).class, "Std.ArgumentException");
    assert_eq!(thrown(copy_range(&ctx, &a, 0, &a, 1, 2).unwrap_err()).class, "Std.ArgumentException");
}

// ── 写屏障：老的 struct[] 收到年轻的引用叶子 ─────────────────────────────────────

/// 一个已晋升（老年代）的 `Demo.S[4]`：钉住熬过足够多次 minor，再**解钉**——此后它不是根，minor 也不扫
/// 老条目，所以它的引用叶子只能靠卡表被 minor 看见（真实场景：老数组挂在老对象图上）。
fn old_struct_arr(ctx: &VmContext) -> GcRef<ArrayObj> {
    let dst = struct_arr(ctx, "Demo.S", 4);
    let pin = ctx.heap().pin_root(Value::Array(dst));
    for _ in 0..crate::gc::region::PROMOTION_THRESHOLD { ctx.heap().force_collect(); }
    assert!(GcRef::gen_age(&dst) >= crate::gc::region::PROMOTION_THRESHOLD, "setup: destination is old");
    ctx.heap().unpin_root(pin);
    dst
}

/// 老的 struct[] 经 `copy_range` 收到只有它引用的年轻叶子，跑一次 minor 后叶子是否还活着。
/// `barrier = false` 是阴性对照：同样的拷贝不发屏障。
fn young_leaves_survive_a_minor(barrier: bool) -> bool {
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
    let dst = old_struct_arr(&ctx);
    let weak = {
        let src = struct_arr(&ctx, "Demo.S", 4);
        let leaves: Vec<Value> = (0..4).map(|i| marker(&ctx, 100 + i)).collect();
        for (i, l) in leaves.iter().enumerate() { put(&src, i, i as i32, *l); }
        if barrier {
            copy_range(&ctx, &src, 0, &dst, 0, 4).expect("copy");
        } else {
            dst.borrow_mut().copy_elems_from(&src.borrow(), 0, 0, 4);
        }
        leaves.iter().map(|l| ctx.heap().make_weak(l).expect("array")).collect::<Vec<_>>()
    };
    ctx.heap().force_collect();   // generational ⇒ a minor; only `dst` (old, pinned) holds the leaves
    let alive = weak.iter().all(|w| ctx.heap().upgrade_weak(w).is_some());
    if alive {
        assert_eq!((0..4).map(|i| marker_val(&o_of(&dst, i))).collect::<Vec<_>>(), vec![100, 101, 102, 103]);
    }
    alive
}

#[test]
fn young_reference_leaves_copied_into_an_old_struct_array_survive_a_minor() {
    assert!(young_leaves_survive_a_minor(true));
}

#[test]
fn without_the_barrier_the_same_copy_loses_them() {
    assert!(!young_leaves_survive_a_minor(false), "control: the barrier is what keeps them");
}

/// 同一数组内的挪动也是写入：挪到老数组别的位置的年轻叶子同样要有卡。
#[test]
fn a_struct_copy_within_an_old_array_dirties_its_card() {
    let ctx = VmContext::new();
    ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
    let dst = old_struct_arr(&ctx);
    let young = marker(&ctx, 1);
    put(&dst, 0, 1, young);   // raw write: no card yet
    let Value::Array(owner) = Value::Array(dst) else { unreachable!() };
    // SAFETY: live handle from this heap.
    let (ci, _) = unsafe { owner.entry_ptr().as_ref() }.location();
    assert!(!ctx.heap().array_card_dirty_for_test(ci), "setup: card starts clean");
    copy_range(&ctx, &dst, 0, &dst, 2, 1).expect("copy");
    assert!(ctx.heap().array_card_dirty_for_test(ci), "the moved young leaf must dirty the card");
}

