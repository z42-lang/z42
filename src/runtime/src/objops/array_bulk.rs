//! `Std.Array` 的无类型 / 批量元素操作：`CopyRange`（`__array_copy`）、`GetValue` 的元素读取、
//! `SetValue` 的值 struct 元素校验，以及批量写入后的写屏障。corelib 只解析参数、把 [`OpError`] 抛给用户。
//!
//! 元素可以是任意模式。值 struct 数组（STRUCT：元素 blob 紧凑打包，引用叶子是 8 B 引用字）
//! 的读不能走 `get_boxed`——它没有堆、装不了箱——所以这里按两侧 backing 选一条路：
//!
//! | 源 → 目的 | 做法 | 类型不符 |
//! |---|---|---|
//! | `T[]` → `T[]`（同一 struct） | 逐元素拷 blob，引用叶子经引用字写原语（同数组按 memmove 方向） | 元素类型不同 → `ArrayTypeMismatchException` |
//! | struct[] → 引用数组（`object[]` / 接口数组） | 逐元素装箱 | 目的元素类型装不下这个 struct → `ArrayTypeMismatchException` |
//! | 引用数组 → struct[] | 先逐元素校验是该 struct 的装箱，再逐元素拆进去 | 数组类型不兼容 → `ArrayTypeMismatchException`；某个元素不是（含 null）→ `InvalidCastException` |
//! | struct[] ↔ 基元数组 | — | `ArrayTypeMismatchException` |
//! | 引用数组窄化或接口转换 | 校验并拷贝同一份值快照 | 类型不兼容或元素不可转换 |
//! | 其余（基元 / 引用之间） | `ArrayObj::copy_elems_from`（同种 PRIM backing 一次切片拷贝，REF → REF 逐字） | 元素种类不符 → `InvalidCastException` |
//!
//! 抛异常时目的数组不动。写入之后对写入区间里**每个引用槽**（REF / MIXED / FROZEN 的元素、struct[] 元素的每个
//! 引用叶子）里的堆引用发 `write_barrier_array_elem`；覆盖前的旧引用由各写入原语交给 SATB。

use crate::gc::{Heap, RootHandle};
use crate::metadata::index::type_names::{builtin_fqn, erased};
use crate::object::ArrRef;
use crate::vm::exec::ops::cast::{bound_param_extra_isa, isa_object, isa_td, is_subclass_or_eq_td};
use crate::vm::runtime::dispatch::{
    isa::{array_isa, is_open_type_arg, is_open_type_arg_name},
    receiver::{prim_isa, primitive_class_name},
};
use crate::vm::runtime::ids::tokens::TypeKeyCell;
use crate::object::{ArrayMode, ArrayObj, ElemType};
use crate::object::Value;
use crate::vm_context::VmContext;

use super::error::{OpError, OpResult, INVALID_CAST_EXC};

/// `Array.CopyRange(src, si, dst, di, n)`：`src[si..si+n]` → `dst[di..di+n]`，同一数组内重叠按 memmove。
pub fn copy_range(
    ctx: &VmContext, src: &ArrRef, si: usize, dst: &ArrRef, di: usize, n: usize,
) -> OpResult<()> {
    if n == 0 {
        return Ok(());
    }
    // 同一数组：元素类型必然相同，按 memmove 方向在数组内移动。
    if ArrRef::ptr_eq(src, dst) {
        {
            let a = dst.borrow();
            let len = a.len();
            if n > len || si > len - n || di > len - n {
                return Err(OpError::copy_range_out_of_bounds(len, si, len, di, n));
            }
            a.copy_elems_within(si, di, n);
        }
        barrier_after_range_store(ctx, dst, di, n);
        return Ok(());
    }
    let (s, d) = (Shape::of(&src.borrow()), Shape::of(&dst.borrow()));
    if n > s.len || n > d.len || si > s.len - n || di > d.len - n {
        return Err(OpError::copy_range_out_of_bounds(s.len, si, d.len, di, n));
    }
    // 类型判定会查类型表（可能触发惰性加载）。数组没有锁（对象模型 R4）：两条线程同时互相拷贝
    // 两个数组只是逐元素竞争，没有取锁顺序问题。
    match plan(ctx, &s, &d)? {
        Plan::Direct => dst.borrow().copy_elems_from(&src.borrow(), si, di, n),
        Plan::Box => box_into(ctx, src, &s, si, dst, di, n)?,
        Plan::Unbox => return copy_checked(ctx, src, si, dst, di, n, |v| check_struct_box(&d.elem, v)),
        Plan::CheckedPrimitive => {
            let array = dst.borrow();
            return copy_checked(ctx, src, si, dst, di, n, |v| check_untyped_store(ctx, &array, v));
        }
        Plan::Checked => {
            let key = TypeKeyCell::new();
            let target = type_name(&d.elem);
            return copy_checked(ctx, src, si, dst, di, n, |v| check_reference(ctx, target, v, &key));
        }
    }
    barrier_after_range_store(ctx, dst, di, n);
    Ok(())
}

/// `Array.GetValue(i)` 的元素：值 struct 数组的元素装箱成一份快照（独立于数组），其余同 `get_boxed`。
/// 调用方已检查下标。
pub fn elem_get_boxed(ctx: &VmContext, arr: &ArrRef, i: usize) -> OpResult<Value> {
    let (et, elem) = {
        let a = arr.borrow();
        match a.struct_elem(i) {
            None => return Ok(a.get_boxed(i)),
            Some(blob) => (a.element_type(), blob),
        }
    };
    box_struct(ctx, et, &elem.0, &elem.1)
}

/// 无类型写入（`Array.SetValue`）前的校验，不符 → `InvalidCastException`（同 C#），数组不动：
/// 值 struct 数组只收该 struct 的装箱（null 也不行）；基元数组只收同种值（`prim_backing_accepts`，
/// 不拓宽）；引用数组按声明的元素类型校验。
pub fn check_untyped_store(ctx: &VmContext, arr: &ArrayObj, v: &Value) -> OpResult<()> {
    if arr.struct_layout().is_some() {
        return check_struct_box(&arr.element_type(), v);
    }
    match arr.prim_backing_kind() {
        Some(kind) if !arr.prim_backing_accepts(v) => Err(OpError::array_elem_store_rejected(kind, v)),
        Some(_) => Ok(()),
        None => {
            let elem = arr.element_type();
            if matches!(v, Value::Null) || accepts_any(ctx, &elem) { return Ok(()); }
            check_reference(ctx, type_name(&elem), v, &TypeKeyCell::new())
        }
    }
}

/// 批量写入 `dst[di..di+n]` **之后**的写屏障：区间内每个引用槽（REF / MIXED / FROZEN 的元素、
/// struct[] 元素的每个引用叶子）里的堆引用各发一次 `write_barrier_array_elem`——槽里实际落下的引用，
/// 装箱的值给盒子（`ArrayObj::visit_elem_refs`；只收集真正的引用，`List<int>` 的扩容拷贝不分配）。
/// 并发模式要逐个染色新值，所以不能只发一次。调用时数组的借用必须已经放掉。
pub fn barrier_after_range_store(ctx: &VmContext, dst: &ArrRef, di: usize, n: usize) {
    let mut hits: Vec<(usize, Value)> = Vec::new();
    dst.borrow().visit_elem_refs(di, n, |i, v| hits.push((i, *v)));
    if hits.is_empty() {
        return;
    }
    let owner = Value::Array(*dst);
    for (i, v) in &hits {
        ctx.heap().write_barrier_array_elem(&owner, *i, v);
    }
}

// ── 拷贝计划 ─────────────────────────────────────────────────────────────────

/// 一侧数组决定拷贝路线的全部信息。
struct Shape {
    len: usize,
    elem: ElemType,
    /// 值 struct 数组的元素 `(size, 引用叶子偏移)`。
    strukt: Option<(usize, Box<[u32]>)>,
    /// 基元 backing 的种类标签（`int[]` …）；引用 / struct backing 为 `None`。
    prim: Option<&'static str>,
}

impl Shape {
    fn of(a: &ArrayObj) -> Self {
        Shape {
            len: a.len(),
            elem: a.element_type(),
            strukt: a.struct_layout().map(|l| (l.size, l.ref_offsets.clone())),
            prim: a.prim_backing_kind(),
        }
    }
    fn kind(&self) -> &str {
        self.prim.map(|p| p.trim_end_matches("[]")).unwrap_or(&self.elem)
    }
}

enum Plan { Direct, Box, Unbox, CheckedPrimitive, Checked }

fn plan(ctx: &VmContext, s: &Shape, d: &Shape) -> OpResult<Plan> {
    let mismatch = || OpError::array_type_mismatch(s.kind(), d.kind());
    match (&s.strukt, &d.strukt) {
        (Some(sl), Some(dl)) => {
            if *s.elem != *d.elem { return Err(mismatch()); }
            if sl != dl {
                return Err(OpError::internal(format!(
                    "Array.CopyRange: two `{}[]` arrays with different element layouts", &*d.elem)));
            }
            Ok(Plan::Direct)
        }
        (Some(_), None) if d.prim.is_none() && holds_box_of(ctx, &d.elem, &s.elem) => Ok(Plan::Box),
        (None, Some(_)) if s.prim.is_none() && holds_box_of(ctx, &s.elem, &d.elem) => Ok(Plan::Unbox),
        (Some(_), None) | (None, Some(_)) => Err(mismatch()),
        (None, None) if d.prim.is_some() => Ok(if s.prim == d.prim { Plan::Direct } else { Plan::CheckedPrimitive }),
        (None, None) if s.elem == d.elem || accepts_any(ctx, &d.elem) => Ok(Plan::Direct),
        (None, None) if assignable_name(ctx, &s.elem, &d.elem) => Ok(Plan::Direct),
        (None, None) if !has_open_arguments(&s.elem) && known_class(ctx, &s.elem)
            && known_class(ctx, &d.elem) && !assignable_name(ctx, &d.elem, &s.elem) => Err(mismatch()),
        (None, None) => Ok(Plan::Checked),
    }
}

fn type_name(name: &str) -> &str {
    match name {
        "Object" => "Std.Object",
        "Array" => "Std.Array",
        other => builtin_fqn(other).unwrap_or(other),
    }
}

fn accepts_any(ctx: &VmContext, name: &str) -> bool {
    name.is_empty() || type_name(name) == "Std.Object"
        || has_open_arguments(name)
        || is_open_type_arg_name(name) && lookup_desc(ctx, name).is_none()
}

fn has_open_arguments(name: &str) -> bool {
    crate::metadata::index::type_param_markers::has_markers(name)
        || name.contains(['<', '[']) && is_open_type_arg(name)
}

fn known_class(ctx: &VmContext, name: &str) -> bool {
    if name.ends_with("[]") || matches!(type_name(name), "Std.String" | "Std.Object" | "Std.Array") {
        return true;
    }
    lookup_desc(ctx, erased(type_name(name)))
        .is_some_and(|td| td.class_flags & crate::metadata::ir::CLASS_FLAG_INTERFACE == 0)
}

fn lookup_desc(ctx: &VmContext, name: &str) -> Option<std::sync::Arc<crate::object::TypeDesc>> {
    ctx.module().and_then(|m| m.type_registry.get(name).cloned())
        .or_else(|| ctx.try_lookup_type(name))
}

fn assignable_name(ctx: &VmContext, src: &str, dst: &str) -> bool {
    let (src, dst) = (type_name(src), type_name(dst));
    if src == dst || dst == "Std.Object" {
        return true;
    }
    // The general type-test wildcard for erased arguments does not prove a bulk upcast safe.
    if has_open_arguments(src) {
        return false;
    }
    if let Some(elem) = src.strip_suffix("[]") {
        return array_isa(elem, dst);
    }
    ctx.module().is_some_and(|m| is_subclass_or_eq_td(ctx, &m.type_registry, src, dst))
}

fn check_reference(ctx: &VmContext, target: &str, value: &Value, key: &TypeKeyCell) -> OpResult<()> {
    let matches = match value {
        Value::Null => true,
        Value::Array(a) => array_isa(&a.borrow().element_type(), target),
        _ if prim_isa(value, target) => true,
        _ => ctx.module().is_some_and(|m| {
            let registry = &m.type_registry;
            match value {
                Value::Object(o) => isa_object(ctx, registry, o.type_desc(), o.type_args(), target, key),
                Value::BoxedStruct(b) => isa_td(ctx, registry, b.type_desc(), target, key),
                _ => bound_param_extra_isa(ctx, registry, value, target)
                    || primitive_class_name(value).is_some_and(|src| is_subclass_or_eq_td(ctx, registry, src, target)),
            }
        }),
    };
    if matches {
        return Ok(());
    }
    Err(OpError::array_elem_store_rejected(target, value))
}

/// 元素类型为 `holder` 的引用数组能不能装 `strukt` 的装箱：`object` / 未知（Rust 合成的数组、
/// 擦除的型参）/ 该 struct 实现的接口。`string` 与其它已知的类 → 不能。
fn holds_box_of(ctx: &VmContext, holder: &str, strukt: &str) -> bool {
    match holder {
        "" | "object" | "Std.Object" | "Std.ValueType" => return true,
        "string" | "Std.String" => return false,
        _ => {}
    }
    let Some(m) = ctx.module() else { return true };
    let known = m.type_registry.contains_key(holder) || ctx.try_lookup_type(holder).is_some();
    !known || crate::vm::exec::ops::cast::is_subclass_or_eq_td(ctx, &m.type_registry, strukt, holder)
}

/// 值必须是元素类型 `elem` 的装箱 struct。
fn check_struct_box(elem: &str, v: &Value) -> OpResult<()> {
    if let Value::BoxedStruct(b) = v {
        if *b.borrow().type_desc.name == *elem {
            return Ok(());
        }
    }
    Err(OpError::array_elem_store_rejected(elem, v))
}

/// struct[] → 引用数组：逐元素装箱。
/// 每个盒子一出生就写进目的数组（目的数组在调用方寄存器里，是根）。
fn box_into(
    ctx: &VmContext, src: &ArrRef, s: &Shape, si: usize, dst: &ArrRef, di: usize, n: usize,
) -> OpResult<()> {
    for k in 0..n {
        let (bytes, refs) = src.borrow().struct_elem(si + k)
            .ok_or_else(|| OpError::internal("Array.CopyRange: struct[] source is not a value-struct array".into()))?;
        let boxed = box_struct(ctx, s.elem, &bytes, &refs)?;
        // 下一个盒子的分配可能触发回收：这一个先发屏障，不等整段写完。
        let wrote = dst.borrow().set_boxed(di + k, boxed);
        super::array::barrier_elem(ctx, dst, di + k, wrote, &boxed);
    }
    Ok(())
}

/// The snapshot is one GC root even if a checked range contains many references.
struct SnapshotRoot<'a>(&'a Heap, RootHandle);

impl Drop for SnapshotRoot<'_> {
    fn drop(&mut self) {
        self.0.unpin_root(self.1);
    }
}

fn copy_checked(
    ctx: &VmContext, src: &ArrRef, si: usize, dst: &ArrRef, di: usize, n: usize,
    check: impl Fn(&Value) -> OpResult<()>,
) -> OpResult<()> {
    let heap = ctx.heap();
    let source = src.borrow();
    let direct_refs = source.mode() == ArrayMode::Ref && dst.borrow().mode() == ArrayMode::Ref;
    let snapshot = if direct_refs {
        source.snapshot_ref_range(heap, si, n)
    } else {
        ArrayObj::alloc_frozen_from(heap, n, |i| source.get_boxed(si + i))
    };
    let Value::Array(values) = snapshot else {
        return Err(OpError::oom("Array.CopyRange snapshot".into()));
    };
    let _root = SnapshotRoot(heap, heap.pin_root(snapshot));
    for value in values.borrow().iter_boxed() {
        check(&value)?;
    }
    if direct_refs {
        dst.borrow().copy_elems_from(&values.borrow(), 0, di, n);
        barrier_after_range_store(ctx, dst, di, n);
        return Ok(());
    }
    for (i, value) in values.borrow().iter_boxed().enumerate() {
        let wrote = dst.borrow().try_set_boxed(di + i, value)
            .map_err(|e| OpError::throw(INVALID_CAST_EXC, e.to_string()))?;
        super::array::barrier_elem(ctx, dst, di + i, wrote, &value);
    }
    Ok(())
}

/// 把一份 struct 元素的字节 + 引用叶子装箱。泛型 struct 实例名查不到时按擦除基名（同 `try_struct_backed`）。
fn box_struct(ctx: &VmContext, elem: ElemType, bytes: &[u8], refs: &[Value]) -> OpResult<Value> {
    let name: &str = &elem;
    let name = if ctx.try_lookup_type(name).is_some() { name } else { name.split('<').next().unwrap_or(name) };
    match crate::corelib::convert::box_struct_blob(ctx, name, bytes, refs)? {
        Value::Null => Err(OpError::oom(format!("a boxed {name}"))),
        v => Ok(v),
    }
}

#[cfg(test)]
#[path = "array_bulk_tests.rs"]
mod array_bulk_tests;
