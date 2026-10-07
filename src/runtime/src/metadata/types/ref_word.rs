//! 对象引用字段的 8 B 自描述字（object model R1）。
//!
//! 对象里每个引用字段是一个 8 B 字，单次原子读写：低 3 位记**种类**，其余位是句柄本身
//! （`GcRef` / `VarGcRef` 的原始位，二者都 8 字节对齐，低 3 位恒为 0）。整个字为 0 = `null`。
//! 16 B 的 `Value` 只活在寄存器里；字与 `Value` 之间的转换只在这里。
//!
//! | 种类 | `Value` | 句柄 |
//! |---|---|---|
//! | 0（整字为 0） | `Null` | — |
//! | 1 | `Object` | `GcRef<ScriptObject>` |
//! | 2 | `Array` | `GcRef<ArrayObj>` |
//! | 3 | `Str` | `VarGcRef` |
//! | 4 | `Closure` | `VarGcRef` |
//! | 5 | `FuncRef` | `VarGcRef` |
//! | 6 | `BoxedStruct` | `GcRef<ScriptObject>` |
//! | 7 | 装箱的任意 `Value`（逃生口） | `GcRef<ArrayObj>`，单元素数组 |
//!
//! **种类 7 是逃生口**：`object` / 接口字段能经擦除泛型收到裸的 `I64` / `F64` / `Bool` / `Char`
//! （`void Set<T>(H h, T t) { h.O = t; }` 以 `T = int` 调用），这些值装不进 8 B。写入时把它装进一个
//! 只读的单元素数组，读出时再取回原值 —— 对用户代码完全透明，只多一次分配。型参字段（`T F;`）
//! 不走这里：它们留在 16 B 侧表（见 `ObjectLayout::ref_offsets`），否则 `List<int>` 之类的节点
//! 每次写都要分配。
//!
//! GC 追踪用 [`decode_for_trace`]：种类 7 交出盒子本身（`Value::Array`），盒子再交出里面的值。

use super::{ArrayObj, ScriptObject, Value};
use crate::gc::var_region::VarGcRef;
use crate::gc::GcRef;
use crate::metadata::vstr::Str;

/// 低 3 位：种类。
pub const KIND_MASK: u64 = 7;
pub const RK_OBJECT: u64 = 1;
pub const RK_ARRAY: u64 = 2;
pub const RK_STR: u64 = 3;
pub const RK_CLOSURE: u64 = 4;
pub const RK_FUNC_REF: u64 = 5;
pub const RK_BOXED_STRUCT: u64 = 6;
pub const RK_BOXED_VALUE: u64 = 7;

/// JIT 读路径的查表：种类 → 寄存器里 `Value` 的判别值。`null` 的字是 0 ⇒ 种类 0 ⇒ `Null`(5)，
/// 负载随之为 0。种类 7 是 [`SLOW_PATH_TAG`]：要回落到 helper 取盒子里的值。判别值由
/// `metadata/types_tests.rs::value_discriminants_pinned` 钉住。
pub static KIND_TO_VALUE_TAG: [u8; 8] = [5, 7, 6, 4, 10, 9, 17, SLOW_PATH_TAG];
pub const SLOW_PATH_TAG: u8 = 0xFF;

// 句柄的低 3 位必须空着。64 位目标上 entry / 块头都含指针宽字段，天然 8 对齐；32 位目标由
// `RegionEntry` 的 `repr(align(8))` 与 `GcBlockHeader` 的 `align(8)` 保证。
const _: () = assert!(std::mem::align_of::<crate::gc::region::RegionEntry<ScriptObject>>() >= 8);
const _: () = assert!(std::mem::align_of::<crate::gc::region::RegionEntry<ArrayObj>>() >= 8);

#[inline(always)]
fn tag(bits: u64, kind: u64) -> u64 {
    debug_assert_eq!(bits & KIND_MASK, 0, "heap handle is not 8-aligned: {bits:#x}");
    bits | kind
}

/// `Value` → 字。`None` = 这个值装不进 8 B（基元、栈上句柄），调用方要先装箱（[`encode_boxed`]）。
#[inline(always)]
pub fn encode(v: &Value) -> Option<u64> {
    Some(match v {
        Value::Null => 0,
        Value::Object(r) => tag(r.to_tagged_bits(), RK_OBJECT),
        Value::Array(r) => tag(r.to_tagged_bits(), RK_ARRAY),
        Value::Str(s) => tag(s.var_ref().to_bits(), RK_STR),
        Value::Closure(c) => tag(c.to_bits(), RK_CLOSURE),
        Value::FuncRef(s) => tag(s.var_ref().to_bits(), RK_FUNC_REF),
        Value::BoxedStruct(r) => tag(r.to_tagged_bits(), RK_BOXED_STRUCT),
        _ => return None,
    })
}

/// 装箱逃生口的字：`boxed` 是一个只装了原值的单元素数组。
#[inline]
pub fn encode_boxed(boxed: &GcRef<ArrayObj>) -> u64 {
    tag(boxed.to_tagged_bits(), RK_BOXED_VALUE)
}

/// 用当前线程的环境堆（`gc::ambient`）分配逃生盒子。没有活动堆（不带 VM 的单测）时报内部错误。
pub fn alloc_box_ambient(v: &Value) -> anyhow::Result<GcRef<ArrayObj>> {
    let heap = crate::gc::ambient::current_heap().ok_or_else(|| {
        anyhow::anyhow!("cannot store {v:?} into a reference field: no active heap to box it")
    })?;
    match heap.alloc_array(vec![*v]) {
        Value::Array(r) => Ok(r),
        other => anyhow::bail!("boxing a field value: heap returned {other:?}"),
    }
}

/// 字 → 句柄的原始位（去掉种类）。
#[inline(always)]
pub fn handle_bits(w: u64) -> u64 {
    w & !KIND_MASK
}

/// 字 → 追踪用的 `Value`：引用原样，种类 7 交出盒子（`Value::Array`）。
///
/// # Safety
/// `w` 是 [`encode`] / [`encode_boxed`] 写下、且所指对象仍活着的字（GC 追踪与持有对象锁的
/// 读者都满足：字里的引用是对象的强边）。
#[inline(always)]
pub unsafe fn decode_for_trace(w: u64) -> Value {
    let h = handle_bits(w);
    // SAFETY（下面每个 from_*）：`h` 来自同一种句柄的 `to_*bits`，调用方保证对象仍活着。
    unsafe {
        match w & KIND_MASK {
            RK_OBJECT => GcRef::from_tagged_bits(h).map_or(Value::Null, Value::Object),
            RK_ARRAY | RK_BOXED_VALUE => GcRef::from_tagged_bits(h).map_or(Value::Null, Value::Array),
            RK_STR => VarGcRef::from_bits(h).map_or(Value::Null, |b| Value::Str(Str::from_var_ref(b))),
            RK_CLOSURE => VarGcRef::from_bits(h).map_or(Value::Null, Value::Closure),
            RK_FUNC_REF => VarGcRef::from_bits(h).map_or(Value::Null, |b| Value::FuncRef(Str::from_var_ref(b))),
            RK_BOXED_STRUCT => GcRef::from_tagged_bits(h).map_or(Value::Null, Value::BoxedStruct),
            _ => {
                debug_assert_eq!(w, 0, "reference word with kind 0 but a non-zero handle");
                Value::Null
            }
        }
    }
}

/// 字 → 用户看到的 `Value`：种类 7 取出盒子里的原值，其余同 [`decode_for_trace`]。
///
/// # Safety
/// 同 [`decode_for_trace`]。
#[inline(always)]
pub unsafe fn decode(w: u64) -> Value {
    if w & KIND_MASK == RK_BOXED_VALUE {
        return unsafe { unbox(w) };
    }
    unsafe { decode_for_trace(w) }
}

#[cold]
#[inline(never)]
unsafe fn unbox(w: u64) -> Value {
    // SAFETY: 种类 7 的句柄来自 `encode_boxed(&GcRef<ArrayObj>)`。
    match unsafe { GcRef::<ArrayObj>::from_tagged_bits(handle_bits(w)) } {
        Some(b) => b.borrow().get(0).unwrap_or(Value::Null),
        None => Value::Null,
    }
}

#[cfg(test)]
#[path = "ref_word_tests.rs"]
pub(crate) mod ref_word_tests;
