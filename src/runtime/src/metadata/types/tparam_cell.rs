//! 型参字段的 16 B 单元（object model R2）。
//!
//! 型参字段（`T F;` / `T? F;`）在擦除泛型下能收到任何 `Value`：`T = int` 的实例收到 `I64`，
//! 也可能收到 `null`；没带实参分配的实例（擦除代码里的 `new Node<T>()`）什么都可能收到。
//! 单元是对象 `bytes` 里的两个 8 B 字，各自单次原子读写：
//!
//! - **标签字**（W0）：编译器布局里这个字段自己的 8 B 槽位。自描述，acquire 读 / release 写。
//! - **负载字**（W1）：编译器布局之后追加的 8 B（`ObjectLayout::tparam_payload_offset`），
//!   **只装基元的原始位**，relaxed 读写。
//!
//! | 标签字 | 值 | 单元存过基元吗 |
//! |---|---|---|
//! | `0` | `null` | 从未（分配时的状态） |
//! | 引用字，种类 1–6（`ref_word`） | 这个引用 | 从未 |
//! | 引用字，种类 7 | 盒子里的值（R1 逃生口）；裸的 `7`（空句柄）= `null` | 可能存过 |
//! | `PRIM(k)` = `k << 4` | 种类 `k` 的基元，原始位在负载字 | 种类定为 `k` |
//! | `NULL(k)` = `k << 4 \| 8` | `null` | 种类定为 `k` |
//!
//! `k`：1 = `I64`，2 = `F64`，3 = `Bool`，4 = `Char`。标记字低 3 位为 0 且非 0，与引用字不相交
//! （引用字低 3 位是种类，整字为 0 才是 null）。
//!
//! **不变式：负载字一生只装一种基元的位。** 种类由唯一一次成功的「认领」（标签字 CAS `0 → NULL(k)`）
//! 定下，之后标签字只在 `PRIM(k)` / `NULL(k)` / 种类 7 之间变，不再回到 `0` 或直接引用字（那两种状态
//! 表示「从未存过基元」，从它们出发的写一律 CAS）。于是：
//! - 读者 acquire 到 `PRIM(k)` 后 relaxed 读负载字，读到的必是某次写入的 `k` 种位 —— 不会把别的种类的位、
//!   更不会把引用当基元解读（负载字从不装引用，GC 也不看它）。
//! - 写 `k` 种基元：已是 `PRIM(k)` → 只写负载字；`NULL(k)` → 先写负载字，再 release 写 `PRIM(k)`。
//! - `null` 与引用不需要负载字：从未存过基元的单元里，引用就是标签字本身（直接引用字），`null` 是 `0`；
//!   种类已定的单元里 `null` 是 `NULL(k)`。
//! - 其余不符的写装箱成种类 7（逃生口）：种类已定后来了引用或别种基元、直接引用状态下来了基元、
//!   栈句柄等。这是擦除泛型下「同一个 `T` 字段先后放不同类型的值」才会走到的路，z42c 工作区构建与
//!   e2e 里几乎为 0。种类 7 之后单元留在标签字里（它可能存过某种基元，而那种是什么已经不知道了）。
//!
//! **为什么种类不在分配时写死**：分配点常常不知道实参 —— 泛型代码里的 `new Node<T>()` 带的是字面的 `T`
//! （`LinkedList<int>.AddLast` 的节点即如此）。若这种实例一律按引用编码，每次存 `int` 都要装箱。改成
//! 「第一次存基元时认领、之后不变」，这些实例照样走快路，且不需要在运行期解析开放实参（那会改变
//! 反射 / `default(T)` 的可见行为）。实参是已知基元时，`ObjNew` 的零值初始化（`generic_field_zero_overrides`）
//! 本身就是一次基元写，分配时即定下种类。
//!
//! 写屏障：每次改写标签字都经 [`publish`] 或 CAS 拿到旧字，旧字是引用时交给 SATB；新落进单元的引用
//! 经 `FieldWrite` 交给调用方的 `write_barrier_field`。GC 只追踪标签字（`visit`）。

use std::sync::atomic::Ordering;

use super::ref_word::{self, KIND_MASK, RK_BOXED_VALUE};
use super::{ArrayObj, FieldWrite, ObjStorage, Value};
use crate::gc::GcRef;

const NULL_FLAG: u64 = 8;
const K_I64: u64 = 1;
const K_F64: u64 = 2;
const K_BOOL: u64 = 3;
const K_CHAR: u64 = 4;

/// 标签字：种类 `k` 的基元在负载字里。
#[inline(always)]
pub const fn prim_word(k: u64) -> u64 {
    k << 4
}

/// 标签字：`null`，种类 `k` 已定。
#[inline(always)]
pub const fn null_word(k: u64) -> u64 {
    (k << 4) | NULL_FLAG
}

/// 标签字：装箱的 `null`（种类 7、空句柄）——单元可能存过基元、又不知道是哪种时的 `null`。
pub const BOXED_NULL: u64 = RK_BOXED_VALUE;

/// `PRIM(k)` / `NULL(k)`：种类已定的标记字。
#[inline(always)]
fn is_marker(w0: u64) -> bool {
    w0 != 0 && w0 & KIND_MASK == 0
}

/// 基元 `Value` → (种类, 原始位)；不是基元 → `None`。
#[inline(always)]
fn prim_parts(v: &Value) -> Option<(u64, u64)> {
    Some(match v {
        Value::I64(n) => (K_I64, *n as u64),
        Value::F64(f) => (K_F64, f.to_bits()),
        Value::Bool(b) => (K_BOOL, *b as u64),
        Value::Char(c) => (K_CHAR, *c as u64),
        _ => return None,
    })
}

#[inline(always)]
fn prim_value(k: u64, bits: u64) -> Value {
    match k {
        K_I64 => Value::I64(bits as i64),
        K_F64 => Value::F64(f64::from_bits(bits)),
        K_BOOL => Value::Bool(bits != 0),
        _ => Value::Char(char::from_u32(bits as u32).unwrap_or('\0')),
    }
}

/// 读单元。
#[inline]
pub fn load(st: &ObjStorage, w0_off: u32, w1_off: u32) -> Value {
    let (w0, w1) = st.load_tparam(w0_off as usize, w1_off as usize);
    if w0 & KIND_MASK != 0 {
        // SAFETY: a reference word of this cell (written by `store`); its referent is kept
        // alive by the object, which the caller holds.
        return unsafe { ref_word::decode(w0) };
    }
    if w0 == 0 || w0 & NULL_FLAG != 0 {
        return Value::Null;
    }
    // `PRIM(k)`: the payload was loaded after the tag word's acquire, which orders the payload
    // written before `PRIM(k)` was published.
    prim_value(w0 >> 4, w1)
}

/// 写单元。返回值交给调用方发写屏障（见模块文档）。
#[inline]
pub fn store(st: &mut ObjStorage, w0_off: u32, w1_off: u32, v: &Value) -> anyhow::Result<FieldWrite> {
    let (w0_off, w1_off) = (w0_off as usize, w1_off as usize);
    let prim = prim_parts(v);
    if let Some((k, bits)) = prim {
        // Fast path: the cell already holds a `k` primitive — only the payload changes.
        if st.store_tparam_payload_if(w0_off, w1_off, prim_word(k), bits) {
            return Ok(FieldWrite::NotRef);
        }
    }
    let w0 = st.load_word(w0_off, Ordering::Relaxed);
    store_transition(st, w0_off, w1_off, v, prim, w0)
}

/// Every write that changes the tag word.
#[inline(never)]
fn store_transition(
    st: &mut ObjStorage, w0_off: usize, w1_off: usize, v: &Value, prim: Option<(u64, u64)>, mut w0: u64,
) -> anyhow::Result<FieldWrite> {
    let w0 = &mut w0;
    loop {
        if is_marker(*w0) {
            // The variant is fixed: `k` primitives and `null` keep it, anything else is boxed.
            let k = *w0 >> 4;
            return Ok(match prim {
                Some((pk, bits)) if pk == k => {
                    st.store_word(w1_off, bits, Ordering::Relaxed);
                    publish(st, w0_off, prim_word(k));
                    FieldWrite::NotRef
                }
                _ if matches!(v, Value::Null) => {
                    publish(st, w0_off, null_word(k));
                    FieldWrite::NotRef
                }
                _ => store_boxed(st, w0_off, v)?,
            });
        }
        if *w0 & KIND_MASK == RK_BOXED_VALUE {
            // The cell may have held a primitive of a variant nobody remembers: it never goes
            // back to the "never held a primitive" states, so everything stays in the tag word.
            if matches!(v, Value::Null) {
                publish(st, w0_off, BOXED_NULL);
                return Ok(FieldWrite::NotRef);
            }
            return store_boxed(st, w0_off, v);
        }
        // `0` or a direct reference word: the cell never held a primitive. Every transition
        // out of here is a CAS, so a racing claim is never undone.
        let (new, wrote) = match (v, prim) {
            (Value::Null, _) => (0, FieldWrite::NotRef),
            (_, Some((k, bits))) if *w0 == 0 => {
                // Claim the variant, then publish the value.
                match st.cas_word(w0_off, 0, null_word(k)) {
                    Ok(_) => {
                        st.store_word(w1_off, bits, Ordering::Relaxed);
                        publish(st, w0_off, prim_word(k));
                        return Ok(FieldWrite::NotRef);
                    }
                    Err(actual) => {
                        *w0 = actual;
                        continue;
                    }
                }
            }
            (_, None) => match ref_word::encode(v) {
                Some(w) => (w, FieldWrite::Ref),
                None => {
                    let b = box_value(v)?;
                    (ref_word::encode_boxed(&b), FieldWrite::Boxed(b))
                }
            },
            // A primitive while the cell holds a reference: claiming would hide that reference
            // from a concurrent reader, so box it.
            (_, Some(_)) => {
                let b = box_value(v)?;
                (ref_word::encode_boxed(&b), FieldWrite::Boxed(b))
            }
        };
        match st.cas_word(w0_off, *w0, new) {
            Ok(old) => {
                record(old);
                return Ok(wrote);
            }
            Err(actual) => *w0 = actual,
        }
    }
}

/// Box `v` into the tag word (kind 7).
#[cold]
fn store_boxed(st: &mut ObjStorage, w0_off: usize, v: &Value) -> anyhow::Result<FieldWrite> {
    let b = box_value(v)?;
    publish(st, w0_off, ref_word::encode_boxed(&b));
    Ok(FieldWrite::Boxed(b))
}

#[cold]
#[inline(never)]
fn box_value(v: &Value) -> anyhow::Result<GcRef<ArrayObj>> {
    #[cfg(test)]
    tparam_cell_tests::count_box();
    ref_word::alloc_box_ambient(v)
}

/// Release-store a tag word. While a major mark is running it is a swap, and a replaced
/// reference word goes to the SATB deletion barrier.
#[inline(always)]
fn publish(st: &mut ObjStorage, w0_off: usize, w: u64) {
    if crate::gc::satb::marking_any() {
        record(st.swap_word(w0_off, w));
    } else {
        st.store_word(w0_off, w, Ordering::Release);
    }
}

/// Hand a replaced tag word to the SATB barrier when it was a reference word.
#[inline(always)]
fn record(old: u64) {
    if old & KIND_MASK != 0 {
        // SAFETY: `old` was this cell's word an instant ago (no safepoint in between), so its
        // referent is still live.
        crate::gc::satb::record_overwrite(&unsafe { ref_word::decode_for_trace(old) });
    }
}

/// The cell's reference edge for GC tracing, if its tag word holds one (a boxed value yields
/// its box). The payload word never holds a reference.
#[inline]
pub fn visit(st: &ObjStorage, w0_off: u32, visit: &mut dyn FnMut(&Value)) {
    let w0 = st.load_word(w0_off as usize, Ordering::Acquire);
    if w0 & KIND_MASK != 0 && ref_word::handle_bits(w0) != 0 {
        // SAFETY: a reference word of a reachable object names a live referent.
        visit(&unsafe { ref_word::decode_for_trace(w0) });
    }
}

/// Break the cell's reference edge (GC sweeping a dead object — no barrier).
#[inline]
pub fn clear(st: &mut ObjStorage, w0_off: u32) {
    let w0 = st.load_word(w0_off as usize, Ordering::Relaxed);
    if w0 & KIND_MASK != 0 {
        st.store_word(w0_off as usize, 0, Ordering::Relaxed);
    }
}

#[cfg(test)]
#[path = "tparam_cell_tests.rs"]
pub(crate) mod tparam_cell_tests;
