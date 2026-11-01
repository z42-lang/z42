//! 数组：分配、元素读写、长度的唯一实现，外加 ref 解引用与 JIT 打包快路用的原语。
//!
//! 数组形态：堆数组 `Array`（变长区里的一块：头 + 元素；REF / MIXED / PRIM / STRUCT / FROZEN，见
//! `metadata::types::array`）、栈数组 `StackArray`（逃逸分析，经 per-context 栈 arena）。模式在分配时由元素类型定
//! （[`elem_mode`]）。`Null` → `NullReferenceException`；
//! 下标越界（含负数）→ `IndexOutOfRangeException`；长度为负 → `OverflowException`。

use crate::metadata::types::{default_value_for_tag, ArrayMode, ArrayObj, ElemKind, ElemType, FieldWrite, StructArrayElem, StructTypeLayout};
use crate::metadata::Value;
use crate::gc::ArrRef;
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

/// `ArrayGet`。值 struct 数组（STRUCT）的元素以 `StructRefHeap` 句柄返回（指向数组字节，
/// 支持原地 `arr[i].x`），句柄载荷进 transient arena，`frame_id` 按需取（惰性分配）。
#[inline(always)]
pub fn array_get<F: FnOnce() -> u32>(ctx: &VmContext, arr: &Value, idx: &Value, frame_id: F) -> OpResult<Value> {
    if let Value::Array(rc) = arr {
        let a = rc.borrow();
        let i = checked_index(ArrayOp::Get, idx, a.len())?;
        if a.mode() != ArrayMode::Struct {
            return Ok(a.get_boxed(i));   // 打包基元在这里装回 Value
        }
        return Ok(struct_elem_handle(ctx, *rc, i, frame_id()));
    }
    array_get_rare(ctx, arr, idx)
}

/// 值 struct 数组元素的 `StructRefHeap` 句柄：载荷进 transient arena，盖上当前帧号。
#[inline(never)]
fn struct_elem_handle(ctx: &VmContext, arr: ArrRef, i: usize, fid: u32) -> Value {
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

/// `ArraySet`。堆数组写入堆引用时发 `write_barrier_array_elem`（基元不发；写进单元的是盒子时给盒子）；
/// 栈数组不发。
#[inline(always)]
pub fn array_set(ctx: &VmContext, arr: &Value, idx: &Value, v: &Value) -> OpResult<()> {
    if let Value::Array(rc) = arr {
        let (i, wrote) = {
            let a = rc.borrow();
            let i = checked_index(ArrayOp::Set, idx, a.len())?;
            (i, a.set_boxed(i, *v))   // 打包基元在这里拆箱；引用编成引用字 / 型参单元
        };
        barrier_elem(ctx, rc, i, wrote, v);
        return Ok(());
    }
    array_set_rare(ctx, arr, idx, v)
}

#[inline(never)]
fn array_set_rare(ctx: &VmContext, arr: &Value, idx: &Value, v: &Value) -> OpResult<()> {
    match arr {
        Value::StackArray { idx: aidx, frame_id } => {
            ctx.stack_arena.lock().with_arr_mut(*aidx, *frame_id, |a| {
                checked_index(ArrayOp::Set, idx, a.len()).map(|i| a.set(i, *v))
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
        Value::Array(rc) => rc.borrow().jit_packed(),
        _ => (std::ptr::null(), 0, 0),
    }
}

/// JIT 引用读快路：REF / MIXED 数组的 `(元素基址, 长度, 步长 | zero << 8)`（步长 8|16，`zero` 是 MIXED 数组
/// 未写过的单元读作的基元种类）；其它一切 → 0 = 「无快路」，内联码回落到 `array_get`。不抛。
/// 指针在帧内有效（同 [`packed_data`]）。
#[inline]
pub fn ref_data(arr: &Value) -> (*const u8, i64, i64) {
    match arr {
        Value::Array(rc) => rc.borrow().jit_refs(),
        _ => (std::ptr::null(), 0, 0),
    }
}

/// `LoadElemAddr` 的检查：只有堆数组能取元素地址，下标此刻就校验（与读写同一规则）。
pub fn check_elem_addr(arr: &Value, idx: &Value) -> OpResult<(ArrRef, usize)> {
    match arr {
        Value::Array(rc) => {
            let i = checked_index(ArrayOp::Addr, idx, rc.borrow().len())?;
            Ok((*rc, i))
        }
        other => Err(not_an_array(ArrayOp::Addr, other)),
    }
}

/// 经 `ref` 读元素（`RefKind::Array`）。
pub fn elem_load(gc: &ArrRef, i: usize) -> OpResult<Value> {
    let a = gc.borrow();
    if i >= a.len() { return Err(OpError::index_out_of_range(i as i64, a.len())); }
    Ok(a.get_boxed(i))
}

/// 经 `ref` 写元素：与 `ArraySet` 同一条写入 + 写屏障规则。
pub fn elem_store(ctx: &VmContext, gc: &ArrRef, i: usize, v: &Value) -> OpResult<()> {
    let wrote = {
        let a = gc.borrow();
        if i >= a.len() { return Err(OpError::index_out_of_range(i as i64, a.len())); }
        a.set_boxed(i, *v)
    };
    barrier_elem(ctx, gc, i, wrote, v);
    Ok(())
}

/// 元素写入**之后**的写屏障（卡表 / 并发 shade；SATB 删除屏障在写原语里）：落进单元的引用
/// （`wrote`——装不进 8 B 的值是它的盒子）。`ArraySet`、经 `ref` 写元素、`Std.Array.SetValue` 共用这一份。
/// 调用时数组的借用必须已经放掉。
///
/// **装箱 struct 写进 `struct[]` 是例外**：写入把盒子的 blob **拷进**数组，数组持有的是那些引用叶子、
/// 不是盒子。拿盒子判龄的话，老盒子、年轻叶子就漏了卡（叶子此后只靠数组可达，下一次 minor 把它扫掉），
/// 所以这种写入对元素的每个引用叶子发屏障（`array_bulk::barrier_after_range_store`）。
#[inline]
pub fn barrier_elem(ctx: &VmContext, arr: &ArrRef, i: usize, wrote: FieldWrite, v: &Value) {
    if matches!(v, Value::BoxedStruct(_)) && arr.borrow().struct_layout().is_some() {
        return super::array_bulk::barrier_after_range_store(ctx, arr, i, 1);
    }
    wrote.with_barrier_value(v, |s| ctx.heap().write_barrier_array_elem(&Value::Array(*arr), i, s));
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
/// - 元素是 blob 值 struct → STRUCT 堆数组（不走栈分配）。
/// - `tp_prim`：泛型型参在运行期解析成**基元**实参时的（打包种类, 零值）——数组按该宽度打包
///   （M13;`List<int>` 的 backing 每元素 4 B）。引用 / struct / 未解析的型参 → `None`，
///   擦除 MIXED（现状）。方法级型参只有 interp 会解析（JIT 不翻译这种站点）。
/// - `bound`：`new T[n]` 里 `T` 在运行期绑定的实参（[`type_param_bound_elem`]）——数组**报告**的
///   元素类型（`GetType()`、`Array.GetValue` 的装箱）是它而非字面 `T`；backing 的种类仍按上面
///   擦除口径选（共享泛型体里的 `T[]` 一律按擦除形态存取）。
/// - `stack`：编译器证明不逃逸且运行期允许时给出栈帧号，数组进栈 arena。
#[allow(clippy::too_many_arguments)]
pub fn array_new<F: FnOnce() -> u32>(
    ctx: &VmContext, len: &Value, elem_tag: u8, element_type: ElemType,
    type_param: bool, tp_prim: Option<TypeParamElem>, bound: Option<ElemType>, stack: Option<F>,
) -> OpResult<Value> {
    let n = array_size(len)?;
    let kind = match tp_prim {
        Some(p) => p.kind, // M13: 基元实参按其宽度打包,element_type 名字保持擦除（反射读 "T"）
        None => match elem_mode(ctx, element_type, type_param) {
            ElemMode::Struct(layout) =>
                return alloc(ArrayObj::alloc_struct(ctx.heap(), element_type, n, layout), || format!("struct array[{n}]")),
            ElemMode::Kind(k) => k,
        },
    };
    let default = tp_prim.map(|p| p.zero).unwrap_or_else(|| default_value_for_tag(elem_tag));
    if let Some(frame_id) = stack {
        return Ok(stack_array(ctx, frame_id(), vec![default; n]));
    }
    // 默认值直接填进 GC 块，不经 `vec![default; n]` 中转。
    alloc(ArrayObj::alloc_filled(ctx.heap(), bound.unwrap_or(element_type), kind, n, default), || format!("array[{n}]"))
}

/// `new T[n]`'s element type as bound at run time: the argument's name, interned. `None` when
/// the parameter is not bound here (the array keeps reporting `T`).
pub fn type_param_bound_elem(arg: Option<&str>) -> Option<ElemType> {
    arg.filter(|a| !a.is_empty() && !crate::metadata::index::type_param_markers::has_markers(a)).map(ElemType::intern)
}

/// The class-level argument `index` on `receiver` (the carrier [`class_type_param_elem`] reads).
pub fn class_type_param_bound(receiver: Option<&Value>, index: usize) -> Option<ElemType> {
    let Some(Value::Object(rc)) = receiver else { return None };
    let ta = rc.borrow().type_args_handle();
    type_param_bound_elem(ta.get(index).map(String::as_str))
}

/// `ArrayNewLit`：由 `elems` 构成的数组（blob 值 struct 字面量逐个打包进 STRUCT 数组）。
/// 元素按需从迭代器取，堆数组直接打包进 GC 块，不经中转 `Vec`。
pub fn array_new_lit<F: FnOnce() -> u32>(
    ctx: &VmContext, elems: impl ExactSizeIterator<Item = Value>, element_type: ElemType, stack: Option<F>,
) -> OpResult<Value> {
    let n = elems.len();
    let kind = match elem_mode(ctx, element_type, false) {
        ElemMode::Struct(layout) => {
            let arr = alloc(ArrayObj::alloc_struct(ctx.heap(), element_type, n, layout), || format!("struct array literal[{n}]"))?;
            if let Value::Array(a) = &arr {
                // A fresh array, not yet in any register (no safepoint before it is): packing the
                // elements needs no barrier.
                for (i, v) in elems.enumerate() { pack_struct_elem(ctx, a.borrow(), i, &v)?; }
            }
            return Ok(arr);
        }
        ElemMode::Kind(k) => k,
    };
    if let Some(frame_id) = stack {
        return Ok(stack_array(ctx, frame_id(), elems.collect()));
    }
    alloc(ArrayObj::alloc_from_iter(ctx.heap(), element_type, kind, n, elems), || format!("array literal[{n}]"))
}

/// 类级泛型型参 `T`（`new T[n]`，`T` 是接收者类的型参）在运行期解析成**基元**实参时的
/// 打包种类与零值（M13）。引用 / struct 实参、非对象接收者、越界下标 → `None`
/// （擦除 MIXED 数组，元素零值按 `elem_tag`）。interp 与 JIT 共用（方法级型参只有 interp
/// 的帧带着，见 `interp::exec_array`）。
pub fn class_type_param_elem(receiver: Option<&Value>, index: usize) -> Option<TypeParamElem> {
    let Some(Value::Object(rc)) = receiver else { return None };
    let ta = rc.borrow().type_args_handle();
    let kind = ta.elem_kind(index);
    if !kind.is_prim() { return None; }
    let zero = crate::metadata::types::default_value_for(ta.get(index)?);
    (!matches!(zero, Value::Null)).then_some(TypeParamElem { kind, zero })
}

/// 一个解析成基元实参的型参（M13）：数组按 `kind` 的宽度打包，`zero` 是每个元素的默认值。
#[derive(Clone, Copy)]
pub struct TypeParamElem {
    pub kind: ElemKind,
    pub zero: Value,
}

/// An allocation's result: `Null` (the heap refused — strict OOM, or a payload past the block
/// limit) → `OutOfMemoryException` naming `what`.
fn alloc(arr: Value, what: impl FnOnce() -> String) -> OpResult<Value> {
    if matches!(arr, Value::Null) { return Err(OpError::oom(what())); }
    Ok(arr)
}

fn stack_array(ctx: &VmContext, frame_id: u32, elems: Vec<Value>) -> Value {
    let idx = ctx.stack_alloc_arr(frame_id, crate::interp::stack_alloc::StackArray::new(elems));
    Value::StackArray { idx, frame_id }
}

/// 数组的模式：STRUCT（元素布局），或交给构造器的 [`ElemKind`]。
pub enum ElemMode {
    Struct(std::sync::Arc<StructTypeLayout>),
    Kind(ElemKind),
}

/// 按元素类型定数组模式——分配时一次定下，之后不变：
///
/// - 名字就能定的（[`ElemKind::of`]）：基元 → PRIM；`string` / `object` / 数组类型 → REF。
/// - 型参元素（`new T[n]`，`type_param`）→ MIXED（擦除的 `T[]`）。**例外（M13）**：分配点已把
///   型参解析成基元实参时（[`TypeParamElem`]），[`array_new`] 跳过这里直接按解析宽度打包，
///   不进本函数。
/// - 其余查类型表：**blob 值 struct** → STRUCT；类 / 接口 / 委托 → REF；enum（值是裸整数）、
///   非 blob 的 struct、查不到的名字 → MIXED（什么都装得下）。
///
/// 泛型类型先按完整实例名查（编译器为特化实例交付的合成描述符），再按擦除基名查；`element_type`
/// 原样传给 backing，反射拿到的是实例化后的元素类型。这次查表本来就在建数组的路上（判 struct）。
pub fn elem_mode(ctx: &VmContext, element_type: ElemType, type_param: bool) -> ElemMode {
    let kind = element_type.kind();
    if kind != ElemKind::Mixed || type_param {
        return ElemMode::Kind(kind);
    }
    let name: &str = &element_type;
    let td = match ctx.try_lookup_type(name) {
        Some(td) => td,
        None => match ctx.try_lookup_type(name.split('<').next().unwrap_or(name)) {
            Some(td) => td,
            None => return ElemMode::Kind(ElemKind::Mixed),
        },
    };
    if td.is_struct() {
        // 闸门与编译器 `StructLayout.IsBlobStruct` 必须逐字一致（FieldCount >= 1）；全 VM 唯一一份镜像。
        if let (false, Some(layout)) = (td.fields.is_empty(), td.struct_layout()) {
            if layout.size != 0 {
                return ElemMode::Struct(layout);
            }
        }
        return ElemMode::Kind(ElemKind::Mixed);
    }
    if td.is_enum() {
        return ElemMode::Kind(ElemKind::Mixed);
    }
    ElemMode::Kind(ElemKind::Ref)
}

/// 把一个 struct 值打包进刚分配的 `struct[]` 的第 `i` 个元素（`new P[]{ p1, p2 }`）。
/// 源是 arena `StructRef` 或装箱 `BoxedStruct`；`Null` = 默认值（元素保持零初始化）。
pub fn pack_struct_elem(ctx: &VmContext, arr: &ArrayObj, i: usize, v: &Value) -> OpResult<()> {
    let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = match v {
        Value::BoxedStruct(b) => b.borrow().boxed_blob(),
        Value::StructRef { idx, frame_id } =>
            ctx.struct_arena.lock().with(*idx, *frame_id, |s| (s.bytes.to_vec(), s.refs.to_vec()))?,
        Value::Null => return Ok(()),
        other => return Err(OpError::internal(format!(
            "struct array literal element must be a struct value, got {other:?}"))),
    };
    // A fresh, young array not yet reachable from any register: no barrier owed.
    arr.write_struct_elem(i, &src_bytes, &src_refs, &mut |_| {})?;
    Ok(())
}
