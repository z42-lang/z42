use super::*;
use crate::gc::GcRef;
use crate::metadata::Value;
use crate::vm_context::VmContext;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

#[test]
fn clone_primitives_independent() {
    let ctx = ctx();
    let original = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![Value::I64(1), Value::I64(2), Value::I64(3)])));
    let cloned = builtin_array_clone(&ctx, std::slice::from_ref(&original)).expect("clone ok");

    let (orig_rc, copy_rc) = match (&original, &cloned) {
        (Value::Array(o), Value::Array(c)) => (o, c),
        _ => panic!("expected arrays"),
    };
    assert!(!GcRef::ptr_eq(orig_rc, copy_rc), "clone returns a distinct array reference");
    assert_eq!(copy_rc.borrow().len(), 3);

    copy_rc.borrow_mut().set_boxed(0, Value::I64(99));
    assert!(matches!(orig_rc.borrow().get_boxed(0), Value::I64(1)));
    assert!(matches!(copy_rc.borrow().get_boxed(0), Value::I64(99)));
}

#[test]
fn clone_shares_reference_elements() {
    let ctx = ctx();
    let inner = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![Value::I64(7)])));
    let original = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(vec![inner.clone()])));
    let cloned = builtin_array_clone(&ctx, std::slice::from_ref(&original)).expect("clone ok");

    let (orig_rc, copy_rc) = match (&original, &cloned) {
        (Value::Array(o), Value::Array(c)) => (o, c),
        _ => panic!("expected arrays"),
    };
    let orig_inner = orig_rc.borrow().get_boxed(0).clone();
    let copy_inner = copy_rc.borrow().get_boxed(0).clone();
    match (orig_inner, copy_inner) {
        (Value::Array(a), Value::Array(b)) => assert!(GcRef::ptr_eq(&a, &b),
            "shallow clone shares reference-type elements"),
        _ => panic!("expected nested arrays"),
    }
}

#[test]
fn clone_empty_array() {
    let ctx = ctx();
    let empty = Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(Vec::new())));
    let cloned = builtin_array_clone(&ctx, std::slice::from_ref(&empty)).expect("clone ok");

    let (orig_rc, copy_rc) = match (&empty, &cloned) {
        (Value::Array(o), Value::Array(c)) => (o, c),
        _ => panic!("expected arrays"),
    };
    assert_eq!(copy_rc.borrow().len(), 0);
    assert!(!GcRef::ptr_eq(orig_rc, copy_rc));
}

#[test]
fn clone_rejects_non_array() {
    let ctx = ctx();
    let err = builtin_array_clone(&ctx, &[Value::I64(42)]).unwrap_err();
    assert!(err.to_string().contains("expected an array"));
}

#[test]
fn clone_rejects_null() {
    let ctx = ctx();
    let err = builtin_array_clone(&ctx, &[Value::Null]).unwrap_err();
    assert!(err.to_string().contains("null array"));
}

// ── __array_copy (perf-bulk-array-copy) ──────────────────────────────────────

fn ints(v: &[i64]) -> Value {
    Value::Array(GcRef::new(crate::metadata::types::ArrayObj::new_leaked(
        v.iter().map(|n| Value::I64(*n)).collect(),
    )))
}

fn read(v: &Value) -> Vec<i64> {
    match v {
        Value::Array(rc) => {
            let a = rc.borrow();
            (0..a.len())
                .map(|i| match a.get_boxed(i) {
                    Value::I64(n) => n,
                    other => panic!("expected I64, got {other:?}"),
                })
                .collect()
        }
        other => panic!("expected array, got {other:?}"),
    }
}

fn copy(ctx: &VmContext, src: &Value, si: i64, dst: &Value, di: i64, n: i64) -> Result<Value> {
    builtin_array_copy(
        ctx,
        &[src.clone(), Value::I64(si), dst.clone(), Value::I64(di), Value::I64(n)],
    )
}

#[test]
fn copy_between_arrays_moves_range() {
    let ctx = ctx();
    let src = ints(&[1, 2, 3, 4, 5]);
    let dst = ints(&[0, 0, 0, 0, 0]);
    copy(&ctx, &src, 1, &dst, 2, 3).expect("copy ok");
    assert_eq!(read(&dst), vec![0, 0, 2, 3, 4]);
    assert_eq!(read(&src), vec![1, 2, 3, 4, 5], "source untouched");
}

#[test]
fn copy_zero_length_is_a_noop() {
    let ctx = ctx();
    let src = ints(&[1, 2, 3]);
    let dst = ints(&[9, 9, 9]);
    // len 0 must not even bounds-check the (otherwise out-of-range) indices.
    copy(&ctx, &src, 3, &dst, 3, 0).expect("zero-length copy ok");
    assert_eq!(read(&dst), vec![9, 9, 9]);
}

#[test]
fn copy_within_same_array_overlapping_forward() {
    let ctx = ctx();
    // dst above src → must copy backward, else the tail clobbers unread elements.
    let a = ints(&[1, 2, 3, 4, 5]);
    copy(&ctx, &a, 0, &a, 2, 3).expect("self copy ok");
    assert_eq!(read(&a), vec![1, 2, 1, 2, 3]);
}

#[test]
fn copy_within_same_array_overlapping_backward() {
    let ctx = ctx();
    let a = ints(&[1, 2, 3, 4, 5]);
    copy(&ctx, &a, 2, &a, 0, 3).expect("self copy ok");
    assert_eq!(read(&a), vec![3, 4, 5, 4, 5]);
}

#[test]
fn copy_within_same_array_same_index_is_a_noop() {
    let ctx = ctx();
    let a = ints(&[1, 2, 3]);
    copy(&ctx, &a, 1, &a, 1, 2).expect("self copy ok");
    assert_eq!(read(&a), vec![1, 2, 3]);
}

#[test]
fn copy_out_of_bounds_errors() {
    let ctx = ctx();
    let src = ints(&[1, 2, 3]);
    let dst = ints(&[0, 0]);
    assert!(copy(&ctx, &src, 0, &dst, 0, 3).is_err(), "destination too short");
    assert!(copy(&ctx, &src, 2, &dst, 0, 2).is_err(), "source range past end");
    let same = ints(&[1, 2, 3]);
    assert!(copy(&ctx, &same, 1, &same, 0, 3).is_err(), "same-array range past end");
}

#[test]
fn copy_rejects_non_arrays_and_negative_indices() {
    let ctx = ctx();
    let src = ints(&[1, 2]);
    let dst = ints(&[0, 0]);
    assert!(builtin_array_copy(&ctx, &[Value::Null, Value::I64(0), dst.clone(), Value::I64(0), Value::I64(1)]).is_err());
    assert!(copy(&ctx, &src, -1, &dst, 0, 1).is_err());
    assert!(copy(&ctx, &src, 0, &dst, 0, -1).is_err());
}

// ── fix-missing-array-write-barriers (2026-09-10) ────────────────────────────

/// Storing a reference into an array is a heap write, and an **old** array receiving a
/// **young** element must dirty its card or the next minor will not re-root it.
///
/// The interpreter's `ArraySet` and the JIT's array-store helper have always fired the
/// barrier; these `Std.Array` builtins never did. `perf-bulk-array-copy` is the sharpest
/// case: it replaced a script-side `for` loop of `ArraySet` — every iteration of which fired
/// the barrier — with one primitive that fired none, while its own doc comment claims "a copy
/// is indistinguishable from the loop it replaces".
mod write_barriers {
    use super::*;
    use crate::gc::{GcMode, MagrGC};

    /// Allocate an array through the heap (so it has a region entry with a card), and age it
    /// past the promotion threshold so a young element written into it is a cross-gen edge.
    fn old_array(ctx: &VmContext, len: usize) -> Value {
        let v = ctx.heap().alloc_array(vec![Value::Null; len]);
        let Value::Array(gc) = &v else { panic!() };
        for _ in 0..crate::gc::region::PROMOTION_THRESHOLD {
            // SAFETY: fresh entry from this heap; ageing it is what a surviving minor does.
            let e = unsafe { gc.entry_ptr().as_ref() };
            e.gen_age.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        v
    }

    fn card_dirty(ctx: &VmContext, arr: &Value) -> bool {
        let Value::Array(gc) = arr else { panic!() };
        // SAFETY: live handle from this heap.
        let (ci, _) = unsafe { gc.entry_ptr().as_ref() }.location;
        ctx.heap().array_card_dirty_for_test(ci)
    }

    #[test]
    fn array_set_value_dirties_the_card() {
        let ctx = ctx();
        ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
        let dst = old_array(&ctx, 4);
        let young = ctx.heap().alloc_array(vec![Value::I64(7)]);
        assert!(!card_dirty(&ctx, &dst), "test setup: card starts clean");

        builtin_array_set(&ctx, &[dst.clone(), young, Value::I64(0)]).expect("set ok");

        assert!(card_dirty(&ctx, &dst), "Array.SetValue must fire the write barrier");
    }

    #[test]
    fn array_copy_dirties_the_card() {
        let ctx = ctx();
        ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
        let dst = old_array(&ctx, 4);
        let src = ctx.heap().alloc_array(vec![Value::Null; 4]);
        {
            let Value::Array(s) = &src else { panic!() };
            let young = ctx.heap().alloc_array(vec![Value::I64(1)]);
            s.borrow_mut().set_boxed(0, young);
        }
        assert!(!card_dirty(&ctx, &dst), "test setup: card starts clean");

        builtin_array_copy(
            &ctx,
            &[src, Value::I64(0), dst.clone(), Value::I64(0), Value::I64(4)],
        )
        .expect("copy ok");

        assert!(card_dirty(&ctx, &dst), "Array.Copy must fire the write barrier");
    }

    /// A copy that moves no references must not dirty anything — the barrier is precise, not
    /// a blanket "this array was written to".
    #[test]
    fn a_primitive_only_copy_dirties_nothing() {
        let ctx = ctx();
        ctx.heap().set_mode(GcMode::GenerationalMarkSweep);
        let dst = old_array(&ctx, 4);
        let src = ctx.heap().alloc_array(vec![Value::I64(1); 4]);

        builtin_array_copy(
            &ctx,
            &[src, Value::I64(0), dst.clone(), Value::I64(0), Value::I64(4)],
        )
        .expect("copy ok");

        assert!(!card_dirty(&ctx, &dst), "a primitive copy has no cross-gen edge to record");
    }
}

// ── fix-silent-array-elem-zero (2026-09-27) ─────────────────────────────────
//
// 无类型数组写入的类型不符**不再静默存 0**。实测的修前行为：`int[0]` 原值 9，
// `a.SetValue(objNull, 0)` 在 **release** 下静默把它变成 **0**、不抛、报成功；
// `Array.CopyRange(string[], 0, int[], 0, 1)` 同样静默清零。debug 会 panic ⇒
// 这是 release-only 的静默数据损坏，而 `0` 与合法写入**完全无法区分**。
//
// 分界线是「这是谁的错」：IR 的 `ArraySet` 那条路上编译器该先转换（原 debug-only 策略
// 依然正确、一个字没动）；而 `SetValue(Object value, int index)` 的形参声明就是 `Object`、
// `CopyRange` 两侧元素类型可不同 —— **没有编译器站点能转换它们**，值是用户给的。

fn int_arr(ctx: &VmContext, v: i64) -> Value {
    ctx.heap().alloc_array_typed("int", vec![Value::I64(v)])
}

fn elem0(v: &Value) -> Value {
    let Value::Array(gc) = v else { panic!("expected an array") };
    gc.borrow().get_boxed(0)
}

/// 🔴 正例：类型不符必须报错，**且不得改动元素**。
#[test]
fn set_value_rejects_kind_mismatch_instead_of_zeroing() {
    let ctx = ctx();
    for bad in [Value::Null, Value::Str("not a number".into()), Value::F64(1.5)] {
        let a = int_arr(&ctx, 9);
        let r = builtin_array_set(&ctx, &[a.clone(), bad.clone(), Value::I64(0)]);
        assert!(r.is_err(), "int[] 收 {bad:?} 必须报错（静默存 0 是回归）");
        assert!(
            matches!(elem0(&a), Value::I64(9)),
            "报错之后元素必须保持旧值 9，实际 {:?}（被清零即回归）", elem0(&a)
        );
    }
}

/// 回归门：`int[] <- I64` 是**正常路径**（z42 整数在 IR 里一律 i64、`int[]` 存 i32）。
#[test]
fn set_value_keeps_the_normal_integer_path() {
    let ctx = ctx();
    let a = int_arr(&ctx, 9);
    builtin_array_set(&ctx, &[a.clone(), Value::I64(42), Value::I64(0)]).expect("正常整数必须写得进");
    assert!(matches!(elem0(&a), Value::I64(42)));
}

/// 裁决记录（**严格**，不做拓宽）：`double[] <- 整数` 报错。
///
/// ⚠️ 这是**刻意**的，不是漏了拓宽：C# 会拓宽、z42 编译器自己也允许 `int → double`，
/// 但本刀选了「只许同种」——判据无歧义，且严格版随时能放宽、反过来不行。
/// 修前这一格是**静默把 7.5 变成 0**，那是三种结果里最坏的。
#[test]
fn set_value_is_strict_no_widening() {
    let ctx = ctx();
    let d = ctx.heap().alloc_array_typed("double", vec![Value::F64(7.5)]);
    let r = builtin_array_set(&ctx, &[d.clone(), Value::I64(42), Value::I64(0)]);
    assert!(r.is_err(), "严格口径：double[] 不收整数");
    assert!(matches!(elem0(&d), Value::F64(f) if f == 7.5), "原值不得被动");
    // 同种照旧
    builtin_array_set(&ctx, &[d.clone(), Value::F64(1.25), Value::I64(0)]).expect("double[] 收 F64");
    assert!(matches!(elem0(&d), Value::F64(f) if f == 1.25));
}

/// 回归门：**引用 backing 写 `Null` 完全合法** —— 不钉这条，「把整条路堵死」也会让正例变绿。
#[test]
fn set_value_reference_backing_still_accepts_null() {
    let ctx = ctx();
    let s = ctx.heap().alloc_array_typed("string", vec![Value::Str("x".into())]);
    builtin_array_set(&ctx, &[s.clone(), Value::Null, Value::I64(0)]).expect("引用元素写 null 合法");
    assert!(matches!(elem0(&s), Value::Null));
}

/// 🔴 正例：`CopyRange` 跨不兼容元素类型必须报错。
#[test]
fn copy_across_incompatible_element_types_raises() {
    let ctx = ctx();
    let si = ctx.heap().alloc_array_typed("string", vec![Value::Str("x".into())]);
    let di = int_arr(&ctx, 9);
    let r = builtin_array_copy(&ctx, &[si, Value::I64(0), di.clone(), Value::I64(0), Value::I64(1)]);
    assert!(r.is_err(), "string[] → int[] 必须报错");
    assert!(matches!(elem0(&di), Value::I64(9)), "目的地不得被清零");
}

/// 回归门：同元素类型的拷贝走**零成本快路**（`perf-bulk-array-copy` 的理由不受影响）。
#[test]
fn copy_same_element_type_is_unaffected() {
    let ctx = ctx();
    let a = int_arr(&ctx, 5);
    let b = int_arr(&ctx, 0);
    builtin_array_copy(&ctx, &[a, Value::I64(0), b.clone(), Value::I64(0), Value::I64(1)])
        .expect("同型拷贝照旧");
    assert!(matches!(elem0(&b), Value::I64(5)));
}

/// 🔴 **防漂移**：`prim_backing_accepts` 必须与 `set_boxed` 每个臂里的 `if let Value::X` 一致。
///
/// 判据复制是本仓反复出问题的形状（结构审计 R2）。本测试逐格核对
/// 「`accepts` 说能存」⇒「`set_boxed` 真的忠实存了」。探针值刻意**都不是零值**，
/// 所以「读回不是零」就等价于「忠实存了」，无歧义。
///
/// ⚠️ 反方向（`accepts` 说不能 ⇒ `set_boxed` 会存 0）**这里不断言**：`set_boxed` 在不符时
/// 走 `debug_assert!(false)`，debug 下会 panic。那个方向由上面几条行为测试间接覆盖
/// （若 `accepts` 误判为 false，正例会变红）。
#[test]
fn prim_backing_accepts_agrees_with_set_boxed() {
    let ctx = ctx();
    let probes = [
        Value::I64(7), Value::F64(2.5), Value::Bool(true),
        Value::Char('q'), Value::Str("s".into()), Value::Null,
    ];
    // ⚠️ 播种值必须**按元素类型**给：`alloc_array_typed` 走 `pack_backing`，它与 `set_boxed`
    // 共用同一套「类型不符」信号 ⇒ 拿 `I64(0)` 去播 `double[]` 会在**构造期**就 debug-panic。
    // （第一版就这么写的，被这条测试自己抓了出来 —— 说明这套信号是有效的。）
    for (ty, zero) in [
        ("int",    Value::I64(0)),
        ("long",   Value::I64(0)),
        ("byte",   Value::I64(0)),
        ("double", Value::F64(0.0)),
        ("bool",   Value::Bool(false)),
        ("char",   Value::Char('\0')),
        ("string", Value::Null),
    ] {
        for p in &probes {
            let arr = ctx.heap().alloc_array_typed(ty, vec![zero.clone()]);
            let Value::Array(gc) = &arr else { panic!() };
            if !gc.borrow().prim_backing_accepts(p) {
                continue;   // 见上：反方向在 debug 下会 panic，不在这里断言
            }
            gc.borrow_mut().set_boxed(0, p.clone());
            let back = gc.borrow().get_boxed(0);
            let faithful = match (p, &back) {
                (Value::I64(a), Value::I64(b)) => a == b,
                (Value::F64(a), Value::F64(b)) => a == b,
                (Value::Bool(a), Value::Bool(b)) => a == b,
                (Value::Char(a), Value::Char(b)) => a == b,
                (Value::Str(a), Value::Str(b)) => a == b,
                (Value::Null, Value::Null) => true,
                _ => false,
            };
            assert!(
                faithful,
                "{ty}[] 的 prim_backing_accepts 说能存 {p:?}，但 set_boxed 存成了 {back:?} \
                 —— 两份判据漂移了"
            );
        }
    }
}
