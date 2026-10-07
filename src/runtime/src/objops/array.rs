//! 数组：分配、元素读写、长度的唯一实现，外加 ref 解引用与 JIT 打包快路用的原语。
//!
//! 数组形态：堆数组 `Array`（`Boxed` / 打包基元 / `StructBytes`）、栈数组 `StackArray`
//! （逃逸分析，经 per-context 栈 arena）。`Null` → `NullReferenceException`；
//! 下标越界（含负数）→ `IndexOutOfRangeException`；长度为负 → `OverflowException`。

use crate::metadata::types::{default_value_for_tag, ArrayBacking, ArrayObj, ElemType, StructArrayElem};
use crate::metadata::Value;
use crate::gc::GcRef;
use crate::interp::transient_arena::TransientPayload;
use crate::vm_context::VmContext;

use super::error::{ArrayOp, OpError, OpResult};

/// 下标检查：`I64` 且落在 `[0, len)` → `usize`；越界（含负数）→ `IndexOutOfRangeException`；
/// 非整数 → 内部错误（编译器保证下标是整数）。
#[inline(always)]
pub fn checked_index(op: ArrayOp, idx: &Value, len: usize) -> OpResult<usize> {
    match idx {
        Value::I64(n) if (*n as u64) < len as u64 => Ok(*n as usize),
        Value::I64(n) => Err(OpError::index_out_of_range(*n, len)),
        other => Err(OpError::internal(format!(
            "{}: array index must be an integer, got {other:?}", op.opcode()))),
    }
}

#[cold]
fn not_an_array(op: ArrayOp, v: &Value) -> OpError {
    match v {
        Value::Null => OpError::null_array(op),
        other => OpError::internal(format!("{}: expected array, got {other:?}", op.opcode())),
    }
}

/// `ArrayGet`。值 struct 数组（`StructBytes`）的元素以 `StructRefHeap` 句柄返回（指向数组字节，
/// 支持原地 `arr[i].x`），句柄载荷进 transient arena，`frame_id` 按需取（惰性分配）。
#[inline(always)]
pub fn array_get<F: FnOnce() -> u32>(ctx: &VmContext, arr: &Value, idx: &Value, frame_id: F) -> OpResult<Value> {
    if let Value::Array(rc) = arr {
        let a = rc.borrow();
        let i = checked_index(ArrayOp::Get, idx, a.len())?;
        if !matches!(&a.backing, ArrayBacking::StructBytes { .. }) {
            return Ok(a.get_boxed(i));   // 打包基元在这里装回 Value
        }
        drop(a);
        return Ok(struct_elem_handle(ctx, *rc, i, frame_id()));
    }
    array_get_rare(ctx, arr, idx)
}

/// 值 struct 数组元素的 `StructRefHeap` 句柄：载荷进 transient arena，盖上当前帧号。
#[inline(never)]
fn struct_elem_handle(ctx: &VmContext, arr: GcRef<ArrayObj>, i: usize, fid: u32) -> Value {
    let hidx = ctx.transient_alloc(fid, TransientPayload::StructElem(StructArrayElem { arr, index: i as u32 }));
    Value::StructRefHeap { idx: hidx, frame_id: fid }
}

#[inline(never)]
fn array_get_rare(ctx: &VmContext, arr: &Value, idx: &Value) -> OpResult<Value> {
    match arr {
        Value::StackArray { idx: aidx, frame_id: afid } => {
            ctx.stack_arena.lock().with_arr(*aidx, *afid, |a| {
                checked_index(ArrayOp::Get, idx, a.len()).map(|i| a.get_boxed(i))
            })?
        }
        other => Err(not_an_array(ArrayOp::Get, other)),
    }
}

/// `ArraySet`。堆数组写入堆引用时发 `write_barrier_array_elem`（基元不发）；栈数组不发。
#[inline(always)]
pub fn array_set(ctx: &VmContext, arr: &Value, idx: &Value, v: &Value) -> OpResult<()> {
    if let Value::Array(rc) = arr {
        let i = {
            let mut a = rc.borrow_mut();
            let i = checked_index(ArrayOp::Set, idx, a.len())?;
            a.set_boxed(i, *v);   // 打包基元在这里拆箱
            i
        };
        if v.is_heap_ref() {
            ctx.heap().write_barrier_array_elem(arr, i, v);
        }
        return Ok(());
    }
    array_set_rare(ctx, arr, idx, v)
}

#[inline(never)]
fn array_set_rare(ctx: &VmContext, arr: &Value, idx: &Value, v: &Value) -> OpResult<()> {
    match arr {
        Value::StackArray { idx: aidx, frame_id } => {
            ctx.stack_arena.lock().with_arr_mut(*aidx, *frame_id, |a| {
                checked_index(ArrayOp::Set, idx, a.len()).map(|i| a.set_boxed(i, *v))
            })?
        }
        other => Err(not_an_array(ArrayOp::Set, other)),
    }
}

/// `ArrayLen`。
#[inline]
pub fn array_len(ctx: &VmContext, arr: &Value) -> OpResult<i64> {
    match arr {
        Value::Array(rc) => Ok(rc.borrow().len() as i64),
        Value::StackArray { idx, frame_id } => {
            Ok(ctx.stack_arena.lock().with_arr(*idx, *frame_id, |a| a.len() as i64)?)
        }
        other => Err(not_an_array(ArrayOp::Len, other)),
    }
}

/// JIT 打包快路：`int[]` / `long[]` / `double[]` 的 `(数据基址, 长度, 槽宽 4|8)`。
/// 其它一切（非打包 backing / 栈数组 / 非数组 / null）→ 宽度 0 = 「无快路」，内联码回落到
/// `array_get` / `array_set`，由它们给出与 interp 相同的异常。不抛。
///
/// 指针在帧内有效：数组定长不重分配、GC 不移动。
#[inline]
pub fn packed_data(arr: &Value) -> (*const u8, i64, i64) {
    match arr {
        Value::Array(rc) => {
            let a = rc.borrow();
            (a.packed_num_ptr().unwrap_or(std::ptr::null()), a.len() as i64, a.packed_elem_width())
        }
        _ => (std::ptr::null(), 0, 0),
    }
}

/// `LoadElemAddr` 的检查：只有堆数组能取元素地址，下标此刻就校验（与读写同一规则）。
pub fn check_elem_addr(arr: &Value, idx: &Value) -> OpResult<(GcRef<ArrayObj>, usize)> {
    match arr {
        Value::Array(rc) => {
            let i = checked_index(ArrayOp::Addr, idx, rc.borrow().len())?;
            Ok((*rc, i))
        }
        other => Err(not_an_array(ArrayOp::Addr, other)),
    }
}

/// 经 `ref` 读元素（`RefKind::Array`）。
pub fn elem_load(gc: &GcRef<ArrayObj>, i: usize) -> OpResult<Value> {
    let a = gc.borrow();
    if i >= a.len() { return Err(OpError::index_out_of_range(i as i64, a.len())); }
    Ok(a.get_boxed(i))
}

/// 经 `ref` 写元素：与 `ArraySet` 同一条写入 + 写屏障规则。
pub fn elem_store(ctx: &VmContext, gc: &GcRef<ArrayObj>, i: usize, v: &Value) -> OpResult<()> {
    {
        let mut a = gc.borrow_mut();
        if i >= a.len() { return Err(OpError::index_out_of_range(i as i64, a.len())); }
        a.set_boxed(i, *v);
    }
    if v.is_heap_ref() {
        ctx.heap().write_barrier_array_elem(&Value::Array(*gc), i, v);
    }
    Ok(())
}

// ── 分配 ─────────────────────────────────────────────────────────────────────

/// `ArrayNew` 的长度：非负 `I64`；负数 → `OverflowException`；非整数 → 内部错误。
#[inline]
pub fn array_size(len: &Value) -> OpResult<usize> {
    match len {
        Value::I64(n) if *n >= 0 => Ok(*n as usize),
        Value::I64(n) => Err(OpError::negative_array_size(*n)),
        other => Err(OpError::internal(format!("ArrayNew: array size must be an integer, got {other:?}"))),
    }
}

/// `ArrayNew`：`len` 个元素的数组。
///
/// - 元素是 blob 值 struct → `StructBytes` 堆数组（不走栈分配）。
/// - `prim_zero`：泛型型参在运行期解析成基元值类型时的零值（只有 interp 会给，JIT 不翻译这种站点）；
///   否则按 `elem_tag` 取默认值。
/// - `stack`：编译器证明不逃逸且运行期允许时给出栈帧号，数组进栈 arena。
pub fn array_new<F: FnOnce() -> u32>(
    ctx: &VmContext, len: &Value, elem_tag: u8, element_type: ElemType,
    prim_zero: Option<Value>, stack: Option<F>,
) -> OpResult<Value> {
    let n = array_size(len)?;
    if let Some(sb) = try_struct_backed(ctx, element_type, n) {
        return alloc(ctx, sb, || format!("struct array[{n}]"));
    }
    let default = prim_zero.unwrap_or_else(|| default_value_for_tag(elem_tag));
    if let Some(frame_id) = stack {
        return Ok(stack_array(ctx, frame_id(), element_type, vec![default; n]));
    }
    // 默认值直接填进 GC 块，不经 `vec![default; n]` 中转。
    let heap = ctx.heap();
    alloc(ctx, ArrayObj::typed_filled(heap, element_type, n, default), || format!("array[{n}]"))
}

/// `ArrayNewLit`：由 `elems` 构成的数组（blob 值 struct 字面量逐个打包进 `StructBytes`）。
/// 元素按需从迭代器取，堆数组直接打包进 GC 块，不经中转 `Vec`。
pub fn array_new_lit<F: FnOnce() -> u32>(
    ctx: &VmContext, elems: impl ExactSizeIterator<Item = Value>, element_type: ElemType, stack: Option<F>,
) -> OpResult<Value> {
    let n = elems.len();
    if let Some(mut sb) = try_struct_backed(ctx, element_type, n) {
        for (i, v) in elems.enumerate() { pack_struct_elem(ctx, &mut sb, i, &v)?; }
        return alloc(ctx, sb, || format!("struct array literal[{n}]"));
    }
    if let Some(frame_id) = stack {
        return Ok(stack_array(ctx, frame_id(), element_type, elems.collect()));
    }
    let heap = ctx.heap();
    alloc(ctx, ArrayObj::typed_iter(heap, element_type, n, elems), || format!("array literal[{n}]"))
}

/// 类级泛型型参 `T`（`new T[n]`，`T` 是接收者类的型参）：接收者（reg 0）的实例型参解析成基元值类型时，
/// 返回它的零值作为每个元素的默认值；引用 / struct 型参、非对象接收者 → `None`（擦除数组，元素 Null）。
/// interp 与 JIT 共用（方法级型参只有 interp 的帧带着，见 `interp::exec_array`）。
pub fn class_type_param_zero(receiver: Option<&Value>, index: usize) -> Option<Value> {
    let Some(Value::Object(rc)) = receiver else { return None };
    let d = crate::metadata::types::default_value_for(rc.borrow().type_args().get(index)?);
    (!matches!(d, Value::Null)).then_some(d)
}

fn alloc(ctx: &VmContext, a: ArrayObj, what: impl FnOnce() -> String) -> OpResult<Value> {
    let arr = ctx.heap().alloc_array_obj(a);
    if matches!(arr, Value::Null) { return Err(OpError::oom(what())); }
    Ok(arr)
}

fn stack_array(ctx: &VmContext, frame_id: u32, element_type: ElemType, elems: Vec<Value>) -> Value {
    let idx = ctx.stack_alloc_arr(frame_id, ArrayObj::stack_typed(element_type, elems));
    Value::StackArray { idx, frame_id }
}

/// 元素类型是 **blob 值 struct**（≥1 个字段且交付了字节布局，与编译器 `IsBlobStruct` 同一判据）时，
/// 建 `len` 个零初始化元素的 `StructBytes` 数组；基元 / 引用 / 未交付布局 → `None`。
///
/// 泛型值 struct 先按完整实例名查（编译器为特化实例交付的合成描述符，布局内联了 struct 实参），
/// 再按擦除基名查；`element_type`（驻留的 8 B 句柄）仍原样传给 backing，反射拿到的是实例化后的元素类型。
pub fn try_struct_backed(ctx: &VmContext, element_type: ElemType, len: usize) -> Option<ArrayObj> {
    let name: &str = &element_type;
    let td = match ctx.try_lookup_type(name) {
        Some(td) => td,
        None => {
            let erased = name.split('<').next().unwrap_or(name);
            ctx.try_lookup_type(erased)?
        }
    };
    // 闸门与编译器 `StructLayout.IsBlobStruct` 必须逐字一致（FieldCount >= 1）；这是全 VM 唯一一份镜像。
    if td.fields.is_empty() { return None; }
    let layout = td.struct_layout()?;
    if layout.size == 0 { return None; }      // self-referential / empty guard
    Some(ArrayObj::struct_backed(ctx.heap(), element_type, len, layout))
}

/// 把一个 struct 值打包进 `StructBytes` 数组的第 `i` 个元素（`new P[]{ p1, p2 }`）。
/// 源是 arena `StructRef` 或装箱 `BoxedStruct`；`Null` = 默认值（元素保持零初始化）。
pub fn pack_struct_elem(ctx: &VmContext, arr: &mut ArrayObj, i: usize, v: &Value) -> OpResult<()> {
    let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = match v {
        Value::BoxedStruct(b) => { let o = b.borrow(); (o.bytes().to_vec(), o.refs().to_vec()) }
        Value::StructRef { idx, frame_id } =>
            ctx.struct_arena.lock().with(*idx, *frame_id, |s| (s.bytes.to_vec(), s.refs.to_vec()))?,
        Value::Null => return Ok(()),
        other => return Err(OpError::internal(format!(
            "struct array literal element must be a struct value, got {other:?}"))),
    };
    arr.write_struct_elem(i, &src_bytes, &src_refs);
    Ok(())
}
