//! `IsInstance` / `AsCast` interp dispatch — the runtime type tests.
//!
//! refactor-split-interp-exec-object : split out of
//! `exec_object.rs` when it crossed the 500-line hard limit; the parent
//! re-exports so `exec_object::is_instance` / `as_cast` paths are unchanged.

use crate::metadata::tokens::TypeKeyCell;
use crate::vm::runtime::Module;
use crate::object::Value;
use crate::vm_context::VmContext;
use anyhow::Result;

use super::super::dispatch::{array_isa, bound_param_extra_isa, isa_object, isa_td, struct_ref_isa};
use super::super::{prim_isa, Frame};

/// 整数族类名（编译器 QualifyTypeName 发 FQ 形；带非限定形兜底）。
pub(crate) fn is_integer_class(cn: &str) -> bool {
    matches!(cn,
        "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte"
        | "Std.Byte" | "Std.UInt16" | "Std.UInt32" | "Std.UInt64"
        | "Int32" | "Int64" | "Int16" | "SByte"
        | "Byte" | "UInt16" | "UInt32" | "UInt64")
}

/// A target naming a type parameter of a shared generic body (`o is T`, `o is List<T>`) carries
/// markers; it is substituted from this activation first and tested with a key of its own (the
/// site's cached key belongs to the marker spelling, which names a different type per call).
fn dynamic_target(ctx: &VmContext, frame: &Frame, class_name: &str) -> Option<String> {
    crate::metadata::index::type_param_markers::has_markers(class_name)
        .then(|| frame.type_param_env().test_target(ctx, class_name))
}

#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(crate) fn is_instance(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
    key: &TypeKeyCell,
) -> Result<()> {
    match dynamic_target(ctx, frame, class_name) {
        Some(t) => is_instance_of(ctx, module, frame, dst, obj, &t, &TypeKeyCell::new(), true),
        None => is_instance_of(ctx, module, frame, dst, obj, class_name, key, false),
    }
}

/// `bound_param`: `class_name` was substituted for a type-parameter marker — the extra matches of
/// `dispatch::bound_param_extra_isa` apply.
#[allow(clippy::too_many_arguments)]
fn is_instance_of(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
    key: &TypeKeyCell, bound_param: bool,
) -> Result<()> {
    let v = frame.get(obj)?;
    if bound_param && bound_param_extra_isa(ctx, &module.type_registry, v, class_name) {
        frame.set(dst, Value::Bool(true));
        return Ok(());
    }
    let result = match v {
        Value::Object(rc) => isa_object(ctx, &module.type_registry, rc.type_desc(), rc.type_args(), class_name, key),
        // T[] is-a Std.Array is-a Std.Object, and is-a X[] iff its element type is X (invariant).
        Value::Array(rc) => array_isa(&rc.borrow().element_type(), class_name),
        Value::StructRef { idx, frame_id } => struct_ref_isa(ctx, &module.type_registry, *idx, *frame_id, class_name),
        Value::Null => false,
        // add-struct-object-boxing → unify Phase 2 R3: 装箱值类型（struct 或基元）is-a 精确类型 /
        // object（值类型经装箱进 object 层级）；接口 / 基链经 type_desc 解析。基元盒的 type_desc.name
        // 即精确 wrapper（Std.Int64…）→ `9L is long`→true、`5 is long`→false 由 exact/subclass 判定。
        Value::BoxedStruct(b) => {
            class_name == "Std.Object" || class_name == "Object"
                || &*b.type_desc().name == class_name
                || isa_td(ctx, &module.type_registry, b.type_desc(), class_name, key)
        }
        // fix-boxed-primitive-is-as: 未装箱裸基元（未经 object 边界）→ stdlib 类名松匹配兜底。
        other => prim_isa(other, class_name),
    };
    frame.set(dst, Value::Bool(result));
    Ok(())
}

#[inline(never)] // 热循环布局：留在 exec_function_body 之外（见 exec_instr.rs 模块注释）
pub(crate) fn as_cast(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
    key: &TypeKeyCell,
) -> Result<()> {
    match dynamic_target(ctx, frame, class_name) {
        Some(t) => as_cast_to(ctx, module, frame, dst, obj, &t, &TypeKeyCell::new(), true),
        None => as_cast_to(ctx, module, frame, dst, obj, class_name, key, false),
    }
}

#[allow(clippy::too_many_arguments)]
fn as_cast_to(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
    key: &TypeKeyCell, bound_param: bool,
) -> Result<()> {
    let val = frame.get(obj)?.clone();
    if bound_param && bound_param_extra_isa(ctx, &module.type_registry, &val, class_name) {
        frame.set(dst, val);
        return Ok(());
    }
    // add-struct-object-boxing → unify Phase 2 R3: BoxedStruct 特判（struct 或基元装箱统一）——
    // 命中**精确类型** → 拆箱（基元盒 → 裸标量 `o as long`；struct 盒 → 当前帧 arena StructRef 值副本）；
    // 命中 object/base/接口 → 保持 boxed（多态用）；否则 Null。基元 vs struct 盒由 boxed_prim_i64 分流。
    if let Value::BoxedStruct(b) = &val {
        let is_obj = class_name == "Std.Object" || class_name == "Object";
        let prim_scalar = b.borrow().boxed_prim_i64();
        let out = if &*b.type_desc().name == class_name {
            match prim_scalar {
                Some(n) => Value::I64(n), // 基元盒精确命中 → 拆回裸标量
                None => super::super::exec_struct::unbox_struct(ctx, frame.frame_id(ctx), b)?, // struct 盒 → arena StructRef
            }
        } else if is_obj || isa_td(ctx, &module.type_registry, b.type_desc(), class_name, key) {
            val.clone()
        } else {
            Value::Null
        };
        frame.set(dst, out);
        return Ok(());
    }
    // add-struct-foreach (P3b follow-up): a value-struct-array element handle (`arr[i]` in a
    // value context — e.g. `foreach (P p in arr)`) → copy the element out to a fresh current-frame
    // arena `StructRef` (value-semantics snapshot; the loop var must not alias the array).
    if let Value::StructRefHeap { idx, frame_id } = &val {
        let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
        let out = super::super::exec_struct::copy_array_elem_out(ctx, frame.frame_id(ctx), &e)?;
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
        Value::Object(rc) => isa_object(ctx, &module.type_registry, rc.type_desc(), rc.type_args(), class_name, key),
        Value::Array(rc) => array_isa(&rc.borrow().element_type(), class_name),
        Value::Null => true,
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名松匹配兜底。
        other => prim_isa(other, class_name),
    };
    frame.set(dst, if is_match { val } else { Value::Null });
    Ok(())
}

