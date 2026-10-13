//! 对象引用字段的 8 B 自描述字（object model R1）。
//!
//! 对象里每个引用字段是一个 8 B 字，单次原子读写：低 3 位记**种类**，其余位是句柄本身
//! （`GcRef` / `VarGcRef` 的原始位，二者都 8 字节对齐，低 3 位恒为 0）。整个字为 0 = `null`。
//! 16 B 的 `Value` 只活在寄存器里；字与 `Value` 之间的转换只在这里。
//!
//! | 种类 | `Value` | 句柄 |
//! |---|---|---|
//! | 0（整字为 0） | `Null` | — |
//! | 1 | `Object` | `ObjRef`（变长区块） |
//! | 2 | `Array` | `GcRef<ArrayObj>` |
//! | 3 | `Str` | `VarGcRef` |
//! | 4 | `Closure` | `VarGcRef` |
//! | 5 | `FuncRef` | 不是句柄：`FnId << 3`，JIT 读走慢路径 |
//! | 6 | `BoxedStruct` | `ObjRef` |
//! | 7 | 装箱的任意 `Value`（逃生口） | `GcRef<ArrayObj>`，单元素数组 |
//!
//! **种类 7 是逃生口**：`object` / 接口字段能经擦除泛型收到裸的 `I64` / `F64` / `Bool` / `Char`
//! （`void Set<T>(H h, T t) { h.O = t; }` 以 `T = int` 调用），这些值装不进 8 B。写入时把它装进一个
//! 只读的单元素数组，读出时再取回原值 —— 对用户代码完全透明，只多一次分配。型参字段（`T F;`）
//! 不是单个引用字，而是标签字 + 负载字的 16 B 单元（`tparam_cell`）：基元进负载字，否则
//! `LinkedListNode<int>` 之类的节点每次写都要分配；它的标签字沿用本模块的编码。
//!
//! GC 追踪用 [`decode_for_trace`]：种类 7 交出盒子本身（`Value::Array`），盒子再交出里面的值。

use super::{ArrayObj, Cells, FieldWrite, Value};
use crate::gc::ObjRef;
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
pub static KIND_TO_VALUE_TAG: [u8; 8] = [5, 7, 6, 4, 10, SLOW_PATH_TAG, 17, SLOW_PATH_TAG];
pub const SLOW_PATH_TAG: u8 = 0xFF;

/// JIT 写路径的查表（[`KIND_TO_VALUE_TAG`] 的逆）：寄存器里 `Value` 的判别值 → 引用字种类，
/// 字 = `负载 | 种类`（`Null` 的字是 0，种类 0）。[`SLOW_PATH_KIND`] = 这个值不能直接编码：基元
/// （要装箱）、栈上句柄，以及 `FuncRef`（载荷只有低 4 字节有效）→ helper。下标是判别值（< 32）。
/// 由 `ref_word_tests::the_jit_store_table_matches_encode` 钉住。
pub static VALUE_TAG_TO_KIND: [u8; 32] = {
    let mut t = [SLOW_PATH_KIND; 32];
    t[4] = RK_STR as u8;            // Str
    t[5] = 0;                       // Null
    t[6] = RK_ARRAY as u8;          // Array
    t[7] = RK_OBJECT as u8;         // Object
    t[10] = RK_CLOSURE as u8;       // Closure
    t[17] = RK_BOXED_STRUCT as u8;  // BoxedStruct
    t
};
pub const SLOW_PATH_KIND: u8 = 0xFF;

// 句柄的低 3 位必须空着。64 位目标上 entry / 块头都含指针宽字段，天然 8 对齐；32 位目标由
// `RegionEntry` 的 `repr(align(8))` 与 `GcBlockHeader` 的 `align(8)` 保证。对象（R6）与字符串、
// 闭包一样是变长区块，句柄指向块头。
const _: () = assert!(std::mem::align_of::<crate::gc::var_region::GcBlockHeader>() >= 8);
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
        // FuncRef 不是堆引用：字 = `FnId << 3 | 种类`，没有 GC 边。
        Value::FuncRef(id) => ((id.0 as u64) << 3) | RK_FUNC_REF,
        Value::BoxedStruct(r) => tag(r.to_tagged_bits(), RK_BOXED_STRUCT),
        _ => return None,
    })
}

/// 装箱逃生口的字：`boxed` 是一个只装了原值的单元素数组。
#[inline]
pub fn encode_boxed(boxed: &GcRef<ArrayObj>) -> u64 {
    tag(boxed.to_tagged_bits(), RK_BOXED_VALUE)
}

/// 用当前线程的环境堆（`gc::ambient`）分配逃生盒子：只读的单元素数组，元素是原样的 16 B `Value`
/// （`ArrayBacking::Frozen` —— 盒子里可以是栈句柄，不能再按单元编码，否则会无限装箱）。
/// 没有活动堆（不带 VM 的单测）时报内部错误。
pub fn alloc_box_ambient(v: &Value) -> anyhow::Result<GcRef<ArrayObj>> {
    let heap = crate::gc::ambient::current_heap().ok_or_else(|| {
        anyhow::anyhow!("cannot store {v:?} into a reference cell: no active heap to box it")
    })?;
    let boxed = ArrayObj::frozen(heap, vec![*v]);
    match heap.alloc_array_obj(boxed) {
        Value::Array(r) => Ok(r),
        other => anyhow::bail!("boxing a cell value: heap returned {other:?}"),
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
/// `w` 是 [`encode`] / [`encode_boxed`] 写下、且所指对象仍活着的字（GC 追踪与刚从活对象里读出
/// 这个字、中间没过 safepoint 的读者都满足：字里的引用是对象的强边）。
#[inline(always)]
pub unsafe fn decode_for_trace(w: u64) -> Value {
    let h = handle_bits(w);
    // SAFETY（下面每个 from_*）：`h` 来自同一种句柄的 `to_*bits`，调用方保证对象仍活着。
    unsafe {
        match w & KIND_MASK {
            RK_OBJECT => ObjRef::from_tagged_bits(h).map_or(Value::Null, Value::Object),
            RK_ARRAY | RK_BOXED_VALUE => GcRef::from_tagged_bits(h).map_or(Value::Null, Value::Array),
            RK_STR => VarGcRef::from_bits(h).map_or(Value::Null, |b| Value::Str(Str::from_var_ref(b))),
            RK_CLOSURE => VarGcRef::from_bits(h).map_or(Value::Null, Value::Closure),
            RK_FUNC_REF => Value::FuncRef(crate::metadata::tokens::FnId((w >> 3) as u32)),
            RK_BOXED_STRUCT => ObjRef::from_tagged_bits(h).map_or(Value::Null, Value::BoxedStruct),
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

// ── 引用单元：对象字段、struct 引用叶子、REF 数组元素共用 ────────────────────────────

/// 读 `off` 处的引用单元（种类 7 取出盒子里的原值）。
#[inline(always)]
pub fn load(c: &Cells, off: usize) -> Value {
    // SAFETY: the word was written by `store` (or is the zero-initialised `null`); its referent is
    // kept alive by the owner, which the caller holds.
    unsafe { decode(c.load_ref_word(off)) }
}

/// 写 `off` 处的引用单元：能编码就写引用字，否则装箱（种类 7）。被覆盖的旧字交给 SATB；返回值告诉
/// 调用方该给写屏障哪个引用（[`FieldWrite::with_barrier_value`]）。
#[inline(always)]
pub fn store(c: &Cells, off: usize, v: &Value) -> anyhow::Result<FieldWrite> {
    match encode(v) {
        Some(w) => {
            store_word_recorded(c, off, w);
            Ok(FieldWrite::Ref)
        }
        None => store_boxed(c, off, v),
    }
}

#[cold]
#[inline(never)]
fn store_boxed(c: &Cells, off: usize, v: &Value) -> anyhow::Result<FieldWrite> {
    let b = alloc_box_ambient(v)?;
    store_word_recorded(c, off, encode_boxed(&b));
    Ok(FieldWrite::Boxed(b))
}

/// 写引用字，把它替换掉的字交给 SATB 删除屏障（进行中的 major 标记必须仍看得见被切断的对象）。
#[inline(always)]
pub fn store_word_recorded(c: &Cells, off: usize, w: u64) {
    let old = c.store_ref_word(off, w);
    if old != 0 {
        // SAFETY: `old` was this cell's word an instant ago, so its referent is still live (no
        // safepoint in between).
        crate::gc::satb::record_overwrite(&unsafe { decode_for_trace(old) });
    }
}

/// GC 追踪：`off` 处的引用单元若非 null，交出它（种类 7 交出盒子）。
#[inline(always)]
pub fn visit(c: &Cells, off: usize, visit: &mut dyn FnMut(&Value)) {
    let w = c.load_ref_word(off);
    if w != 0 {
        // SAFETY: a reference word of a reachable owner names a live referent.
        visit(&unsafe { decode_for_trace(w) });
    }
}

/// 断开 `off` 处的引用（GC 清扫死对象，不过屏障）。
#[inline(always)]
pub fn clear(c: &Cells, off: usize) {
    c.store_word(off, 0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
#[path = "ref_word_tests.rs"]
pub(crate) mod ref_word_tests;
