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
    // make-vm-loading-lazy: an imported class (e.g. Std.Cli.SubcommandRouter) is
    // NOT in the merged module's type registry until first use — it lives in the
    // lazy loader. Probe it there before the blank-descriptor fallback, mirroring
    // interp's `exec_object::obj_new`. Without this, `new SubcommandRouter()` gets
    // a zero-field TypeDesc → zero slots → every field read returns Null (observed:
    // `this._count` reads Null → `I64(0) vs Null` in SubcommandRouter.Add).
    let type_desc = module.type_lookup(&class_name).cloned()
        .or_else(|| vm_ctx_ref(ctx).try_lookup_type(&class_name))
        .unwrap_or_else(|| std::sync::Arc::new(crate::metadata::TypeDesc {
            name: class_name.clone(), base_name: None,
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
    if let Some(entry) = ctx_ref.resolve_fn_by_name_tiered(ctor_name.as_str()) {
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
        let module = &*ctx_ref.module;
        let oc = if let Some(callee) = module.func_index.get(ctor_name.as_str())
            .and_then(|&idx| module.functions.get(idx))
        {
            Some(crate::interp::exec_function(vm_ctx, module, callee, &ctor_args))
        } else if let Some(lazy_fn) = vm_ctx.try_lookup_function(ctor_name.as_str()) {
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

/// post-layout JIT perf (P5-B): **non-throwing** byte-aware field resolver for the
/// loop-invariant hoist. Emitted ONCE in the JIT entry block for an object register
/// proven never-reassigned (e.g. `this`) + a fixed field name. If the field is a
/// direct **inline primitive** (scalar packed in `bytes`) whose runtime width/tag
/// match the JIT's compile-time expectation (from `reg_types`), writes
/// `out_bytes_ptr = bytes.as_ptr()` and `out_off = byte offset`; the per-`FieldGet`/
/// `FieldSet` inline then does a native width-aware byte load/store. Otherwise
/// (non-object / null / field-not-found / reference / inlined obj-array ref /
/// struct root / string / width-or-tag mismatch) writes `out_off = -1` and does NOT
/// throw — the inline detects `off < 0` and falls back to `jit_field_get`/
/// `jit_field_set` (correct exception / Str.Length / write-barrier semantics at the
/// real site). GC-safe: non-moving collector + fixed `bytes` allocation + the object
/// is held live ⇒ the returned ptr stays valid for the frame.
///
/// unify-object-byte-layout (PR-2) had stubbed this to always signal "no fast path"
/// (byte storage broke the old `slots.as_ptr()` + STRIDE assumption); P5-B restores
/// it against the `bytes` layout via `ScriptObject::inline_prim_field`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_field_slot(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    obj: u32, field_name_ptr: *const u8, field_name_len: usize,
    expected_width: u32, expected_tag: u32,
    out_bytes_ptr: *mut *const u8, out_off: *mut i64,
) {
    // default: no fast path → the inline sees off<0 and routes to the helper.
    *out_bytes_ptr = std::ptr::null();
    *out_off = -1;
    let field_name = match std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len)) {
        Ok(s) => s,
        Err(_) => return,
    };
    let Value::Object(rc) = &(*frame).regs[obj as usize] else { return };
    let b = rc.borrow();
    if let Some((ptr, off, width, tag)) = b.inline_prim_field(field_name) {
        // Confirm the runtime layout matches the JIT's compile-time (width, tag) —
        // guards against aliases / layout surprises: a mismatch keeps the helper.
        if width == expected_width && tag as u32 == expected_tag {
            *out_bytes_ptr = ptr;
            *out_off = off as i64;
        }
    }
}

/// post-layout JIT perf (T1-B): **non-throwing** byte-inlined **reference** field
/// resolver for the loop-invariant hoist — the reference twin of
/// [`jit_obj_field_slot`]. Emitted ONCE in the JIT entry block for an object register
/// proven never-reassigned (e.g. `this`) + a fixed field name. If the field is a
/// direct byte-inlined class-instance or array reference (8B tagged pointer in
/// `bytes`, `ref_slot == -1`), writes `out_bytes_ptr = bytes.as_ptr()`,
/// `out_off = byte offset`, and `out_tag` = the `Value` discriminant to stamp on a
/// non-null load (`7` = `Value::Object` / `6` = `Value::Array`); the per-`FieldGet`
/// inline then does a native 8B load + `raw==0 ? Null : tagged store`, byte-identical
/// to `read_inline_ref`. Otherwise (non-object receiver / null / field-not-found /
/// primitive / side-table reference = closure·func·**string** / struct root) writes
/// `out_off = -1` and does NOT throw — the inline detects `off < 0` and falls back to
/// `jit_field_get` (correct Str.Length / null-throw / string-GcRef semantics at the
/// real site). GC-safe: non-moving collector + fixed `bytes` allocation + the object
/// held live ⇒ the returned ptr stays valid for the frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_obj_ref_field_slot(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    obj: u32, field_name_ptr: *const u8, field_name_len: usize,
    out_bytes_ptr: *mut *const u8, out_off: *mut i64, out_tag: *mut i32,
) {
    // default: no fast path → the inline sees off<0 and routes to the helper.
    *out_bytes_ptr = std::ptr::null();
    *out_off = -1;
    *out_tag = 0;
    let field_name = match std::str::from_utf8(std::slice::from_raw_parts(field_name_ptr, field_name_len)) {
        Ok(s) => s,
        Err(_) => return,
    };
    let Value::Object(rc) = &(*frame).regs[obj as usize] else { return };
    let b = rc.borrow();
    if let Some((ptr, off, is_array)) = b.inline_ref_field(field_name) {
        *out_bytes_ptr = ptr;
        *out_off = off as i64;
        // `Value` `#[repr(C, u8)]` discriminants (pinned by `value_discriminants_pinned`
        // in `metadata/types_tests.rs`): `Array` = 6, `Object` = 7.
        *out_tag = if is_array { 6 } else { 7 };
    }
}

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
        // fix-jit-osr-stackobject: under OSR the interp portion may have created a
        // stack-allocated object (escape analysis) that is live in `frame.regs` when
        // the JIT takes over. Mirror interp `field_get` — resolve via the per-context
        // stack arena, reusing the same monomorphic `FieldIC` as the heap path.
        // (Non-OSR JIT never produces a StackObject, so this arm only fires on the OSR
        // entry path. See `jit_array_get` for the StackArray analogue.)
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            let res = vm_ctx_ref(ctx).stack_arena.lock().with_obj(idx, frame_id, |obj| {
                if !ic_ptr.is_null() {
                    let recv_type = obj.type_desc.id.0;
                    if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                        return obj.field_value(slot as usize);
                    }
                    if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                        return obj.field_value(slot);
                    }
                    return Value::Null;
                }
                match obj.type_desc.field_index.get(field_name) {
                    Some(&slot) => obj.field_value(slot),
                    None => Value::Null,
                }
            });
            match res {
                Ok(v) => v,
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
            }
        }
        Value::Object(rc) => {
            let b = rc.borrow();
            // PIC fast path (review.md C4 P2 — 4-slot linear scan).
            if !ic_ptr.is_null() {
                let recv_type = b.type_desc.id.0;
                if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                    let v = b.field_value(slot as usize);
                    (*frame).regs[dst as usize] = v;
                    return 0;
                }
                if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                    crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                    b.field_value(slot)
                } else { Value::Null }
            } else if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                b.field_value(slot)
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
        // fix-jit-osr-stackobject: OSR-entry stack object — write the slot in the
        // arena (validated), mirroring interp `field_set`. No GC write barrier: the
        // stack object is not a heap slot; its heap-ref fields are kept live by
        // root-scanning the arena. Reuses the same FieldIC as the heap path. See
        // `jit_field_get` / `jit_array_set`.
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            let res = vm_ctx_ref(ctx).stack_arena.lock().with_obj_mut(idx, frame_id, |obj| {
                let slot_opt: Option<usize> = if !ic_ptr.is_null() {
                    let recv_type = obj.type_desc.id.0;
                    if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                        Some(slot as usize)
                    } else if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                        Some(slot)
                    } else {
                        None
                    }
                } else {
                    obj.type_desc.field_index.get(field_name).copied()
                };
                if let Some(slot) = slot_opt {
                    obj.set_field_value(slot, &v);
                }
            });
            match res {
                Ok(()) => 0,
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
            }
        }
        Value::Object(rc) => {
            let mut b = rc.borrow_mut();
            // PIC fast path
            if !ic_ptr.is_null() {
                let recv_type = b.type_desc.id.0;
                if let Some(slot) = crate::metadata::resolver::field_ic_lookup(&*ic_ptr, recv_type) {
                    let slot = slot as usize;
                    let wrote_ref = b.set_field_value(slot, &v);
                    drop(b);
                    if wrote_ref && v.is_heap_ref() {
                        vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                    }
                    return 0;
                }
                let slot_opt = b.type_desc.field_index.get(field_name).copied();
                if let Some(slot) = slot_opt {
                    crate::metadata::resolver::field_ic_install(&*ic_ptr, recv_type, slot as u32);
                    let wrote_ref = b.set_field_value(slot, &v);
                    drop(b);
                    if wrote_ref && v.is_heap_ref() {
                        vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
                    }
                }
            } else if let Some(&slot) = b.type_desc.field_index.get(field_name) {
                let wrote_ref = b.set_field_value(slot, &v);
                drop(b);
                if wrote_ref && v.is_heap_ref() {
                    vm_ctx_ref(ctx).heap().write_barrier_field(&owner, slot, &v);
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
        // add-struct-object-boxing → unify Phase 2 R3: 装箱值类型（struct 或基元）is-a 精确类型 /
        // object（镜像 interp is_instance；基元盒 type_desc.name 即精确 wrapper）。
        Value::BoxedStruct(b) => class_name == "Std.Object" || class_name == "Object"
            || &*b.type_desc().name == class_name
            || is_subclass_or_eq(vm_ctx_ref(ctx), module, &b.type_desc().name, class_name),
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
        } else if is_obj || is_subclass_or_eq(vm_ctx_ref(ctx), module, &b.type_desc().name, class_name) {
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
