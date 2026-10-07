#![allow(dangerous_implicit_autorefs)]
//! Object allocation, field access, type tests, static fields, and the
//! generic `default(T)` runtime helper.

use crate::interp::dispatch::isa_td;
use crate::metadata::resolver::ObjNewSite;
use crate::metadata::{NativeData, Value};

use super::super::frame::{JitFrame, JitModuleCtx};
use super::super::invoke::{call_entry, NativeOutcome};
use super::{set_exception, vm_ctx_ref};

// ── Object allocation ────────────────────────────────────────────────────────

/// `ObjNew`. Class and ctor resolution is shared with the interpreter
/// (`interp::obj_new_resolve`); after the first allocation at a site both come from the
/// site cache — no name hashing, no locks, a cross-package class / ctor included. The
/// ctor runs natively once compiled (counted toward its tier-up by `FnId`), else on the
/// interpreter.
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
    // P1-2 PR 5: this site's `ResolvedTokens::obj_new` cache (null when the function was
    // compiled without a resolved token table).
    site_ptr: *const ObjNewSite,
    // encode-ctorless-objnew: compile-time positive ctor marker; see `missing_ctor_exception`.
    ctor_known: u8,
) -> u8 {
    use crate::interp::obj_new_resolve::{resolve_class, resolve_ctor, site_for};
    // Baked by `TxCtx::str_val` from the IR: valid UTF-8, no re-validation per `new`.
    let class_name = super::baked_str(cls_name_ptr, cls_name_len);
    let ctor_name = super::baked_str(ctor_name_ptr, ctor_name_len);
    let ctx_ref   = &*ctx;
    let module    = &*ctx_ref.module;
    let frame_ref = &mut *frame;
    let vm        = vm_ctx_ref(ctx);
    let site = site_for(vm, module, if site_ptr.is_null() { None } else { Some(&*site_ptr) });

    let type_desc = match resolve_class(vm, module, class_name, site) {
        Ok(td) => td,
        Err(exc) => { set_exception(vm, exc); return 1; }
    };
    // add-static-constructors：创建实例是 C# 的类型初始化触发点之一。TypeDesc 已在手 →
    // 一次 `Option` 判断即可。add-module-init-hook：包级初始化先于类型初始化。与 interp 对称。
    if let Err(msg) = vm.ensure_module_inits(Some(type_desc.name.as_str())) {
        set_exception(vm, crate::vm_context::cctor::make_type_init_exception(vm, module, &msg));
        return 1;
    }
    if let Err(msg) = vm.ensure_type_init(&type_desc) {
        set_exception(vm, crate::vm_context::cctor::make_type_init_exception(vm, module, &msg));
        return 1;
    }
    // Bound before the object exists (binding may load a package; see interp `obj_new`).
    let mut lazy_holder = None;
    let ctor = match resolve_ctor(
        vm, module, class_name, ctor_name, argc + 1 /* +this */, ctor_known != 0, site,
        &mut lazy_holder,
    ) {
        Ok(c) => c,
        Err(exc) => { set_exception(vm, exc); return 1; }
    };

    // unify-object-byte-layout (PR-2): fields default to zero-initialized bytes +
    // `Null` refs (= the old per-field defaults), produced inside `alloc_object` from
    // the composed layout; pass no initial values (mirrors interp `obj_new`).
    let obj_val = vm.heap().alloc_object(type_desc, Vec::new(), NativeData::None);

    // 2026-05-07 expand-jit-type-args: populate per-instance type_args BEFORE
    // ctor call so the ctor body's `default(T)` resolves correctly (mirrors
    // interp ObjNew handler order).
    // complete-generic-class-identity P1: an instantiation's class name already carries its
    // arguments, so the compiler ships no separate copy on the instruction. Fall back to the
    // ones the registry parsed off the name — must mirror interp `obj_new` exactly (a
    // one-sided pair here is what made `GBox<int>().V == 0` disagree between the two engines).
    let name_args: Box<[String]> = if type_args_count > 0 {
        Box::new([])
    } else if let Value::Object(ref rc) = obj_val {
        Box::<[String]>::from(rc.borrow().type_desc.type_args())
    } else {
        Box::new([])
    };
    if type_args_count > 0 || !name_args.is_empty() {
        if let Value::Object(ref rc) = obj_val {
            let slice: &[String] = if type_args_count > 0 {
                std::slice::from_raw_parts(type_args_ptr, type_args_count)
            } else {
                &name_args
            };
            let mut o = rc.borrow_mut();
            o.set_type_args(Box::<[String]>::from(slice));
            // fix-generic-typeparam-field-zero: a `T`-typed field is laid out as a
            // *reference* slot (layout comes from the declaration), so layout zero-init
            // leaves it `Null` instead of the instantiation's zero. Must mirror the interp
            // `obj_new` handler exactly — this pair being one-sided is what made
            // `GBox<int>().V == 0` answer `false` under interp and `true` under JIT.
            let overrides = crate::metadata::types::generic_field_zero_overrides(
                &o.type_desc, slice,
            );
            for (slot, zero) in overrides {
                o.set_field_value(slot, &zero);
            }
        }
    }

    // runtime-jit-tiering Phase 1b: tiered ctor. Compiled (or compiling at the threshold) →
    // native, `this` in reg 0 and args straight from the caller's registers; cold /
    // untranslatable / no `FnId` → the interpreter, which mutates `this` (a shared GcRef)
    // in place. `None` = a class without a ctor: the object stays default-initialised.
    if let Some((func, id)) = ctor {
        let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
        match id.and_then(|id| ctx_ref.resolve_fn_by_id_tiered(id)) {
            Some(entry) => {
                let callee = JitFrame::new_method_args_from(
                    vm, entry.max_reg, obj_val.clone(), &frame_ref.regs, arg_regs);
                // The ctor mutates `this` in place; its (void) return value is discarded.
                if let NativeOutcome::Threw = call_entry(vm, ctx, entry, callee) { return 1; }
            }
            None => {
                let mut ctor_args: Vec<Value> = Vec::with_capacity(argc + 1);
                ctor_args.push(obj_val.clone());
                ctor_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
                match crate::interp::exec_function(vm, module, func, &ctor_args) {
                    Ok(crate::interp::ExecOutcome::Returned(_)) => {} // ctor mutated `this` in place
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm, val); return 1; }
                    Err(e) => { set_exception(vm, Value::Str(e.to_string().into())); return 1; }
                }
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
            b.type_args().get(param_index as usize)
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
    // make-hard-cast-fail-properly：与解释器同一判据（semantics::hard_cast_failure），
    // 并构造**真异常对象**而非 `Value::Str`——后者在 catch 机配上是 `<non-exception-value>`，
    // `catch (Exception e)` 匹配不上，于是 interp 能 catch、JIT 不能，是一处 interp/JIT 分叉。
    if let Some((exc_fq, msg)) = crate::semantics::hard_cast_failure(&src_val, to_tag as u8) {
        let vm = vm_ctx_ref(ctx);
        let exc = match vm.module() {
            Some(m) => crate::exception::make_stdlib_exception(vm, m, exc_fq, msg.clone())
                .unwrap_or(Value::Str(msg.clone().into())),
            None => Value::Str(msg.clone().into()),
        };
        set_exception(vm, exc);
        return 1;
    }
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
// `dispatch::isa_td` (id-keyed `IsaCache` → shared memo → chain walk), so
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
    key: *const crate::metadata::tokens::TypeKeyCell,
) {
    let class_name = super::baked_str(cls_ptr, cls_len);
    // The instruction's own key cell, baked at translate time (lives as long as the code).
    let key = &*key;
    let module = &*(*ctx).module;
    let result = match &(*frame).regs[obj as usize] {
        Value::Object(rc) => isa_td(vm_ctx_ref(ctx), &module.type_registry, rc.type_desc(), class_name, key),
        Value::Array(_)   => is_array_isa(class_name),
        // add-struct-object-boxing → unify Phase 2 R3: 装箱值类型（struct 或基元）is-a 精确类型 /
        // object（镜像 interp is_instance；基元盒 type_desc.name 即精确 wrapper）。
        Value::BoxedStruct(b) => class_name == "Std.Object" || class_name == "Object"
            || &*b.type_desc().name == class_name
            || isa_td(vm_ctx_ref(ctx), &module.type_registry, b.type_desc(), class_name, key),
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名匹配（Null → None → false）。
        other => crate::interp::prim_isa(other, class_name),
    };
    (*frame).regs[dst as usize] = Value::Bool(result);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_as_cast(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32, cls_ptr: *const u8, cls_len: usize,
    key: *const crate::metadata::tokens::TypeKeyCell,
) {
    let class_name = super::baked_str(cls_ptr, cls_len);
    // The instruction's own key cell, baked at translate time (lives as long as the code).
    let key = &*key;
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
        } else if is_obj || isa_td(vm_ctx_ref(ctx), &module.type_registry, b.type_desc(), class_name, key) {
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
        Value::Object(rc) => isa_td(vm_ctx_ref(ctx), &module.type_registry, rc.type_desc(), class_name, key),
        Value::Array(_)   => is_array_isa(class_name),
        Value::Null => true,
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名匹配。
        other       => crate::interp::prim_isa(other, class_name),
    };
    (*frame).regs[dst as usize] = if is_match { val } else { Value::Null };
}

// ── Static fields ────────────────────────────────────────────────────────────

/// `StaticGet` helper — adapter over [`crate::objops::statics::static_get`] (init
/// barrier, lazy zero-init, missing-symbol check; shared with the interpreter).
/// `field_id` = pre-resolved `StaticFieldId`, or `UNRESOLVED` for a lazily-loaded
/// function compiled without its token table — then the field resolves by NAME
/// (`field_ptr`/`field_len`, always passed).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_static_get(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, field_id: u32,
    field_ptr: *const u8, field_len: usize,
) -> i32 {
    let field = super::baked_str(field_ptr, field_len);
    let id = (field_id != crate::metadata::tokens::UNRESOLVED).then_some(field_id);
    match crate::objops::statics::static_get(vm_ctx_ref(ctx), &*(*ctx).module, field, id) {
        Ok(v) => { (*frame).regs[dst as usize] = v; 0 }
        Err(e) => super::raise(ctx, e) as i32,
    }
}

/// `StaticSet` helper — adapter over [`crate::objops::statics::static_set`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_static_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    field_id: u32, val: u32,
    field_ptr: *const u8, field_len: usize,
) -> i32 {
    let field = super::baked_str(field_ptr, field_len);
    let id = (field_id != crate::metadata::tokens::UNRESOLVED).then_some(field_id);
    let v = (*frame).regs[val as usize];
    match crate::objops::statics::static_set(vm_ctx_ref(ctx), &*(*ctx).module, field, id, v) {
        Ok(()) => 0,
        Err(e) => super::raise(ctx, e) as i32,
    }
}
