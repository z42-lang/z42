//! 实例字段读写：`FieldGet` / `FieldSet` 的唯一实现，外加 ref 解引用与 JIT 提升快路
//! 用到的按名读写 / 槽位解析原语。
//!
//! 接收者分派（都在这里，两引擎不另写）：
//! - 堆对象 `Object`：FieldIC 命中直取槽位，未命中查 `field_index` 并装进 IC；引用写入走写屏障。
//! - 栈对象 `StackObject`：经 per-context 栈 arena（同一个 FieldIC，不走写屏障——arena 是根）。
//! - `Str` / `Array` / `StackArray` 的 `Length`（及 `ByteLength` / `Count`）、`PinnedView` 的 `ptr` / `len`。
//! - `BoxedStruct`：擦除返回位流出的值 struct 盒，按名读叶子（复用反射那一份）。
//! - `Null` → `NullReferenceException`。

use crate::metadata::resolver::{assert_field_ic_slot, field_ic_install, field_ic_lookup, FieldIC};
use crate::metadata::types::{FieldWrite, ScriptObject, TypeDesc};
use crate::metadata::Value;
use crate::gc::GcRef;
use crate::vm_context::VmContext;

use super::error::{OpError, OpResult};

/// 解析字段槽位：IC 命中直接返回；未命中查 `field_index` 并安装。字段不存在 → `None`。
#[inline(always)]
fn slot_of(td: &TypeDesc, name: &str, ic: Option<&FieldIC>) -> Option<usize> {
    let Some(ic) = ic else { return td.field_index.get(name).copied() };
    let recv = td.id.0;
    if let Some(slot) = field_ic_lookup(ic, recv) {
        assert_field_ic_slot(td, name, slot);
        return Some(slot as usize);
    }
    let slot = *td.field_index.get(name)?;
    field_ic_install(ic, recv, slot as u32);
    Some(slot)
}

/// 按槽位读；字段不存在读出 `Null`（与历史行为一致）。
#[inline(always)]
fn load(o: &ScriptObject, name: &str, ic: Option<&FieldIC>) -> Value {
    match slot_of(&o.type_desc, name, ic) {
        Some(slot) => o.field_value(slot),
        None => Value::Null,
    }
}

/// 写入（不含写屏障）。返回 `Some((slot, 写了什么))`；调用方据此发屏障。
#[inline(always)]
fn store(o: &mut ScriptObject, name: &str, v: &Value, ic: Option<&FieldIC>) -> OpResult<Option<(usize, FieldWrite)>> {
    let Some(slot) = slot_of(&o.type_desc, name, ic) else { return Ok(None) };
    match o.try_set_field_value(slot, v) {
        Ok(w) => Ok(Some((slot, w))),
        Err(_) => Err(OpError::field_store_rejected(name, v)),
    }
}

/// 引用单元写入后的屏障：只对落进单元的堆引用发（基元经擦除写进 `object` 字段时是装它的盒子）。
#[inline(always)]
fn barrier(ctx: &VmContext, owner: &Value, v: &Value, wrote: Option<(usize, FieldWrite)>) {
    if let Some((slot, w)) = wrote {
        w.with_barrier_value(v, |stored| ctx.heap().write_barrier_field(owner, slot, stored));
    }
}

/// `FieldGet`：读接收者 `recv` 的字段 `name`。
///
/// 堆对象是热路径，强制内联进两侧适配层（JIT helper 少一层调用）；其余形态走不内联的慢路。
#[inline(always)]
pub fn field_get(ctx: &VmContext, recv: &Value, name: &str, ic: Option<&FieldIC>) -> OpResult<Value> {
    match recv {
        Value::Object(rc) => Ok(load(&rc.borrow(), name, ic)),
        // `.Length` 落到 FieldGet 的两种常见接收者（词法器之类的热循环）也留在内联路径。
        Value::Str(s) if name == "Length" => Ok(Value::I64(crate::corelib::str_meta::char_len(s) as i64)),
        Value::Array(rc) if name == "Length" => Ok(Value::I64(rc.borrow().len() as i64)),
        _ => field_get_rare(ctx, recv, name, ic),
    }
}

#[inline(never)]
fn field_get_rare(ctx: &VmContext, recv: &Value, name: &str, ic: Option<&FieldIC>) -> OpResult<Value> {
    match recv {
        Value::Object(rc) => Ok(load(&rc.borrow(), name, ic)),
        Value::Str(s) => match name {
            "Length"     => Ok(Value::I64(crate::corelib::str_meta::char_len(s) as i64)),
            "ByteLength" => Ok(Value::I64(s.len() as i64)),
            other        => Err(OpError::internal(format!("string has no field `{other}`"))),
        },
        Value::Array(rc) => array_len_field(name, rc.borrow().len()),
        Value::Null => Err(OpError::null_field_read(name)),
        Value::StackObject { idx, frame_id } => {
            Ok(ctx.stack_arena.lock().with_obj(*idx, *frame_id, |o| load(o, name, ic))?)
        }
        Value::StackArray { idx, frame_id } => {
            let len = ctx.stack_arena.lock().with_arr(*idx, *frame_id, |a| a.len())?;
            array_len_field(name, len)
        }
        Value::PinnedView { idx, frame_id } => {
            let (ptr, len) = ctx.transient_arena.lock().with(*idx, *frame_id, |p| match p {
                crate::interp::transient_arena::TransientPayload::PinView(pv) => (pv.ptr, pv.len),
                _ => (0u64, 0u64),
            })?;
            match name {
                // Spec C4 — only `ptr` / `len` are exposed; element type (kind) stays internal.
                "ptr" => Ok(Value::I64(ptr as i64)),
                "len" => Ok(Value::I64(len as i64)),
                other => Err(OpError::internal(format!(
                    "PinnedView has no field `{other}` (only `ptr` / `len`)"))),
            }
        }
        // accept-boxed-struct-field-get：值 struct 经擦除的返回位流出泛型函数时，运行期是带完整
        // `TypeDesc` + `struct_layout` 的堆盒，调用点静态类型是裸 `T` ⇒ 落到通用 FieldGet。语义与
        // 反射 `GetValue` 是同一件事，复用 `boxed_struct_field_get`，不另写一份布局复刻。
        // `field_set` 刻意不跟：写进临时盒是静默丢弃写，正解是编译期拒绝。
        Value::BoxedStruct(gc) => {
            Ok(crate::corelib::reflection::accessors::boxed_struct_field_get(ctx, gc, name)?)
        }
        other => Err(OpError::internal(format!(
            "FieldGet: expected object, got {other:?} (field `{name}`)"))),
    }
}

#[inline]
fn array_len_field(name: &str, len: usize) -> OpResult<Value> {
    match name {
        "Length" | "Count" => Ok(Value::I64(len as i64)),
        other => Err(OpError::internal(format!("array has no field `{other}`"))),
    }
}

/// `FieldSet`：把 `v` 写进接收者 `recv` 的字段 `name`。
///
/// 堆对象写进引用槽且新值是堆引用时发 `write_barrier_field`（基元写不发）；IC 快慢两路同样处理。
/// 栈对象不发屏障：它不是堆槽，引用字段靠扫 arena 保活。
#[inline(always)]
pub fn field_set(ctx: &VmContext, recv: &Value, name: &str, v: &Value, ic: Option<&FieldIC>) -> OpResult<()> {
    if let Value::Object(rc) = recv {
        let wrote = store(&mut rc.borrow_mut(), name, v, ic)?;
        barrier(ctx, recv, v, wrote);
        return Ok(());
    }
    field_set_rare(ctx, recv, name, v, ic)
}

#[inline(never)]
fn field_set_rare(ctx: &VmContext, recv: &Value, name: &str, v: &Value, ic: Option<&FieldIC>) -> OpResult<()> {
    match recv {
        Value::StackObject { idx, frame_id } => {
            ctx.stack_arena.lock().with_obj_mut(*idx, *frame_id, |o| store(o, name, v, ic))??;
            Ok(())
        }
        Value::Null => Err(OpError::null_field_write(name)),
        other => Err(OpError::internal(format!(
            "FieldSet: expected object, got {other:?} (field `{name}`)"))),
    }
}

/// `LoadFieldAddr` 的接收者检查：只有堆对象能取字段地址。
pub fn check_field_addr(recv: &Value, name: &str) -> OpResult<GcRef<ScriptObject>> {
    match recv {
        Value::Object(rc) => Ok(*rc),
        Value::Null => Err(OpError::null_field_addr(name)),
        other => Err(OpError::internal(format!("LoadFieldAddr: expected object, got {other:?}"))),
    }
}

/// 经 `ref` 读字段（`RefKind::Field`）：字段必须存在。
pub fn load_named(gc: &GcRef<ScriptObject>, name: &str) -> OpResult<Value> {
    let o = gc.borrow();
    let slot = *o.type_desc.field_index.get(name).ok_or_else(|| OpError::internal(format!(
        "ref field `{name}` not found on type `{}`", o.type_desc.name)))?;
    Ok(o.field_value(slot))
}

/// 经 `ref` 写字段：与 `FieldSet` 同一条写入 + 写屏障规则。
pub fn store_named(ctx: &VmContext, gc: &GcRef<ScriptObject>, name: &str, v: &Value) -> OpResult<()> {
    let wrote = {
        let mut o = gc.borrow_mut();
        if !o.type_desc.field_index.contains_key(name) {
            return Err(OpError::internal(format!(
                "ref field `{name}` not found on type `{}`", o.type_desc.name)));
        }
        store(&mut o, name, v, None)?
    };
    barrier(ctx, &Value::Object(*gc), v, wrote);
    Ok(())
}

/// JIT 循环不变量提升：堆对象接收者的**内联基元**字段 → `(bytes 基址, 字节偏移, 宽度, tag)`。
/// 其它一切（非对象 / null / 字段不存在 / 引用 / struct 根 / string）→ `None`，内联码回落到
/// `field_get` / `field_set`，异常在真实访问点抛出。
#[inline]
pub fn inline_prim_slot(recv: &Value, name: &str) -> Option<(*const u8, u32, u32, u8)> {
    let Value::Object(rc) = recv else { return None };
    rc.borrow().inline_prim_field(name)
}

/// JIT 循环不变量提升：堆对象接收者的**引用字**字段（8 B 自描述字，`ref_word`）
/// → `(bytes 基址, 字节偏移)`。其它 → `None`（回落 `field_get`）。
#[inline]
pub fn inline_ref_slot(recv: &Value, name: &str) -> Option<(*const u8, u32)> {
    let Value::Object(rc) = recv else { return None };
    rc.borrow().inline_ref_field(name)
}
