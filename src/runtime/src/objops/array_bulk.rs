//! `Std.Array` 的无类型 / 批量元素操作：`CopyRange`（`__array_copy`）、`GetValue` 的元素读取、
//! `SetValue` 的值 struct 元素校验，以及批量写入后的写屏障。corelib 只解析参数、把 [`OpError`] 抛给用户。
//!
//! 元素可以是任意 backing。值 struct 数组（`StructBytes`：元素字节紧凑打包 + 引用叶子侧表）不能走
//! `get_boxed` / `set_boxed`——那两个访问器没有堆、装不了箱——所以这里按两侧 backing 选一条路：
//!
//! | 源 → 目的 | 做法 | 类型不符 |
//! |---|---|---|
//! | `T[]` → `T[]`（同一 struct） | 字节区间 + 引用叶子区间各拷一次（同数组按 memmove） | 元素类型不同 → `ArrayTypeMismatchException` |
//! | struct[] → 引用数组（`object[]` / 接口数组） | 逐元素装箱 | 目的元素类型装不下这个 struct → `ArrayTypeMismatchException` |
//! | 引用数组 → struct[] | 先逐元素校验是该 struct 的装箱，再逐元素拆进去 | 数组类型不兼容 → `ArrayTypeMismatchException`；某个元素不是（含 null）→ `InvalidCastException` |
//! | struct[] ↔ 基元数组 | — | `ArrayTypeMismatchException` |
//! | 其余（基元 / 引用之间） | `ArrayObj::copy_elems_from`（同种 backing 一次切片拷贝） | 元素种类不符 → `InvalidCastException` |
//!
//! 抛异常时目的数组不动。写入之后对写入区间里**每个引用槽**（引用数组的元素、struct[] 元素的每个
//! 引用叶子）发 `write_barrier_array_elem`；覆盖前的旧引用由各写入原语交给 SATB。

use crate::gc::GcRef;
use crate::metadata::types::{ArrayObj, ElemType};
use crate::metadata::Value;
use crate::vm_context::VmContext;

use super::error::{OpError, OpResult, INVALID_CAST_EXC};

/// `Array.CopyRange(src, si, dst, di, n)`：`src[si..si+n]` → `dst[di..di+n]`，同一数组内重叠按 memmove。
pub fn copy_range(
    ctx: &VmContext, src: &GcRef<ArrayObj>, si: usize, dst: &GcRef<ArrayObj>, di: usize, n: usize,
) -> OpResult<()> {
    if n == 0 {
        return Ok(());
    }
    // 同一数组 ⇒ 只取一把锁（同一 GcRef 上 borrow + borrow_mut 会自锁）；元素类型必然相同。
    if GcRef::ptr_eq(src, dst) {
        {
            let mut a = dst.borrow_mut();
            let len = a.len();
            if si + n > len || di + n > len {
                return Err(OpError::copy_range_out_of_bounds(len, si, len, di, n));
            }
            a.copy_elems_within(si, di, n);
        }
        barrier_after_range_store(ctx, dst, di, n);
        return Ok(());
    }
    let (s, d) = (Shape::of(&src.borrow()), Shape::of(&dst.borrow()));
    if si + n > s.len || di + n > d.len {
        return Err(OpError::copy_range_out_of_bounds(s.len, si, d.len, di, n));
    }
    // 类型判定会查类型表（可能触发惰性加载），放在数组锁之外。
    match plan(ctx, &s, &d)? {
        Plan::Direct => copy_direct(src, si, dst, di, n)?,
        Plan::Box => box_into(ctx, src, &s, si, dst, di, n)?,
        Plan::Unbox => unbox_into(src, si, dst, &d, di, n)?,
    }
    barrier_after_range_store(ctx, dst, di, n);
    Ok(())
}

/// `Array.GetValue(i)` 的元素：值 struct 数组的元素装箱成一份快照（独立于数组），其余同 `get_boxed`。
/// 调用方已检查下标。
pub fn elem_get_boxed(ctx: &VmContext, arr: &GcRef<ArrayObj>, i: usize) -> OpResult<Value> {
    let (et, elem) = {
        let a = arr.borrow();
        match a.struct_elem(i) {
            None => return Ok(a.get_boxed(i)),
            Some((b, r)) => (a.element_type, (b.to_vec(), r.to_vec())),
        }
    };
    box_struct(ctx, et, &elem.0, &elem.1)
}

/// 无类型写入（`Array.SetValue`）前的校验，不符 → `InvalidCastException`（同 C#），数组不动：
/// 值 struct 数组只收该 struct 的装箱（null 也不行）；基元数组只收同种值（`prim_backing_accepts`，
/// 不拓宽）；引用数组什么都收。
pub fn check_untyped_store(arr: &ArrayObj, v: &Value) -> OpResult<()> {
    if arr.struct_layout().is_some() {
        return check_struct_box(&arr.element_type, v);
    }
    match arr.prim_backing_kind() {
        Some(kind) if !arr.prim_backing_accepts(v) => Err(OpError::throw(INVALID_CAST_EXC, format!(
            "cannot store {} into a {kind} element", crate::semantics::value_kind_name(v)))),
        _ => Ok(()),
    }
}

/// 批量写入 `dst[di..di+n]` **之后**的写屏障：区间内每个引用槽（引用数组的元素 / struct[] 元素的每个
/// 引用叶子）里的堆引用各发一次 `write_barrier_array_elem`。并发模式要逐个染色新值，所以不能只发一次。
/// 调用时数组的借用必须已经放掉。
pub fn barrier_after_range_store(ctx: &VmContext, dst: &GcRef<ArrayObj>, di: usize, n: usize) {
    let hits: Vec<(usize, Value)> = {
        let a = dst.borrow();
        let (slots, per) = a.elem_ref_slots(di, n);
        if per == 0 {
            return;
        }
        slots.iter().enumerate()
            .filter(|(_, v)| v.is_heap_ref())
            .map(|(k, v)| (di + k / per, *v))
            .collect()
    };
    let owner = Value::Array(*dst);
    for (i, v) in &hits {
        ctx.heap().write_barrier_array_elem(&owner, *i, v);
    }
}

// ── 拷贝计划 ─────────────────────────────────────────────────────────────────

/// 一侧数组决定拷贝路线的全部信息（取完就放锁）。
struct Shape {
    len: usize,
    elem: ElemType,
    /// 值 struct 数组的元素 `(size, ref_count)`。
    strukt: Option<(usize, usize)>,
    /// 基元 backing 的种类标签（`int[]` …）；引用 / struct backing 为 `None`。
    prim: Option<&'static str>,
}

impl Shape {
    fn of(a: &ArrayObj) -> Self {
        Shape {
            len: a.len(),
            elem: a.element_type,
            strukt: a.struct_layout().map(|l| (l.size, l.ref_count())),
            prim: a.prim_backing_kind(),
        }
    }
    fn kind(&self) -> &str {
        self.prim.map(|p| p.trim_end_matches("[]")).unwrap_or(&self.elem)
    }
}

enum Plan { Direct, Box, Unbox }

fn plan(ctx: &VmContext, s: &Shape, d: &Shape) -> OpResult<Plan> {
    let mismatch = || OpError::array_type_mismatch(s.kind(), d.kind());
    match (s.strukt, d.strukt) {
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
        (None, None) => Ok(Plan::Direct),
    }
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
    !known || crate::interp::dispatch::is_subclass_or_eq_td(ctx, &m.type_registry, strukt, holder)
}

/// 值必须是元素类型 `elem` 的装箱 struct。
fn check_struct_box(elem: &str, v: &Value) -> OpResult<()> {
    if let Value::BoxedStruct(b) = v {
        if *b.borrow().type_desc.name == *elem {
            return Ok(());
        }
    }
    Err(OpError::struct_elem_store_rejected(elem, v))
}

// ── 三条路线 ─────────────────────────────────────────────────────────────────

/// 同种 backing / 同一 struct：`copy_elems_from`。基元 ↔ 引用这种混搭先逐元素校验种类，不符就整段拒绝。
fn copy_direct(src: &GcRef<ArrayObj>, si: usize, dst: &GcRef<ArrayObj>, di: usize, n: usize) -> OpResult<()> {
    let s = src.borrow();
    let mut d = dst.borrow_mut();
    // 两侧 backing 同种 ⇒ 不可能不符，零成本跳过（真实用法几乎全在这条）。
    if s.prim_backing_kind() != d.prim_backing_kind() {
        if let Some(kind) = d.prim_backing_kind() {
            for k in 0..n {
                let elem = s.get_boxed(si + k);
                if !d.prim_backing_accepts(&elem) {
                    return Err(OpError::throw(INVALID_CAST_EXC, format!(
                        "source element {} is {} — cannot store it into a {kind} element",
                        si + k, crate::semantics::value_kind_name(&elem))));
                }
            }
        }
    }
    d.copy_elems_from(&s, si, di, n);
    Ok(())
}

/// struct[] → 引用数组：逐元素装箱。装箱要分配，分配期间不持有任何数组锁；
/// 每个盒子一出生就写进目的数组（目的数组在调用方寄存器里，是根）。
fn box_into(
    ctx: &VmContext, src: &GcRef<ArrayObj>, s: &Shape, si: usize, dst: &GcRef<ArrayObj>, di: usize, n: usize,
) -> OpResult<()> {
    for k in 0..n {
        let (bytes, refs) = {
            let a = src.borrow();
            let (b, r) = a.struct_elem(si + k)
                .ok_or_else(|| OpError::internal("Array.CopyRange: struct[] source lost its StructBytes backing".into()))?;
            (b.to_vec(), r.to_vec())
        };
        let boxed = box_struct(ctx, s.elem, &bytes, &refs)?;
        dst.borrow_mut().set_boxed(di + k, boxed);
    }
    Ok(())
}

/// 引用数组 → struct[]：先整段校验（不符就抛、目的地不动），再逐元素把装箱的字节与引用叶子拷进去。
fn unbox_into(src: &GcRef<ArrayObj>, si: usize, dst: &GcRef<ArrayObj>, d: &Shape, di: usize, n: usize) -> OpResult<()> {
    let s = src.borrow();
    let mut a = dst.borrow_mut();
    for k in 0..n {
        check_struct_box(&d.elem, &s.get_boxed(si + k))?;
    }
    for k in 0..n {
        a.set_boxed(di + k, s.get_boxed(si + k));   // StructBytes 臂：拷字节 + 引用叶子（SATB 记旧叶子）
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
