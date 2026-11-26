//! 数组：分配、元素读写、长度的唯一实现，外加 ref 解引用与 JIT 打包快路用的原语。
//!
//! 数组形态：堆数组 `Array`（变长区里的一块：头 + 元素；REF / MIXED / PRIM / STRUCT / FROZEN，见
//! `object::array::array`）、栈数组 `StackArray`（逃逸分析，经 per-context 栈 arena）。模式在分配时由元素类型定
//! （[`elem_mode`]）。`Null` → `NullReferenceException`；
//! 下标越界（含负数）→ `IndexOutOfRangeException`；长度为负 → `OverflowException`。

use crate::object::{
    default_value_for_tag, ArrayMode, ArrayObj, ElemKind, ElemType, StructArrayElem,
    StructTypeLayout,
};
use crate::object::Value;
use crate::object::ArrRef;
use crate::vm::VmContext;

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
/// 擦除型参读取的值可以跨帧传输，返回独立装箱快照。
#[inline(always)]
pub fn array_get<F: FnOnce() -> u32>(ctx: &VmContext, arr: &Value, idx: &Value, erased: bool, frame_id: F) -> OpResult<Value> {
    if let Value::Array(rc) = arr {
        let a = rc.borrow();
        let i = checked_index(ArrayOp::Get, idx, a.len())?;
        if a.mode() != ArrayMode::Struct {
            return Ok(a.get_boxed(i));   // 打包基元在这里装回 Value
        }
        if erased {
            return super::array_bulk::elem_get_boxed(ctx, rc, i);
        }
        return Ok(struct_elem_handle(ctx, *rc, i, frame_id()));
    }
    array_get_rare(ctx, arr, idx)
}

/// 值 struct 数组元素的 `StructRefHeap` 句柄：载荷进 transient arena，盖上当前帧号。
#[inline(never)]
fn struct_elem_handle(ctx: &VmContext, arr: ArrRef, i: usize, fid: u32) -> Value {
    ctx.exec().alloc_struct_elem(fid, StructArrayElem { arr, index: i as u32 })
}

#[inline(never)]
fn array_get_rare(ctx: &VmContext, arr: &Value, idx: &Value) -> OpResult<Value> {
    match arr {
        Value::StackArray { idx: aidx, frame_id: afid } => {
            ctx.exec.stack_arena.lock().with_arr(*aidx, *afid, |a| {
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
        let i = {
            let a = rc.borrow();
            let i = checked_index(ArrayOp::Set, idx, a.len())?;
            // Erased default(T) in existing bytecode can be Null for a value struct.
            if matches!(v, Value::Null) && a.mode() == ArrayMode::Struct {
                return Ok(a.clear_struct_elem(i)?);
            }
            i
        };
        crate::object::store_array_element(ctx.heap(), rc, i, v);
        return Ok(());
    }
    array_set_rare(ctx, arr, idx, v)
}

#[inline(never)]
fn array_set_rare(ctx: &VmContext, arr: &Value, idx: &Value, v: &Value) -> OpResult<()> {
    match arr {
        Value::StackArray { idx: aidx, frame_id } => {
            ctx.exec.stack_arena.lock().with_arr_mut(*aidx, *frame_id, |a| {
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
            Ok(ctx.exec.stack_arena.lock().with_arr(*idx, *frame_id, |a| a.len() as i64)?)
        }
        other => Err(not_an_array(ArrayOp::Len, other)),
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
    let len = gc.borrow().len();
    if i >= len { return Err(OpError::index_out_of_range(i as i64, len)); }
    crate::object::store_array_element(ctx.heap(), gc, i, v);
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
/// - 元素是 blob 值 struct → STRUCT 堆数组（不走栈分配）。
/// - `tp_prim`：泛型型参在运行期解析成**基元**实参时的（打包种类, 零值）——数组按该宽度打包
///   （M13，`List<int>` 的 backing 每元素 4 B）。方法级型参从当前激活解析。
/// - `bound`：`new T[n]` 在运行期绑定的实参。已绑定的 blob struct 使用连续布局；
///   引用和未绑定的型参使用擦除存储。反射报告实际绑定的元素类型。
/// - `stack`：编译器证明不逃逸且运行期允许时给出栈帧号，数组进栈 arena。
#[allow(clippy::too_many_arguments)]
pub fn array_new<F: FnOnce() -> u32>(
    ctx: &VmContext, len: &Value, elem_tag: u8, element_type: ElemType,
    type_param: bool, tp_prim: Option<TypeParamElem>, bound: Option<ElemType>, stack: Option<F>,
) -> OpResult<Value> {
    let n = array_size(len)?;
    if type_param && tp_prim.is_none() {
        if let Some(elem) = bound {
            if let ElemMode::Struct(layout) = elem_mode(ctx, elem, false) {
                return alloc(ArrayObj::alloc_struct(ctx.heap(), elem, n, layout), || format!("struct array[{n}]"));
            }
        }
    }
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

/// Bind parameters inside a constructed element name using the current activation.
#[inline]
pub fn array_elem_for_activation<'a>(
    ctx: &VmContext, element: ElemType, method_args: &[String],
    receiver: impl FnOnce() -> Option<&'a Value>,
) -> ElemType {
    if !element.has_type_param_markers() {
        return element;
    }
    let env = crate::exec::type_param_env::TypeParamEnv::new(method_args, receiver());
    ElemType::intern(&env.substitute(ctx, &element))
}

/// `new T[n]`'s element type as bound at run time: the argument's name, interned. `None` when
/// the parameter is not bound here (the array keeps reporting `T`).
pub fn type_param_bound_elem(arg: Option<&str>) -> Option<ElemType> {
    arg.filter(|a| !a.is_empty() && !crate::zpkg::index::type_param_markers::has_markers(a)).map(ElemType::intern)
}

/// Resolve a class-level array element on a heap or stack receiver.
pub fn class_type_param_array_args(
    ctx: &VmContext, receiver: Option<&Value>, index: usize, owner: Option<&str>,
) -> (Option<TypeParamElem>, Option<ElemType>) {
    let Some(receiver) = receiver else { return (None, None) };
    let own_args = |object: &crate::object::ScriptObject| {
        let name = &object.type_desc.name;
        let definition = crate::object::TypeDesc::erased_definition_name(name).unwrap_or(name);
        owner.is_none_or(|owner| definition == owner).then(|| object.type_args_handle())
    };
    let args = match receiver {
        Value::Object(rc) => own_args(&rc.borrow()),
        Value::StackObject { idx, frame_id } => {
            ctx.exec.stack_arena.lock().with_obj(*idx, *frame_id, own_args).ok().flatten()
        }
        _ => return (None, None),
    };
    if let Some(args) = args {
        return type_param_array_args(args.get(index).map(String::as_str), args.elem_kind(index));
    }
    // A derived receiver carries its own arguments. Resolve the parameter at the
    // base level whose code issued ArrayNew.
    let arg = crate::exec::type_param_env::class_type_arg(ctx, receiver, index, owner);
    let kind = arg.as_deref().map(ElemKind::of_type_param).unwrap_or(ElemKind::Mixed);
    type_param_array_args(arg.as_deref(), kind)
}

/// Resolve a method-level array element from the current activation's arguments.
pub fn method_type_param_array_args(
    args: &[String], index: usize,
) -> (Option<TypeParamElem>, Option<ElemType>) {
    let Some(name) = args.get(index) else { return (None, None) };
    type_param_array_args(Some(name), ElemKind::of_type_param(name))
}

pub(crate) fn type_param_array_args(arg: Option<&str>, kind: ElemKind) -> (Option<TypeParamElem>, Option<ElemType>) {
    let bound = type_param_bound_elem(arg);
    let prim = if kind.is_prim() {
        arg.map(crate::object::default_value_for)
            .filter(|zero| !matches!(zero, Value::Null))
            .map(|zero| TypeParamElem { kind, zero })
    } else {
        None
    };
    (prim, bound)
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
    let idx = ctx.exec().stack_alloc_arr(frame_id, crate::exec::stack_alloc::StackArray::new(elems));
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
    let td = match ctx.loader().try_lookup_type(name) {
        Some(td) => td,
        None => match ctx.loader().try_lookup_type(name.split('<').next().unwrap_or(name)) {
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
            ctx.exec.struct_arena.lock().with(*idx, *frame_id, |s| (s.bytes.to_vec(), s.refs.to_vec()))?,
        Value::Null => return Ok(()),
        other => return Err(OpError::internal(format!(
            "struct array literal element must be a struct value, got {other:?}"))),
    };
    // A fresh, young array not yet reachable from any register: no barrier owed.
    arr.write_struct_elem(i, &src_bytes, &src_refs, &mut |_| {})?;
    Ok(())
}
