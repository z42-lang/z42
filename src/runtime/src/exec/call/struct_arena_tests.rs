//! Unit tests for the value-struct byte arena (add-struct-value-semantics).
use super::*;
use crate::object::{StructTypeLayout, STRUCT_REF_ARC_STRING};
use std::sync::Arc;

/// Pure-primitive layout of `size` bytes (no reference leaves).
fn prim_layout(size: usize) -> Arc<StructTypeLayout> {
    Arc::new(StructTypeLayout { union_layout: None, float_field_ranges: Default::default(), size, ref_offsets: Box::new([]), ref_kinds: Box::new([]), fields: Box::new([]) })
}

#[test]
fn alloc_zero_initializes() {
    let mut a = StructArena::default();
    let idx = a.alloc(1, Arc::from("P"), prim_layout(8), |_, _| {});
    let all_zero = a.with(idx, 1, |s| s.bytes.iter().all(|&b| b == 0)).unwrap();
    assert!(all_zero);
    assert_eq!(a.allocs, 1);
}

#[test]
fn copy_into_produces_independent_blob() {
    let mut a = StructArena::default();
    let ty: Arc<str> = Arc::from("P");
    let src = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
    let dst = a.alloc(1, ty, prim_layout(8), |_, _| {});
    a.with_mut(src, 1, |s| s.bytes[0] = 42).unwrap();
    a.copy_slots(dst, 1, src, 1, Some(8)).unwrap();
    assert_eq!(a.with(dst, 1, |s| s.bytes[0]).unwrap(), 42);
    // Mutating the copy must not touch the source (value semantics at byte level).
    a.with_mut(dst, 1, |s| s.bytes[0] = 99).unwrap();
    assert_eq!(a.with(src, 1, |s| s.bytes[0]).unwrap(), 42);
    assert_eq!(a.with(dst, 1, |s| s.bytes[0]).unwrap(), 99);
}

#[test]
fn stale_or_out_of_range_handle_is_rejected() {
    let mut a = StructArena::default();
    let idx = a.alloc(1, Arc::from("P"), prim_layout(4), |_, _| {});
    assert!(a.with(idx, 2, |_| ()).is_err(), "wrong frame_id must error");
    assert!(a.with(999, 1, |_| ()).is_err(), "out-of-range idx must error");
    assert!(a.with(idx, 1, |_| ()).is_ok(), "correct handle resolves");
}

#[test]
fn truncate_frees_lifo() {
    let mut a = StructArena::default();
    let base = a.base();
    let _ = a.alloc(1, Arc::from("P"), prim_layout(4), |_, _| {});
    let _ = a.alloc(1, Arc::from("P"), prim_layout(4), |_, _| {});
    assert_eq!(a.base(), base + 2);
    a.truncate(base);
    assert_eq!(a.base(), base);
}

/// Reference-leaf value semantics: a `struct { s: string }` blob holds its string
/// in the `refs` side-slice. Copy clones the reference (independent), overwriting
/// one side's leaf leaves the other's intact, and the GC scan visits every leaf.
#[test]
fn ref_leaf_copy_is_independent_and_scanned() {
    let mut a = StructArena::default();
    // struct R { s: string @0 } → 16-byte blob, one reference leaf at offset 0.
    let layout = Arc::new(StructTypeLayout { union_layout: None, float_field_ranges: Default::default(),
        size: 16,
        ref_offsets: Box::new([0]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
        fields: Box::new([]),
    });
    let src = a.alloc(1, Arc::from("R"), layout.clone(), |_, _| {});
    let dst = a.alloc(1, Arc::from("R"), layout, |_, _| {});
    // src.s = "hi"; the leaf lives in `refs`, not in `bytes`.
    a.set_ref(src, 1, 0, Value::Str("hi".into())).unwrap();
    // dst = src  (StructCopy → clones the reference leaf).
    a.copy_slots(dst, 1, src, 1, Some(16)).unwrap();
    match a.get_ref(dst, 1, 0).unwrap() {
        Value::Str(s) => assert_eq!(&*s, "hi"),
        o => panic!("expected copied string, got {o:?}"),
    }
    // dst.s = "bye" → src.s must stay "hi" (independent reference slots).
    a.set_ref(dst, 1, 0, Value::Str("bye".into())).unwrap();
    match a.get_ref(src, 1, 0).unwrap() {
        Value::Str(s) => assert_eq!(&*s, "hi"),
        o => panic!("src ref leaf must be unchanged, got {o:?}"),
    }
    // GC root scan visits each live blob's reference leaf (2 total).
    let mut n = 0;
    a.scan_roots(&mut |_v| n += 1);
    assert_eq!(n, 2, "scan_roots must visit each live blob's reference leaf");
}

/// A reference-leaf write at an offset not in the layout is rejected (no silent
/// out-of-slice write).
#[test]
fn ref_leaf_bad_offset_errors() {
    let mut a = StructArena::default();
    let layout = Arc::new(StructTypeLayout { union_layout: None, float_field_ranges: Default::default(),
        size: 16,
        ref_offsets: Box::new([0]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
        fields: Box::new([]),
    });
    let idx = a.alloc(1, Arc::from("R"), layout, |_, _| {});
    assert!(a.set_ref(idx, 1, 8, Value::Null).is_err(), "unknown ref offset must error");
    assert!(a.get_ref(idx, 1, 8).is_err(), "unknown ref offset must error");
}

/// struct-copy-no-alloc: the in-place copy path must behave identically **in both
/// index directions**. `split_at_mut` splits at the higher index, so `src < dst`
/// and `dst < src` take two different branches — a bug in one of them would show
/// up only for one ordering of the two allocations.
#[test]
fn copy_into_works_in_both_index_directions() {
    let ty: Arc<str> = Arc::from("P");
    // src < dst
    {
        let mut a = StructArena::default();
        let src = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
        let dst = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
        assert!(src < dst);
        a.with_mut(src, 1, |s| s.bytes[0] = 7).unwrap();
        a.copy_slots(dst, 1, src, 1, Some(8)).unwrap();
        assert_eq!(a.with(dst, 1, |s| s.bytes[0]).unwrap(), 7, "src<dst must copy");
    }
    // dst < src  (the other `split_at_mut` branch)
    {
        let mut a = StructArena::default();
        let dst = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
        let src = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
        assert!(dst < src);
        a.with_mut(src, 1, |s| s.bytes[0] = 9).unwrap();
        a.copy_slots(dst, 1, src, 1, Some(8)).unwrap();
        assert_eq!(a.with(dst, 1, |s| s.bytes[0]).unwrap(), 9, "dst<src must copy");
    }
}

/// struct-copy-no-alloc: self-copy (`a = a`, or two handles onto one slot) is a
/// no-op — `split_at_mut` cannot hand out two borrows of one element, so this case
/// returns early. **Validation still runs**: a stale handle must be rejected even
/// though there is nothing to copy (the old snapshot-based code validated both
/// sides, and dropping that would turn a stale-handle bug into silent success).
#[test]
fn copy_into_self_is_noop_but_still_validated() {
    let mut a = StructArena::default();
    let idx = a.alloc(1, Arc::from("P"), prim_layout(8), |_, _| {});
    a.with_mut(idx, 1, |s| s.bytes[0] = 5).unwrap();

    a.copy_slots(idx, 1, idx, 1, Some(8)).unwrap();
    assert_eq!(a.with(idx, 1, |s| s.bytes[0]).unwrap(), 5, "self-copy must not disturb the blob");

    assert!(a.copy_slots(idx, 2, idx, 2, Some(8)).is_err(), "stale frame_id must still be rejected");
    assert!(a.copy_slots(999, 1, 999, 1, Some(8)).is_err(), "out-of-range idx must still be rejected");
}

/// struct-copy-no-alloc: a **stale dst** must be rejected even when src is fine,
/// and vice versa. Both checks happen before the split; losing either one would be
/// invisible to the happy-path tests.
#[test]
fn copy_into_rejects_either_side_stale() {
    let mut a = StructArena::default();
    let ty: Arc<str> = Arc::from("P");
    let src = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
    let dst = a.alloc(1, ty, prim_layout(8), |_, _| {});
    assert!(a.copy_slots(dst, 2, src, 1, Some(8)).is_err(), "stale dst frame_id must error");
    assert!(a.copy_slots(dst, 1, src, 2, Some(8)).is_err(), "stale src frame_id must error");
    assert!(a.copy_slots(dst, 1, 999, 1, Some(8)).is_err(), "out-of-range src must error");
    assert!(a.copy_slots(999, 1, src, 1, Some(8)).is_err(), "out-of-range dst must error");
    assert!(a.copy_slots(dst, 1, src, 1, Some(8)).is_ok(), "valid handles still copy");
}

// ─── check-struct-copy-shape-invariant：正面对照 ─────────────────────────────
//
// `StructCopy` 的不变式是「两个 blob 同类型」，而复制本身只用 `min` 兜 —— 破坏时
// 会静默截断出半个 struct。新增的 `check_copy_invariant` 把它变成 debug 下的错误。
//
// ⚠️ 这三个测试是**这道门唯一的正面对照**。没有它们，门是否还接在 `copy_slots` 上
// 无人知晓 —— 真实程序里不变式**永远成立**（编译器只在同类型之间发 `StructCopy`），
// 所以「全仓零命中」既是期望结果、也正因此不能证明门还活着。
//
// 门只在 debug 存在（release 沿用 `min`，理由见 `check_copy_invariant` 的文档：
// 用户写不出能走到布局偏斜的 z42），故整组 `#[cfg(debug_assertions)]`。

#[cfg(debug_assertions)]
#[test]
fn copy_into_rejects_blobs_of_different_byte_size() {
    let mut a = StructArena::default();
    let src = a.alloc(1, Arc::from("Small"), prim_layout(8), |_, _| {});
    let dst = a.alloc(1, Arc::from("Big"), prim_layout(16), |_, _| {});
    let err = a.copy_slots(dst, 1, src, 1, Some(16)).unwrap_err().to_string();
    assert!(err.contains("different shape"), "unexpected error: {err}");
    // 诊断必须点出两个类型名与各自尺寸 —— 否则定位不到是谁造出了偏斜的布局。
    assert!(err.contains("Small") && err.contains("Big"), "error names neither type: {err}");
    assert!(err.contains("8") && err.contains("16"), "error omits the sizes: {err}");
}

#[cfg(debug_assertions)]
#[test]
fn copy_into_rejects_blobs_with_different_reference_slot_counts() {
    let mut a = StructArena::default();
    // 同字节宽、引用槽数不同 —— 只比字节长度的门会漏掉这一格。
    let refful = Arc::new(StructTypeLayout { union_layout: None, float_field_ranges: Default::default(),
        size: 8,
        ref_offsets: Box::new([0]),
        ref_kinds: Box::new([STRUCT_REF_ARC_STRING]),
        fields: Box::new([]),
    });
    let src = a.alloc(1, Arc::from("HasRef"), refful, |_, _| {});
    let dst = a.alloc(1, Arc::from("NoRef"), prim_layout(8), |_, _| {});
    let err = a.copy_slots(dst, 1, src, 1, Some(8)).unwrap_err().to_string();
    assert!(err.contains("different shape"), "unexpected error: {err}");
    assert!(err.contains("ref(s)"), "error should report the reference-slot counts: {err}");
}

#[cfg(debug_assertions)]
#[test]
fn copy_into_rejects_a_compiler_size_that_disagrees_with_the_layout() {
    let mut a = StructArena::default();
    let ty: Arc<str> = Arc::from("P");
    let src = a.alloc(1, ty.clone(), prim_layout(8), |_, _| {});
    let dst = a.alloc(1, ty, prim_layout(8), |_, _| {});
    // 两个 blob 形状一致，但指令里编码的 size 与布局不符 —— 这是编译器与运行期
    // 对同一个类型的大小有分歧（instantiated-generic 布局分裂就是这个形态）。
    let err = a.copy_slots(dst, 1, src, 1, Some(12)).unwrap_err().to_string();
    assert!(err.contains("size skew"), "unexpected error: {err}");
    assert!(err.contains("12") && err.contains("8"), "error must show both numbers: {err}");
    // 阴性对照：形状一致且 size 相符时必须照常放行（门不能恒响）。
    assert!(a.copy_slots(dst, 1, src, 1, Some(8)).is_ok(), "a matching copy must still succeed");
    // 自拷（`a = a`）走的是 `src_idx == dst_idx` 的提前返回 —— 门刻意放在那之前，
    // 所以偏斜的 size 在自拷上也照样被抓住，而不是被提前返回绕过。
    let e2 = a.copy_slots(src, 1, src, 1, Some(12)).unwrap_err().to_string();
    assert!(e2.contains("size skew"), "self-copy must not bypass the guard: {e2}");
    assert!(a.copy_slots(src, 1, src, 1, Some(8)).is_ok(), "a matching self-copy is still a no-op");
}

#[test]
fn dynamic_copy_keeps_reference_leaves_after_the_callee_frame_exits() {
    use crate::exec::{
        frames::VmFrame, ops::object::struct_leaf::{as_struct_ref, struct_field_get_val, struct_field_set_val},
        push_frame, stack::stack_tests::test_function,
    };
    use crate::object::{STRUCT_REF_GCREF, TAG_ARRAY, TAG_I32};
    use crate::vm::VmContext;

    let ctx = VmContext::new();
    let function = test_function("struct_return", &[], Vec::new());
    let regs = Vec::new();
    let collect = || {
        let _pause = crate::threads::safepoint::request_gc_pause(&ctx).unwrap();
        ctx.heap().collect_full();
    };
    let layout = || Arc::new(StructTypeLayout {
        size: 16,
        ref_offsets: Box::new([8]),
        ref_kinds: Box::new([STRUCT_REF_GCREF]),
        ..Default::default()
    });

    push_frame(&ctx, VmFrame::new(&function, &regs));
    let caller_id = ctx.exec().next_frame_id();
    let destination = ctx.exec().alloc_struct(caller_id, Arc::from("Record"), layout());
    let dst = as_struct_ref(&destination, "caller struct").unwrap();

    push_frame(&ctx, VmFrame::new(&function, &regs));
    let callee_id = ctx.exec().next_frame_id();
    let source = ctx.exec().alloc_struct(callee_id, Arc::from("Record"), layout());
    let src = as_struct_ref(&source, "callee struct").unwrap();
    let reference = ctx.heap().alloc_array(vec![Value::I64(9)]);
    let weak = ctx.heap().make_weak(&reference).unwrap();
    struct_field_set_val(&ctx, &source, 0, TAG_I32, &Value::I64(42)).unwrap();
    struct_field_set_val(&ctx, &source, 8, TAG_ARRAY, &reference).unwrap();
    let (name, bytes, refs) = ctx.exec().snapshot_struct(src.0, src.1).unwrap();
    ctx.exec().copy_struct_slots(dst, src, None).unwrap();

    ctx.exec().pop_frame();
    assert!(ctx.exec().snapshot_struct(src.0, src.1).is_err());
    assert!(ctx.exec().copy_struct_slots(dst, src, None).is_err());
    assert_eq!(&*name, "Record");
    assert_eq!(&bytes[..4], &42_i32.to_le_bytes());
    assert_eq!(refs, vec![reference]);
    assert_eq!(struct_field_get_val(&ctx, &destination, 0, TAG_I32).unwrap(), Value::I64(42));
    assert_eq!(struct_field_get_val(&ctx, &destination, 8, TAG_ARRAY).unwrap(), reference);
    collect();
    assert!(ctx.heap().upgrade_weak(&weak).is_some());

    ctx.exec().pop_frame();
    collect();
    assert!(ctx.heap().upgrade_weak(&weak).is_none());
}

#[test]
fn dynamic_copy_rejects_same_size_slots_of_different_types_or_layouts() {
    let mut arena = StructArena::default();
    let destination = arena.alloc(1, Arc::from("Record"), prim_layout(16), |_, _| {});
    let other_type = arena.alloc(1, Arc::from("Other"), prim_layout(16), |_, _| {});
    let other_layout = arena.alloc(1, Arc::from("Record"), Arc::new(StructTypeLayout { union_layout: None, float_field_ranges: Default::default(),
        size: 16,
        fields: Box::new([crate::object::StructFieldLayout { offset: 0, size: 8, kind: 0 }]),
        ..Default::default()
    }), |_, _| {});
    for source in [other_type, other_layout] {
        arena.with_mut(source, 1, |slot| slot.bytes[0] = 99).unwrap();
        assert!(arena.copy_slots(destination, 1, source, 1, None).is_err());
        assert_eq!(arena.with(destination, 1, |slot| slot.bytes[0]).unwrap(), 0);
    }
}
