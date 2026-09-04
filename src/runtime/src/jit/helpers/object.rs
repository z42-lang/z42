#![allow(dangerous_implicit_autorefs)]
//! Object allocation, field access, type tests, static fields, and the
//! generic `default(T)` runtime helper.

use crate::interp::dispatch::isa_td;
use crate::metadata::{NativeData, Value};

use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

// ── Object allocation ────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_new(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    cls_name_ptr: *const u8, cls_name_len: usize,
    ctor_name_ptr: *const u8, ctor_name_len: usize,
    args_ptr: *const u32, argc: usize,
    // 2026-05-07 expand-jit-type-args: per-instance generic type-args (D-8b-3
    // Phase 2 JIT path). `type_args_ptr` is a `*const String` directly into the
    // IR `Instruction::ObjNew { type_args: Vec<String> }` storage, valid for
    // module lifetime. Non-generic ObjNew passes count = 0.
    type_args_ptr: *const String, type_args_count: usize,
) -> u8 {
    // cache-failed-name-resolution: borrow, don't `to_string()` — 2 allocs per `new`.
    let class_name = std::str::from_utf8(std::slice::from_raw_parts(cls_name_ptr, cls_name_len))
        .unwrap_or("<invalid>");
    let ctor_name = std::str::from_utf8(std::slice::from_raw_parts(ctor_name_ptr, ctor_name_len))
        .unwrap_or("<invalid>");
    let ctx_ref   = &*ctx;
    let module    = &*ctx_ref.module;
    let frame_ref = &mut *frame;

    // E1.P2 Phase 1 exemplar (2026-06-02): route metadata access through
    // the `JitVm` trait instead of `module.type_registry.get(...)` directly.
    // Other helpers stay on concrete-field access for now; Phase 2 spec
    // migrates the remaining ~10 sites.
    use super::super::vm_interface::JitVm;
    // make-vm-loading-lazy: an imported class (e.g. Std.Cli.SubcommandRouter) is
    // NOT in the merged module's type registry until first use — it lives in the
    // lazy loader. Probe it there before the blank-descriptor fallback, mirroring
    // interp's `exec_object::obj_new`. Without this, `new SubcommandRouter()` gets
    // a zero-field TypeDesc → zero slots → every field read returns Null (observed:
    // `this._count` reads Null → `I64(0) vs Null` in SubcommandRouter.Add).
    let type_desc = module.type_lookup(class_name).cloned()
        .or_else(|| vm_ctx_ref(ctx).try_lookup_type(class_name))
        .unwrap_or_else(|| std::sync::Arc::new(crate::metadata::TypeDesc {
            name: class_name.to_string(), base_name: None,
            class_flags: 0,
            visibility: 0,
            fields: Vec::new(), field_index: crate::metadata::NameIndex::new(),
            vtable: Vec::new(), vtable_index: crate::metadata::NameIndex::new(),
            cold: None,
            id: crate::metadata::tokens::TypeId::UNRESOLVED,
        }));
    // unify-object-byte-layout (PR-2): fields default to zero-initialized bytes +
    // `Null` refs (= the old per-field defaults), produced inside `alloc_object` from
    // the composed layout; pass no initial values (mirrors interp `obj_new`).
    let obj_val = vm_ctx_ref(ctx).heap().alloc_object(type_desc, Vec::new(), NativeData::None);

    // 2026-05-07 expand-jit-type-args: populate per-instance type_args BEFORE
    // ctor call so the ctor body's `default(T)` resolves correctly (mirrors
    // interp ObjNew handler order).
    if type_args_count > 0 {
        if let Value::Object(ref rc) = obj_val {
            let slice = std::slice::from_raw_parts(type_args_ptr, type_args_count);
            rc.borrow_mut().type_args = Box::<[String]>::from(slice);
        }
    }

    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let mut ctor_args: Vec<Value> = vec![obj_val.clone()];
    ctor_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));

    // 直查 ctor_name (TypeChecker 已 overload-resolve)；无名字推断。
    // runtime-jit-tiering Phase 1b: tiered ctor. A cold (below-threshold) or
    // interp-only ctor resolves to None and is run on the INTERPRETER — it mutates
    // `this` (== ctor_args[0] == obj_val, a shared GcRef) in place. Without this the
    // old code silently skipped the ctor for None, leaving fields uninitialized
    // (observed: `is_pattern_binding` field 5→0). A type with no ctor resolves to
    // nothing → both paths skip and the already-default-initialised object is used.
    if let Some(entry) = ctx_ref.resolve_fn_by_name_tiered(ctor_name) {
        let mut callee = JitFrame::new(entry.max_reg, &ctor_args);
        let jit_fn: JitFn = std::mem::transmute(entry.ptr);
        let vm_ctx = vm_ctx_ref(ctx);
        vm_ctx.push_frame(crate::exception::VmFrame::new(
            entry.name.clone(), entry.file.clone(),
            &callee.regs as *const _, &callee.env_arena as *const _));
        let r = jit_fn(&mut callee, ctx);
        vm_ctx.pop_frame();
        callee.recycle();
        if r != 0 { return 1; }
    } else {
        let vm_ctx = vm_ctx_ref(ctx);
        let oc = if let Some(callee) = module.func_index.get(ctor_name)
            .and_then(|&idx| module.functions.get(idx))
        {
            Some(crate::interp::exec_function(vm_ctx, module, callee, &ctor_args))
        } else if let Some(lazy_fn) = vm_ctx.try_lookup_function(ctor_name) {
            Some(crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &ctor_args))
        } else {
            None // ctor-less type → skip (object already default-initialised)
        };
        if let Some(outcome) = oc {
            match outcome {
                Ok(crate::interp::ExecOutcome::Returned(_)) => {} // ctor mutated `this` in place
                Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); return 1; }
                Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
            }
        }
    }
    frame_ref.regs[dst as usize] = obj_val;
    0
}

/// add-reflection-generic-type-definition: JIT helper for the `Typeof` opcode.
/// Mirrors the interp `Instruction::Typeof` handler — builds a `Std.Type` from
/// the FQ name + structured generic instantiation args. `type_args_ptr` is a
/// `*const String` into the IR `Instruction::Typeof { type_args }` storage
/// (valid for module lifetime; count = 0 for non-generic typeof).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_typeof(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    type_name_ptr: *const u8, type_name_len: usize,
    type_args_ptr: *const String, type_args_count: usize,
) {
    let type_name = std::str::from_utf8(std::slice::from_raw_parts(type_name_ptr, type_name_len))
        .unwrap_or("<invalid>");
    let type_args = std::slice::from_raw_parts(type_args_ptr, type_args_count);
    let v = crate::corelib::reflection::make_constructed_type(vm_ctx_ref(ctx), type_name, type_args);
    (*frame).regs[dst as usize] = v;
}

// 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): JIT helper for
// `default(T)` runtime resolution. Mirrors interp `Instruction::DefaultOf`
// dispatch — reads `frame.regs[0]` (this) → `ScriptObject.type_args[param_index]`
// → `default_value_for(tag)`. Non-Object reg 0 / OOB index / empty type_args
// → graceful Null. Note: JIT-allocated objects currently have empty type_args
// (jit_obj_new doesn't propagate them from the IR ObjNew yet), so this returns
// Null in JIT-only data-flow; interp path is the source of truth for full
// generic-T zero-value resolution.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_default_of(
    _frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, param_index: u32,
) -> u8 {
    let frame_ref = &mut *_frame;
    let val = match frame_ref.regs.first() {
        Some(Value::Object(rc)) => {
            let b = rc.borrow();
            b.type_args.get(param_index as usize)
                .map(|tag| crate::metadata::types::default_value_for(tag))
                .unwrap_or(Value::Null)
        }
        _ => Value::Null,
    };
    frame_ref.regs[dst as usize] = val;
    0
}

/// spec fix-numeric-cast-lowering (2026-05-13): explicit numeric type
/// conversion. Mirrors interp `exec_value::convert` semantics:
///   - source Value variant determines from-type
///   - `to_tag` (u32 from JIT calling convention; really TypeTag byte) gives target
///   - On conversion failure (e.g. invalid Unicode scalar) sets pending
///     exception via `set_exception` and returns 1
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_convert(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, src: u32, to_tag: u32,
) -> u8 {
    let frame_ref = &mut *frame;
    let src_val = match frame_ref.regs.get(src as usize) {
        Some(v) => v.clone(),
        None => {
            set_exception(vm_ctx_ref(ctx),
                Value::Str(format!("jit_convert: undefined register %{}", src).into()));
            return 1;
        }
    };
    // converge-vm-arith-semantics (H3): convert dispatch moved to the shared
    // single source of truth (was `interp::exec_value::convert_value`).
    let result = match crate::semantics::convert_value(src_val, to_tag as u8) {
        Ok(v) => v,
        Err(e) => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("{:#}", e).into()));
            return 1;
        }
    };
    frame_ref.regs[dst as usize] = result;
    0
}

// ── IsInstance / AsCast ──────────────────────────────────────────────────────
//
// perf-vm-isa-cache (2026-09-03): the JIT-private `is_subclass_or_eq` walk (+ its
// `iface_reaches_mod` mirror) is gone — both helpers now call the interpreter's single
// `dispatch::isa_td` (identity-keyed `IsaCache` → shared string memo → chain walk), so
// there is exactly one type-test implementation for interp, JIT and typed `catch`.
// 2026-05-07 add-array-base-class: T[] is-a Std.Array is-a Std.Object.
// Mirror the interp `is_array_isa` hardcoded chain.
pub(super) fn is_array_isa(class_name: &str) -> bool {
    matches!(class_name, "Array" | "Object" | "Std.Array" | "Std.Object")
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_is_instance(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32, cls_ptr: *const u8, cls_len: usize,
) {
    let class_name = std::str::from_utf8(std::slice::from_raw_parts(cls_ptr, cls_len))
        .unwrap_or("<invalid>");
    let module = &*(*ctx).module;
    let result = match &(*frame).regs[obj as usize] {
        Value::Object(rc) => isa_td(vm_ctx_ref(ctx), &module.type_registry, rc.type_desc(), class_name),
        Value::Array(_)   => is_array_isa(class_name),
        // add-struct-object-boxing → unify Phase 2 R3: 装箱值类型（struct 或基元）is-a 精确类型 /
        // object（镜像 interp is_instance；基元盒 type_desc.name 即精确 wrapper）。
        Value::BoxedStruct(b) => class_name == "Std.Object" || class_name == "Object"
            || &*b.type_desc().name == class_name
            || isa_td(vm_ctx_ref(ctx), &module.type_registry, b.type_desc(), class_name),
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名匹配（Null → None → false）。
        other => crate::interp::prim_isa(other, class_name),
    };
    (*frame).regs[dst as usize] = Value::Bool(result);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_as_cast(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32, cls_ptr: *const u8, cls_len: usize,
) {
    let class_name = std::str::from_utf8(std::slice::from_raw_parts(cls_ptr, cls_len))
        .unwrap_or("<invalid>");
    let module = &*(*ctx).module;
    let val    = (*frame).regs[obj as usize].clone();
    // add-struct-object-boxing → unify Phase 2 R3: BoxedStruct 特判（struct 或基元装箱统一，镜像
    // interp as_cast）——精确类型命中 → 拆箱（基元盒 → 裸标量；struct 盒 → 当前帧 arena StructRef 值
    // 副本，frame_id 由 struct_ops::frame_id_of 惰性分配）；object/base/接口 → 保持 boxed（多态）；否则
    // Null。基元 vs struct 盒由 boxed_prim_i64 分流。
    if let Value::BoxedStruct(b) = &val {
        let is_obj = class_name == "Std.Object" || class_name == "Object";
        let prim_scalar = b.borrow().boxed_prim_i64();
        let out = if &*b.type_desc().name == class_name {
            match prim_scalar {
                Some(n) => Value::I64(n), // 基元盒精确命中 → 裸标量
                None => {
                    let fid = super::struct_ops::frame_id_of(frame, ctx);
                    crate::interp::exec_struct::unbox_struct(vm_ctx_ref(ctx), fid, b)
                        .unwrap_or(Value::Null)
                }
            }
        } else if is_obj || isa_td(vm_ctx_ref(ctx), &module.type_registry, b.type_desc(), class_name) {
            val.clone()
        } else {
            Value::Null
        };
        (*frame).regs[dst as usize] = out;
        return;
    }
    // add-struct-generic-boxing (P3a): 未装箱值 struct（StructRef）→ `as P` 恒等（镜像 interp as_cast）。
    if matches!(&val, Value::StructRef { .. }) {
        (*frame).regs[dst as usize] = val;
        return;
    }
    // add-struct-jit-value-path (P5): struct[] 元素句柄在值上下文（foreach 循环变量等）→ 拷出到
    // 当前帧 arena StructRef（值副本快照，镜像 interp copy_array_elem_out）。
    if let Value::StructRefHeap { idx, frame_id } = &val {
        // make-value-copy: resolve the StructRefHeap handle → StructArrayElem via the arena.
        let e = match vm_ctx_ref(ctx).transient_arena.lock().struct_elem(*idx, *frame_id) {
            Ok(e) => e,
            Err(_) => { (*frame).regs[dst as usize] = Value::Null; return; }
        };
        let fid = super::struct_ops::frame_id_of(frame, ctx);
        (*frame).regs[dst as usize] =
            crate::interp::exec_struct::copy_array_elem_out(vm_ctx_ref(ctx), fid, &e)
                .unwrap_or(Value::Null);
        return;
    }
    let is_match = match &val {
        Value::Object(rc) => isa_td(vm_ctx_ref(ctx), &module.type_registry, rc.type_desc(), class_name),
        Value::Array(_)   => is_array_isa(class_name),
        Value::Null => true,
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名匹配。
        other       => crate::interp::prim_isa(other, class_name),
    };
    (*frame).regs[dst as usize] = if is_match { val } else { Value::Null };
}

// ── Static fields ────────────────────────────────────────────────────────────

/// `jit_static_get` after formalize-jit-method-token Phase 2 (2026-05-08):
/// receives pre-resolved `StaticFieldId` directly. make-vm-loading-lazy: a
/// lazily-loaded function is JIT-compiled without its resolved token table, so
/// `field_id` may be `UNRESOLVED` — then resolve the field by NAME
/// (`field_ptr`/`field_len`) at runtime, mirroring interp's `exec_object`
/// `field_id: None` fallback (`ctx.static_get(name)` allocates the id lazily).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_static_get(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, field_id: u32,
    field_ptr: *const u8, field_len: usize,
) {
    let vm = vm_ctx_ref(_ctx);
    let v = if field_id != crate::metadata::tokens::UNRESOLVED {
        vm.static_get_by_id(crate::metadata::tokens::StaticFieldId(field_id))
    } else {
        let field = std::str::from_utf8(std::slice::from_raw_parts(field_ptr, field_len))
            .unwrap_or("<invalid>");
        vm.static_get(field)
    };
    (*frame).regs[dst as usize] = v;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_static_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    field_id: u32, val: u32,
    field_ptr: *const u8, field_len: usize,
) {
    let vm = vm_ctx_ref(ctx);
    let v = (*frame).regs[val as usize].clone();
    if field_id != crate::metadata::tokens::UNRESOLVED {
        vm.static_set_by_id(crate::metadata::tokens::StaticFieldId(field_id), v);
    } else {
        let field = std::str::from_utf8(std::slice::from_raw_parts(field_ptr, field_len))
            .unwrap_or("<invalid>");
        vm.static_set(field, v);
    }
}
