//! Heap object writes and insertion barriers; no VM, frame or opcode dependency.
use crate::gc::Heap;
use super::{ArrRef, ObjRef, FieldAccess, FieldWrite, StructArrayElem, Value, is_ref_tag, prim_width};

/// Store a resolved heap field and complete its insertion barrier. SATB deletion
/// is performed by the storage primitive; callers retain their own error policy.
#[inline]
pub fn store_field_access(heap: &Heap, object: &ObjRef, access: FieldAccess, value: &Value) -> anyhow::Result<()> {
    let wrote = object.borrow().store_access(access, value)?;
    wrote.with_barrier_value(value, |stored| heap.write_barrier_field(&Value::Object(*object), access.offset as usize, stored));
    Ok(())
}

/// Store a heap field by slot; absent storage retains its no-op behavior.
pub fn store_field_slot(heap: &Heap, object: &ObjRef, slot: usize, value: &Value) -> anyhow::Result<()> {
    let Some(access) = object.borrow().field_access_of(slot) else { return Ok(()) };
    store_field_access(heap, object, access, value)
}

/// Store a heap struct leaf using the reference that actually reaches storage.
pub fn store_struct_leaf(heap: &Heap, object: &ObjRef, byte_off: u32, kind: u8, value: &Value) -> anyhow::Result<()> {
    let wrote = object.borrow().store_struct_leaf(byte_off, kind, value)?;
    wrote.with_barrier_value(value, |stored| heap.write_barrier_field(&Value::Object(*object), byte_off as usize, stored));
    Ok(())
}

/// Store a leaf in a struct array element, after execution has resolved its handle.
pub fn store_struct_element_leaf(heap: &Heap, element: &StructArrayElem, byte_off: u32, kind: u8, value: &Value) -> anyhow::Result<()> {
    let array = element.arr.borrow();
    let i = element.index as usize;
    if is_ref_tag(kind) {
        let wrote = array.set_struct_elem_ref(i, byte_off, value)?;
        wrote.with_barrier_value(value, |stored| heap.write_barrier_array_elem(&Value::Array(element.arr), i, stored));
        Ok(())
    } else {
        array.set_struct_elem_prim(i, byte_off, prim_width(kind)?, kind, value)
    }
}

/// Store a typed element and complete the insertion barrier. The caller has
/// checked bounds and the typed IL site's representation contract.
#[inline]
pub fn store_array_element(heap: &Heap, array: &ArrRef, index: usize, value: &Value) {
    let wrote = array.borrow().set_boxed(index, *value);
    barrier_array_element(heap, array, index, wrote, value);
}

/// Store an untyped element; representation rejection leaves storage unchanged.
/// Language-level assignability and index validation remain the caller's policy.
pub fn try_store_array_element(heap: &Heap, array: &ArrRef, index: usize, value: &Value) -> anyhow::Result<()> {
    let wrote = array.borrow().try_set_boxed(index, *value)?;
    barrier_array_element(heap, array, index, wrote, value);
    Ok(())
}

/// 元素写入**之后**的写屏障（卡表 / 并发 shade；SATB 删除屏障在写原语里）：落进单元的引用
/// （`wrote`——装不进 8 B 的值是它的盒子）。`ArraySet`、经 `ref` 写元素、`Std.Array.SetValue` 共用这一份。
/// 调用时数组的借用必须已经放掉。
///
/// **装箱 struct 写进 `struct[]` 是例外**：写入把盒子的 blob **拷进**数组，数组持有的是那些引用叶子、
/// 不是盒子。拿盒子判龄的话，老盒子、年轻叶子就漏了卡（叶子此后只靠数组可达，下一次 minor 把它扫掉），
/// 所以这种写入对元素的每个引用叶子发屏障（`barrier_array_range`）。
#[inline]
pub fn barrier_array_element(heap: &Heap, arr: &ArrRef, i: usize, wrote: FieldWrite, v: &Value) {
    if matches!(v, Value::BoxedStruct(_)) && arr.borrow().struct_layout().is_some() {
        return barrier_array_range(heap, arr, i, 1);
    }
    wrote.with_barrier_value(v, |s| heap.write_barrier_array_elem(&Value::Array(*arr), i, s));
}

/// 批量写入 `dst[di..di+n]` **之后**的写屏障：区间内每个引用槽（REF / MIXED / FROZEN 的元素、
/// struct[] 元素的每个引用叶子）里的堆引用各发一次 `write_barrier_array_elem`——槽里实际落下的引用，
/// 装箱的值给盒子（`ArrayObj::visit_elem_refs`；只收集真正的引用，`List<int>` 的扩容拷贝不分配）。
/// 并发模式要逐个染色新值，所以不能只发一次。调用时数组的借用必须已经放掉。
pub fn barrier_array_range(heap: &Heap, dst: &ArrRef, di: usize, n: usize) {
    let mut hits: Vec<(usize, Value)> = Vec::new();
    dst.borrow().visit_elem_refs(di, n, |i, v| hits.push((i, *v)));
    if hits.is_empty() {
        return;
    }
    let owner = Value::Array(*dst);
    for (i, v) in &hits {
        heap.write_barrier_array_elem(&owner, *i, v);
    }
}


#[cfg(all(test, z42_test_facade))]
#[path = "heap_access_tests.rs"]
mod heap_access_tests;
