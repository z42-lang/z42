//! 静态字段读写：`StaticGet` / `StaticSet` 的唯一实现。
//!
//! 每次访问先过初始化屏障（包初始化器 → 属主类型的静态构造器），失败抛可 catch 的
//! `TypeInitializationException`。读到 `Null` 时才确证字段是否声明过：未声明 → 缺符号异常；
//! 声明了但从未赋值 → 惰性写回该字段类型的零值。

use crate::metadata::tokens::StaticFieldId;
use crate::metadata::{Module, Value};
use crate::vm_context::cctor::make_type_init_exception;
use crate::vm_context::symres::{verify_static_field, StaticNullVerdict};
use crate::vm_context::VmContext;

use super::error::{OpError, OpResult};

/// 初始化屏障。稳态代价：包初始化两次 relaxed load + cctor 一次 relaxed load。
#[inline]
fn barrier(ctx: &VmContext, module: &Module, field: &str, field_id: Option<u32>) -> OpResult<()> {
    // 读/写一个跨包静态字段可能刚把那个包拉进来：包初始化器先于类型初始化器。
    if let Err(msg) = ctx.ensure_module_inits(Some(field)) {
        return Err(OpError::Thrown(make_type_init_exception(ctx, module, &msg)));
    }
    ctx.ensure_static_owner_init(field, field_id)
        .map_err(|msg| OpError::Thrown(make_type_init_exception(ctx, module, &msg)))
}

/// `StaticGet`。`field_id` = 解析好的槽号（`None` → 按名找，惰性分配槽号）。
pub fn static_get(ctx: &VmContext, module: &Module, field: &str, field_id: Option<u32>) -> OpResult<Value> {
    barrier(ctx, module, field, field_id)?;
    let v = match field_id {
        Some(id) => ctx.static_get_by_id(StaticFieldId(id)),
        None => ctx.static_get(field),
    };
    // Null 本身是合法值（未赋值的引用型静态字段），只在读到它时确证。
    if !matches!(v, Value::Null) { return Ok(v); }
    match verify_static_field(ctx, module, field) {
        StaticNullVerdict::Ok => Ok(v),
        StaticNullVerdict::Missing(exc) => Err(OpError::Thrown(exc)),
        // 惰性零初始化：回写槽位，后续读不再走这条确证路径。
        StaticNullVerdict::Default(d) => {
            store(ctx, field, field_id, d);
            Ok(d)
        }
    }
}

/// `StaticSet`。
pub fn static_set(ctx: &VmContext, module: &Module, field: &str, field_id: Option<u32>, v: Value) -> OpResult<()> {
    barrier(ctx, module, field, field_id)?;
    store(ctx, field, field_id, v);
    Ok(())
}

#[inline]
fn store(ctx: &VmContext, field: &str, field_id: Option<u32>, v: Value) {
    match field_id {
        Some(id) => ctx.static_set_by_id(StaticFieldId(id), v),
        None => ctx.static_set(field, v),
    }
}
