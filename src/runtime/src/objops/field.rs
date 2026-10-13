//! 实例字段读写：`FieldGet` / `FieldSet` 的唯一实现，外加 ref 解引用与 JIT 提升快路
//! 用到的按名读写 / 槽位解析原语。
//!
//! 接收者分派（都在这里，两引擎不另写）：
//! - 堆对象 `Object`：FieldIC 命中直取槽位，未命中查 `field_index` 并装进 IC；引用写入走写屏障。
//! - 栈对象 `StackObject`：经 per-context 栈 arena（同一个 FieldIC，不走写屏障——arena 是根）。
//! - `Str` / `Array` / `StackArray` 的 `Length`（及 `ByteLength` / `Count`）、`PinnedView` 的 `ptr` / `len`。
//! - `BoxedStruct`：擦除返回位流出的值 struct 盒，按名读叶子（复用反射那一份）。
//! - `Null` → `NullReferenceException`。

use crate::metadata::resolver::{assert_field_ic_loc, field_ic_install, field_ic_lookup, FieldIC};
use crate::metadata::types::{FieldAccess, FieldLoc, FieldWrite, ScriptObject};
use crate::metadata::Value;
use crate::vm_context::VmContext;

use super::error::{OpError, OpResult};

/// 解析字段单元：IC 命中直接得到 (偏移, 种类)，不查名字、不走布局；未命中查 `field_index` + 布局，
/// 并把结果装进 IC。字段不存在 → `None`；存在但布局里没有它的单元 → `Some(None)`（读 `Null`、写无效果）。
#[inline(always)]
fn resolve(o: &ScriptObject, name: &str, ic: Option<&FieldIC>) -> Option<Option<FieldAccess>> {
    let td = &*o.type_desc;
    if let Some(ic) = ic {
        if let Some(loc) = field_ic_lookup(ic, td.id.0) {
            if cfg!(debug_assertions) {
                let expected = td.field_index.get(name).and_then(|&slot| {
                    FieldLoc::of(&o.field_access_of(slot)?, slot)
                });
                assert_field_ic_loc(td, name, loc, expected);
            }
            return Some(o.access_at(loc));
        }
    }
    resolve_miss(o, name, ic)
}

/// IC 未命中（或站点没有 IC）：按名字解析，装 IC。单独成函数，命中路径保持紧凑。
#[inline(never)]
fn resolve_miss(o: &ScriptObject, name: &str, ic: Option<&FieldIC>) -> Option<Option<FieldAccess>> {
    let td = &*o.type_desc;
    let slot = *td.field_index.get(name)?;
    let fa = o.field_access_of(slot);
    if let (Some(ic), Some(fa)) = (ic, fa.as_ref()) {
        if let Some(loc) = FieldLoc::of(fa, slot) {
            field_ic_install(ic, td.id.0, loc);
        }
    }
    Some(fa)
}

/// 读；字段不存在读出 `Null`（与历史行为一致）。
#[inline(always)]
fn load(o: &ScriptObject, name: &str, ic: Option<&FieldIC>) -> Value {
    match resolve(o, name, ic) {
        Some(Some(fa)) => o.load_access(fa),
        _ => Value::Null,
    }
}

/// 写入（不含写屏障）。返回 `Some((单元字节偏移, 写了什么))`；调用方据此发屏障。
#[inline(always)]
fn store(o: &ScriptObject, name: &str, v: &Value, ic: Option<&FieldIC>) -> OpResult<Option<(usize, FieldWrite)>> {
    let Some(Some(fa)) = resolve(o, name, ic) else { return Ok(None) };
    match o.store_access(fa, v) {
        Ok(w) => Ok(Some((fa.offset as usize, w))),
        Err(_) => Err(OpError::field_store_rejected(name, v)),
    }
}

/// 引用单元写入后的屏障：只对落进单元的堆引用发（基元经擦除写进 `object` 字段时是装它的盒子）。
/// `cell` 是被写单元的字节偏移（只给测试观察者看；分代屏障只看属主与新值）。
#[inline(always)]
fn barrier(ctx: &VmContext, owner: &Value, v: &Value, wrote: Option<(usize, FieldWrite)>) {
    if let Some((cell, w)) = wrote {
        w.with_barrier_value(v, |stored| ctx.heap().write_barrier_field(owner, cell, stored));
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
        let wrote = store(rc.borrow(), name, v, ic)?;
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
pub fn check_field_addr(recv: &Value, name: &str) -> OpResult<crate::gc::ObjRef> {
    match recv {
        Value::Object(rc) => Ok(*rc),
        Value::Null => Err(OpError::null_field_addr(name)),
        other => Err(OpError::internal(format!("LoadFieldAddr: expected object, got {other:?}"))),
    }
}

/// 经 `ref` 读字段（`RefKind::Field`）：字段必须存在。
pub fn load_named(gc: &crate::gc::ObjRef, name: &str) -> OpResult<Value> {
    let o = gc.borrow();
    let slot = *o.type_desc.field_index.get(name).ok_or_else(|| OpError::internal(format!(
        "ref field `{name}` not found on type `{}`", o.type_desc.name)))?;
    Ok(o.field_value(slot))
}

/// 经 `ref` 写字段：与 `FieldSet` 同一条写入 + 写屏障规则。
pub fn store_named(ctx: &VmContext, gc: &crate::gc::ObjRef, name: &str, v: &Value) -> OpResult<()> {
    let wrote = {
        let o = gc.borrow();
        if !o.type_desc.field_index.contains_key(name) {
            return Err(OpError::internal(format!(
                "ref field `{name}` not found on type `{}`", o.type_desc.name)));
        }
        store(o, name, v, None)?
    };
    barrier(ctx, &Value::Object(*gc), v, wrote);
    Ok(())
}

/// 按**槽位**写堆对象的字段，与 `FieldSet` 同一条写入 + 写屏障规则。
///
/// 给不经 `FieldSet` 指令、由 VM 自己往**已存在**对象里写字段的地方用：throw 点补
/// `Exception.StackTrace`、反射 `FieldInfo.SetValue`、反射 `Type` 对象补 `__typeArgs` /
/// `__fullName`。这些站点以前直接调 `try_set_field_value` / `set_field_value`——SATB 删除屏障在
/// 写原语里、照样记了，但**卡表 / 并发 shade 那一半在调用点**，它们一个都没发：老对象收到
/// 年轻引用却没染脏卡，下一次 minor 就把还被引用着的年轻对象扫掉（use-after-free）。
///
/// 基元槽类型不符照 `try_set_field_value` 报错（错误留给调用方按自己的语义包装）。
pub fn store_slot(ctx: &VmContext, gc: &crate::gc::ObjRef, slot: usize, v: &Value) -> anyhow::Result<()> {
    let o = gc.borrow();
    let Some(fa) = o.field_access_of(slot) else { return Ok(()) };
    let w = o.store_access(fa, v)?;
    barrier(ctx, &Value::Object(*gc), v, Some((fa.offset as usize, w)));
    Ok(())
}
