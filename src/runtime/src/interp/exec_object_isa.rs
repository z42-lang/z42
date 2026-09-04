//! `IsInstance` / `AsCast` interp dispatch — the runtime type tests.
//!
//! refactor-split-interp-exec-object (2026-09-04): split out of
//! `exec_object.rs` when it crossed the 500-line hard limit; the parent
//! re-exports so `exec_object::is_instance` / `as_cast` paths are unchanged.

use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::Result;

use super::super::dispatch::isa_td;
use super::super::exec_vcall::is_array_isa;
use super::super::{prim_isa, Frame};

/// 整数族类名（编译器 QualifyTypeName 发 FQ 形；带非限定形兜底）。
pub(crate) fn is_integer_class(cn: &str) -> bool {
    matches!(cn,
        "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte"
        | "Std.Byte" | "Std.UInt16" | "Std.UInt32" | "Std.UInt64"
        | "Int32" | "Int64" | "Int16" | "SByte"
        | "Byte" | "UInt16" | "UInt32" | "UInt64")
}

pub(crate) fn is_instance(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
) -> Result<()> {
    let result = match frame.get(obj)? {
        Value::Object(rc) => isa_td(ctx, &module.type_registry, rc.type_desc(), class_name),
        // 2026-05-07 add-array-base-class: T[] is-a Std.Array is-a Std.Object.
        // VM hardcodes the chain since Value::Array doesn't carry a TypeDesc.
        Value::Array(_) => is_array_isa(class_name),
        Value::Null => false,
        // add-struct-object-boxing → unify Phase 2 R3: 装箱值类型（struct 或基元）is-a 精确类型 /
        // object（值类型经装箱进 object 层级）；接口 / 基链经 type_desc 解析。基元盒的 type_desc.name
        // 即精确 wrapper（Std.Int64…）→ `9L is long`→true、`5 is long`→false 由 exact/subclass 判定。
        Value::BoxedStruct(b) => {
            class_name == "Std.Object" || class_name == "Object"
                || &*b.type_desc().name == class_name
                || isa_td(ctx, &module.type_registry, b.type_desc(), class_name)
        }
        // fix-boxed-primitive-is-as: 未装箱裸基元（未经 object 边界）→ stdlib 类名松匹配兜底。
        other => prim_isa(other, class_name),
    };
    frame.set(dst, Value::Bool(result));
    Ok(())
}

pub(crate) fn as_cast(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
) -> Result<()> {
    let val = frame.get(obj)?.clone();
    // add-struct-object-boxing → unify Phase 2 R3: BoxedStruct 特判（struct 或基元装箱统一）——
    // 命中**精确类型** → 拆箱（基元盒 → 裸标量 `o as long`；struct 盒 → 当前帧 arena StructRef 值副本）；
    // 命中 object/base/接口 → 保持 boxed（多态用）；否则 Null。基元 vs struct 盒由 boxed_prim_i64 分流。
    if let Value::BoxedStruct(b) = &val {
        let is_obj = class_name == "Std.Object" || class_name == "Object";
        let prim_scalar = b.borrow().boxed_prim_i64();
        let out = if &*b.type_desc().name == class_name {
            match prim_scalar {
                Some(n) => Value::I64(n), // 基元盒精确命中 → 拆回裸标量
                None => super::super::exec_struct::unbox_struct(ctx, frame.frame_id, b)?, // struct 盒 → arena StructRef
            }
        } else if is_obj || isa_td(ctx, &module.type_registry, b.type_desc(), class_name) {
            val.clone()
        } else {
            Value::Null
        };
        frame.set(dst, out);
        return Ok(());
    }
    // add-struct-foreach (P3b follow-up): a `StructBytes`-array element handle (`arr[i]` in a
    // value context — e.g. `foreach (P p in arr)`) → copy the element out to a fresh current-frame
    // arena `StructRef` (value-semantics snapshot; the loop var must not alias the array).
    if let Value::StructRefHeap { idx, frame_id } = &val {
        let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
        let out = super::super::exec_struct::copy_array_elem_out(ctx, frame.frame_id, &e)?;
        frame.set(dst, out);
        return Ok(());
    }
    // add-struct-generic-boxing (P3a): 已是未装箱值 struct（StructRef）→ `as P` 恒等（原样返回）。编译器仅在
    // 静态类型即该 struct 处发此 AsCast（泛型容器拆箱统一走 AsCast，元素可能是 BoxedStruct 或已是 StructRef——
    // 普通 P[]；此臂让 StructRef 情形不被误判 Null）。
    if matches!(&val, Value::StructRef { .. }) {
        frame.set(dst, val);
        return Ok(());
    }
    let is_match = match &val {
        Value::Object(rc) => isa_td(ctx, &module.type_registry, rc.type_desc(), class_name),
        Value::Array(_) => is_array_isa(class_name),
        Value::Null => true,
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名松匹配兜底。
        other => prim_isa(other, class_name),
    };
    frame.set(dst, if is_match { val } else { Value::Null });
    Ok(())
}

