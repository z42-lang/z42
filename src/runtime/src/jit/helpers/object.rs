#![allow(dangerous_implicit_autorefs)]
//! Object allocation, field access, type tests, static fields, and the
//! generic `default(T)` runtime helper.

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
    let class_name = std::str::from_utf8(std::slice::from_raw_parts(cls_name_ptr, cls_name_len))
        .unwrap_or("<invalid>").to_string();
    let ctor_name = std::str::from_utf8(std::slice::from_raw_parts(ctor_name_ptr, ctor_name_len))
        .unwrap_or("<invalid>").to_string();
    let ctx_ref   = &*ctx;
    let module    = &*ctx_ref.module;
    let frame_ref = &mut *frame;

    // E1.P2 Phase 1 exemplar (2026-06-02): route metadata access through
    // the `JitVm` trait instead of `module.type_registry.get(...)` directly.
    // Other helpers stay on concrete-field access for now; Phase 2 spec
    // migrates the remaining ~10 sites.
    use super::super::vm_interface::JitVm;
    let type_desc = module.type_lookup(&class_name).cloned()
        .unwrap_or_else(|| std::sync::Arc::new(crate::metadata::TypeDesc {
            name: class_name.clone(), base_name: None,
            class_flags: 0,
            fields: Vec::new(), field_index: crate::metadata::NameIndex::new(),
            vtable: Vec::new(), vtable_index: crate::metadata::NameIndex::new(),
            cold: None,
            id: crate::metadata::tokens::TypeId::UNRESOLVED,
        }));
    // 2026-05-02 fix-class-field-default-init: 按字段类型选默认值（与 interp
    // exec_object.rs::obj_new 镜像，共用 metadata::default_value_for）。
    let slots: Vec<Value> = type_desc.fields.iter()
        .map(|f| crate::metadata::default_value_for(&f.type_tag))
        .collect();
    let obj_val = vm_ctx_ref(ctx).heap().alloc_object(type_desc, slots, NativeData::None);

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
    // lazy-per-function-jit: resolve_fn_by_name compiles the ctor on first use.
    if let Some(entry) = ctx_ref.resolve_fn_by_name(ctor_name.as_str()) {
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
    let result = match crate::interp::exec_value::convert_value(src_val, to_tag as u8) {
        Ok(v) => v,
        Err(e) => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("{:#}", e).into()));
            return 1;
        }
    };
    frame_ref.regs[dst as usize] = result;
    0
}

// ── Field access ─────────────────────────────────────────────────────────────

/// `jit_field_get` after formalize-jit-method-token Phase 2.E (2026-05-08):
/// per-site `FieldIC` is threaded in (stable raw pointer baked at codegen).
/// Mirrors interp `field_get` — IC hit fetches `slots[cached_slot]` directly;
/// miss walks `field_index` and writes (TypeId, slot) to IC. Non-Object
/// receivers (Str / Array) bypass the IC since their field set is hardcoded.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_get(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32,
    field_name_ptr: *const u8, field_name_len: usize,
    ic_ptr: *const crate::metadata::resolver::FieldIC,
) -> u8 {
    let field_name = std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len))
        .unwrap_or("<invalid>");
    let obj_val = &(*frame).regs[obj as usize];
    let val = match obj_val {
        Value::Object(rc) => {
            let b = rc.borrow();
            // PIC fast path (review.md C4 P2 — 4-slot linear scan).
            if !ic_ptr.is_null() {
                let recv_type = b.type_desc.id.0;
                if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                    let v = b.slots.get(slot as usize).cloned().unwrap_or(Value::Null);
                    (*frame).regs[dst as usize] = v;
                    return 0;
                }
                if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                    crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                    b.slots.get(slot).cloned().unwrap_or(Value::Null)
                } else { Value::Null }
            } else if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                b.slots.get(slot).cloned().unwrap_or(Value::Null)
            } else { Value::Null }
        }
        Value::Str(s) if field_name == "Length"     => Value::I64(crate::corelib::str_meta::char_len(s) as i64),
        Value::Str(s) if field_name == "ByteLength" => Value::I64(s.len() as i64),
        Value::Array(rc) if field_name == "Length" || field_name == "Count" => Value::I64(rc.borrow().len() as i64),
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("FieldGet: expected object, got {:?}", other).into()));
            return 1;
        }
    };
    (*frame).regs[dst as usize] = val;
    0
}

/// JIT FieldSet helper.
///
/// **add-write-barriers (2026-05-21)**: dispatches `write_barrier_field`
/// after a successful slot write *iff* `v.is_heap_ref()`. Mirrors
/// `interp::exec_object::field_set` — primitive writes skip dispatch
/// (Decision 1); both IC fast and slow paths fire (Decision 5).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_field_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    obj: u32,
    field_name_ptr: *const u8, field_name_len: usize, val: u32,
    ic_ptr: *const crate::metadata::resolver::FieldIC,
) -> u8 {
    let field_name = std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len))
        .unwrap_or("<invalid>");
    let v = (*frame).regs[val as usize].clone();
    let owner = (*frame).regs[obj as usize].clone();
    match &owner {
        Value::Object(rc) => {
            let mut b = rc.borrow_mut();
            // PIC fast path
            if !ic_ptr.is_null() {
                let recv_type = b.type_desc.id.0;
                if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                    let slot = slot as usize;
                    if slot < b.slots.len() {
                        b.slots[slot] = v.clone();
                        drop(b);
                        if v.is_heap_ref() {
                            vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                        }
                    }
                    return 0;
                }
                let slot_opt = b.type_desc.field_index.get(field_name).copied();
                if let Some(slot) = slot_opt {
                    crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                    if slot < b.slots.len() {
                        b.slots[slot] = v.clone();
                        drop(b);
                        if v.is_heap_ref() {
                            vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                        }
                    }
                }
            } else if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                if slot < b.slots.len() {
                    b.slots[slot] = v.clone();
                    drop(b);
                    if v.is_heap_ref() {
                        vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                    }
                }
            }
            0
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("FieldSet: expected object, got {:?}", other).into()));
            1
        }
    }
}

// ── IsInstance / AsCast ──────────────────────────────────────────────────────
//
// Both helpers share the `is_subclass_or_eq` walk + the `is_array_isa`
// hardcoded array-base chain (2026-05-07 add-array-base-class).

pub(super) fn is_subclass_or_eq(
    vm: &crate::vm_context::VmContext, module: &crate::metadata::Module, derived: &str, target: &str,
) -> bool {
    // Fast path: zero-alloc &str walk while every link resolves in the MAIN module
    // (the overwhelmingly common case — identical to the pre-fallback walk).
    let mut cur: &str = derived;
    loop {
        if cur == target { return true; }
        let Some(c) = module.classes.iter().find(|c| c.name == cur) else { break; };
        // add-reflection-assignable-from: declared interfaces (FQ-named, zbc 1.20)
        // checked at each level; add-reflection-transitive-interfaces: direct OR transitive.
        if c.interfaces.iter().any(|i| iface_reaches_mod(vm, module, i, target)) { return true; }
        match c.base_class.as_deref() {
            Some(base) => cur = base,
            None       => return false,
        }
    }
    // Slow path (fix-crosspkg-interface-impl / dynamic-component-registration):
    // the chain left the main module — reflectively/lazily loaded types
    // (ModuleLoader.Load + Activator.CreateInstance) resolve via the lazy loader.
    // Allocation is confined to this rare branch.
    let mut cur: String = cur.to_string();
    loop {
        if cur == target { return true; }
        let (ifaces, base): (Vec<String>, Option<String>) =
            if let Some(c) = module.classes.iter().find(|c| c.name == cur) {
                (c.interfaces.iter().map(|s| s.to_string()).collect(), c.base_class.clone())
            } else if let Some(td) = vm.try_lookup_type(cur.as_str()) {
                (td.interfaces().iter().map(|s| s.to_string()).collect(), td.base_name.clone())
            } else {
                return false;
            };
        if ifaces.iter().any(|i| iface_reaches_mod(vm, module, i, target)) { return true; }
        match base {
            Some(b) => cur = b,
            None    => return false,
        }
    }
}

/// add-reflection-transitive-interfaces: JIT mirror of `iface_reaches_td` —
/// true if `iface` equals `target` or reaches it via its transitive base
/// interfaces. Lazy-loader fallback only on main-module miss
/// (fix-crosspkg-interface-impl), mirroring interp.
fn iface_reaches_mod(
    vm: &crate::vm_context::VmContext, module: &crate::metadata::Module, iface: &str, target: &str,
) -> bool {
    // Fast path: direct hit without any allocation.
    if iface == target { return true; }
    let mut queue: Vec<String> = vec![iface.to_string()];
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(name) = queue.pop() {
        if name == target { return true; }
        if !seen.insert(name.clone()) { continue; }
        if let Some(c) = module.classes.iter().find(|c| c.name == name) {
            for bi in c.interfaces.iter() { queue.push(bi.to_string()); }
        } else if let Some(td) = vm.try_lookup_type(name.as_str()) {
            for bi in td.interfaces() { queue.push(bi.to_string()); }
        }
    }
    false
}

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
        Value::Object(rc) => is_subclass_or_eq(vm_ctx_ref(ctx), module, &rc.type_desc().name, class_name),
        Value::Array(_)   => is_array_isa(class_name),
        // add-primitive-value-boxing: 装箱基元走真 type_desc（精确强类型，镜像 interp）；
        // object 短路（基元类名未必可解析到 Std.Object 基链）。
        Value::Boxed(b)   => class_name == "Std.Object" || class_name == "Object"
            || is_subclass_or_eq(vm_ctx_ref(ctx), module, &b.class, class_name),
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
    // add-primitive-value-boxing: Boxed 特判（镜像 interp as_cast）——精确基元类 → 拆箱返 inner；
    // base/接口 → 保持 Boxed；否则 Null。
    if let Value::Boxed(b) = &val {
        let is_obj = class_name == "Std.Object" || class_name == "Object";
        let out = if class_name == &*b.class {
            b.inner.clone()
        } else if is_obj || is_subclass_or_eq(vm_ctx_ref(ctx), module, &b.class, class_name) {
            val.clone()
        } else {
            Value::Null
        };
        (*frame).regs[dst as usize] = out;
        return;
    }
    let is_match = match &val {
        Value::Object(rc) => is_subclass_or_eq(vm_ctx_ref(ctx), module, &rc.type_desc().name, class_name),
        Value::Array(_)   => is_array_isa(class_name),
        Value::Null => true,
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名匹配。
        other       => crate::interp::prim_isa(other, class_name),
    };
    (*frame).regs[dst as usize] = if is_match { val } else { Value::Null };
}

// ── Static fields ────────────────────────────────────────────────────────────

/// `jit_static_get` after formalize-jit-method-token Phase 2 (2026-05-08):
/// receives pre-resolved `StaticFieldId` directly. Resolver populates
/// `Function.resolved.static_field_tokens` at load via lazy ID allocation
/// (always succeeds), so JIT codegen embeds the id as i32 const.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_static_get(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, field_id: u32,
) {
    let id = crate::metadata::tokens::StaticFieldId(field_id);
    (*frame).regs[dst as usize] = vm_ctx_ref(_ctx).static_get_by_id(id);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_static_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    field_id: u32, val: u32,
) {
    let id = crate::metadata::tokens::StaticFieldId(field_id);
    let v = (*frame).regs[val as usize].clone();
    vm_ctx_ref(ctx).static_set_by_id(id, v);
}
