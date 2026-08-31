#![allow(dangerous_implicit_autorefs)]
//! Virtual dispatch (`jit_vcall`). Single-helper file because of size and
//! the L3-G4b primitive-as-struct + lazy-loader fallback paths it carries.
//! Mirrors `interp/exec_vcall.rs`.

use crate::metadata::Value;
use crate::metadata::resolver::VCallIC;

use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

/// `jit_vcall` after formalize-jit-method-token Phase 2.E (2026-05-08):
/// the per-site `VCallIC` is threaded in (stable raw pointer baked into
/// machine code by codegen). Mirrors interp `vcall` — IC hit goes
/// straight to `fn_entries_by_id[cached_fn_idx]`; miss falls through
/// the existing primitive / vtable / lazy-loader paths and writes the
/// resolved (TypeId, vtable slot, MethodId) triple back to IC.
///
/// `ic_ptr` may be null when the resolver hasn't run (only happens in
/// tests bypassing `Vm::run`); helper degrades gracefully to slow path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_vcall(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, obj: u32, method_ptr: *const u8, method_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const VCallIC,
    caller_line: u32,   // 2026-05-10 jit-stack-trace
    caller_col:  u32,   // 2026-05-10 span-column-propagate
    caller_offset: u32, // add-offline-symbolication: linearized code offset
) -> u8 {

    // lean-jit-vcall-hit-path: `method` (the UTF-8 method name) is decoded lazily
    // *after* the IC fast path below — the monomorphic hit path returns without ever
    // needing the name (it dispatches by cached fn_idx), so keeping `from_utf8` off the
    // hot path saves a per-vcall UTF-8 decode (~6-9% of vcall-heavy JIT, measured).
    // Only the IC-miss / boxed / primitive / vtable paths reference `method`.
    let ctx_ref   = &*ctx;
    let module    = &*ctx_ref.module;
    let frame_ref = &mut *frame;

    // jit-stack-trace: stamp caller's call-site line + offset once at entry; each
    // invoke path below pushes the callee frame info before running.
    vm_ctx_ref(ctx).update_top_frame_pos(caller_line, caller_col, caller_offset);

    let obj_val = frame_ref.regs[obj as usize].clone();
    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    // Args are read directly from the caller's registers at each call site via
    // `new_method_args_from` (no per-vcall `Vec<Value>` collect); only the
    // primitive-dispatch block below materialises a Vec, and only because its
    // lazy-loader fallback hands a `&[Value]` to the interpreter.

    // ── IC fast path ────────────────────────────────────────────────────
    // Applies when (1) IC pointer non-null, (2) receiver is an object OR a
    // primitive — objects key on their real `TypeDesc.id`, primitives on the
    // synthetic `PRIM_TYPE_*` id (add-jit-primitive-vcall-ic, mirroring interp
    // exec_vcall.rs); Boxed / Null return None and fall through to the boxed /
    // primitive_class_name paths below — (3) receiver id matches cache, (4)
    // cached_fn_idx resolves to an entry in `fn_entries_by_id`. Without this,
    // a primitive receiver (notably string dict keys calling GetHashCode /
    // Equals) paid the `format!`×4 + `Vec` slow path on EVERY call.
    if !ic_ptr.is_null() {
        // Read the receiver id without keeping a borrow into `obj_val`, so the
        // hit path below can *move* `obj_val` into the callee frame (one
        // receiver clone per vcall instead of two — `jit_vcall` is the #1
        // hotspot in compiler workloads). `recv_type` is a Copy u32.
        let recv_type = match &obj_val {
            Value::Object(rc) => Some(rc.type_desc().id.0),
            other => crate::interp::value_synthetic_type_id(other),
        };
        if let Some(recv_type) = recv_type {
            // PIC fast path (review.md C5 P2 — 4-slot linear scan).
            if let Some((_slot, fn_idx)) =
                crate::metadata::resolver::vcall_ic_lookup(&*ic_ptr, recv_type)
            {
                if fn_idx != crate::metadata::tokens::UNRESOLVED {
                    // runtime-jit-tiering Phase 1b: tiered — cold method → None →
                    // fall through to the vtable path (262), whose None-arm interps it.
                    if let Some(entry) = ctx_ref.resolve_fn_by_id_tiered(fn_idx as usize) {
                        // Move `obj_val` in — this branch always returns, so the
                        // primitive / vtable fall-through paths never observe the
                        // move (conditional-move-into-diverging-branch).
                        let mut callee = JitFrame::new_method_args_from(
                            entry.max_reg, obj_val, &frame_ref.regs, arg_regs);
                        let jit_fn: JitFn = std::mem::transmute(entry.ptr);
                        let vm_ctx = vm_ctx_ref(ctx);
                        vm_ctx.push_frame(crate::exception::VmFrame::new(
                            entry.name.clone(), entry.file.clone(),
                            &callee.regs as *const _, &callee.env_arena as *const _));
                        let r = jit_fn(&mut callee, ctx);
                        vm_ctx.pop_frame();
                        if r != 0 { callee.recycle(); return 1; }
                        frame_ref.regs[dst as usize] = callee.ret.take().unwrap_or(Value::Null);
                        callee.recycle();
                        return 0;
                    }
                }
            }
        }
    }

    // lean-jit-vcall-hit-path: IC missed (or receiver is Boxed/Null, or the cached
    // target was cold/untranslatable) — decode the method name now; every path below
    // (boxed / primitive / vtable resolve) needs it. `method_ptr`/`method_len` are
    // function params, valid for the whole call.
    let method = std::str::from_utf8(std::slice::from_raw_parts(method_ptr, method_len))
        .unwrap_or("<invalid>");

    // add-primitive-value-boxing → unify Phase 2 R3: 装箱基元方法调用（镜像 interp/exec_vcall.rs）。
    // 基元盒现是 `BoxedStruct`（整数标量存 struct_bytes）；`boxed_prim_i64` 拆回标量。GetType 保留装箱类
    // （不拆箱 this，否则按标量默认类报告丢宽度）；其余方法拆箱 `this = 裸标量` 交基元 struct 方法体
    // （+ arity 重载，fallback Std.Object）。struct 装箱盒（None）落下方 struct 对象协议块。
    if let Value::BoxedStruct(gc) = &obj_val {
      if let Some(scalar) = gc.borrow().boxed_prim_i64() {
        let vm_ctx = vm_ctx_ref(ctx);
        if method == "GetType" && argc == 0 {
            match crate::corelib::object::builtin_obj_get_type(vm_ctx, &[obj_val.clone()]) {
                Ok(ty) => { frame_ref.regs[dst as usize] = ty; return 0; }
                Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
            }
        }
        let class_name = gc.type_desc().name.clone();
        let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);
        call_args.push(Value::I64(scalar));
        call_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
        let arity = argc;
        let candidates = [
            format!("{}.{}${}", class_name, method, arity),
            format!("{}.{}", class_name, method),
            format!("Std.Object.{}${}", method, arity),
            format!("Std.Object.{}", method),
        ];
        for func_name in &candidates {
            if let Some(entry) = ctx_ref.resolve_fn_by_name(func_name.as_str()) {
                let mut callee = JitFrame::new(entry.max_reg, &call_args);
                let jit_fn: JitFn = std::mem::transmute(entry.ptr);
                vm_ctx.push_frame(crate::exception::VmFrame::new(
                    entry.name.clone(), entry.file.clone(),
                    &callee.regs as *const _, &callee.env_arena as *const _));
                let r = jit_fn(&mut callee, ctx);
                vm_ctx.pop_frame();
                if r != 0 { callee.recycle(); return 1; }
                frame_ref.regs[dst as usize] = callee.ret.take().unwrap_or(Value::Null);
                callee.recycle();
                return 0;
            }
            if let Some(callee) = module.func_index
                .get(func_name.as_str()).and_then(|&idx| module.functions.get(idx))
            {
                match crate::interp::exec_function(vm_ctx, module, callee, &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); return 0;
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); return 1; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
            if let Some(lazy_fn) = vm_ctx.try_lookup_function(func_name) {
                match crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); return 0;
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); return 1; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
        }
        set_exception(vm_ctx, Value::Str(
            format!("VCall on boxed `{}`: method `{}` (arity {}) not found", class_name, method, arity).into()));
        return 1;
      }
    }

    // add-struct-object-methods (PR2b): 装箱 struct 对象协议方法（镜像 interp/exec_vcall.rs BoxedStruct 臂）。
    // GetType/GetHashCode/ToString → native；Equals(+用户方法) → {type_name}.{method}$arity 合成/声明方法。
    if let Value::BoxedStruct(b) = &obj_val {
        let vm_ctx = vm_ctx_ref(ctx);
        if argc == 0 {
            if method == "GetType" {
                match crate::corelib::object::builtin_obj_get_type(vm_ctx, &[obj_val.clone()]) {
                    Ok(ty) => { frame_ref.regs[dst as usize] = ty; return 0; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
            if method == "GetHashCode" {
                match crate::corelib::convert::builtin_struct_hash_code(vm_ctx, &[obj_val.clone()]) {
                    Ok(h) => { frame_ref.regs[dst as usize] = h; return 0; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
            // add-record-value-semantics: record structs step aside from the native type-name
            // intercept so their compiler-synthesized `<Type>.ToString` (record format) is reached
            // via the candidate lookup below (mirrors interp exec_vcall.rs).
            if method == "ToString" && !b.type_desc().is_record() {
                // add-boxed-struct-identity (P4b): type name lives on the box's shared object.
                let n: &str = &b.type_desc().name;
                let short = n.rsplit('.').next().unwrap_or(n);
                frame_ref.regs[dst as usize] = Value::Str(short.into()); return 0;
            }
        }
        let type_name = b.type_desc().name.to_string();
        let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);
        call_args.push(obj_val.clone());
        call_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
        let arity = argc;
        let candidates = [
            format!("{}.{}${}", type_name, method, arity),
            format!("{}.{}", type_name, method),
            format!("Std.Object.{}${}", method, arity),
            format!("Std.Object.{}", method),
        ];
        for func_name in &candidates {
            if let Some(entry) = ctx_ref.resolve_fn_by_name(func_name.as_str()) {
                let mut callee = JitFrame::new(entry.max_reg, &call_args);
                let jit_fn: JitFn = std::mem::transmute(entry.ptr);
                vm_ctx.push_frame(crate::exception::VmFrame::new(
                    entry.name.clone(), entry.file.clone(),
                    &callee.regs as *const _, &callee.env_arena as *const _));
                let r = jit_fn(&mut callee, ctx);
                vm_ctx.pop_frame();
                if r != 0 { callee.recycle(); return 1; }
                frame_ref.regs[dst as usize] = callee.ret.take().unwrap_or(Value::Null);
                callee.recycle();
                return 0;
            }
            if let Some(callee) = module.func_index
                .get(func_name.as_str()).and_then(|&idx| module.functions.get(idx))
            {
                match crate::interp::exec_function(vm_ctx, module, callee, &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); return 0;
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); return 1; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
            if let Some(lazy_fn) = vm_ctx.try_lookup_function(func_name) {
                match crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null); return 0;
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); return 1; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
        }
        set_exception(vm_ctx, Value::Str(
            format!("VCall on boxed struct `{}`: method `{}` (arity {}) not found",
                    type_name, method, arity).into()));
        return 1;
    }

    // L3-G4b primitive-as-struct: primitives dispatch through their stdlib struct's
    // method — construct `{Std.Int32 | Std.Double | ...}.{method}` and invoke via the
    // JIT entry cache. Replaces the old hardcoded `(Value, method) → builtin` table.
    //
    // Overload resolution: when the receiver type is statically `object` the IR
    // carries the unmangled method name (e.g. `Equals`), but IrGen emits overloaded
    // methods with a `$<arity>` suffix (e.g. `Std.String.Equals$1`). When the
    // unmangled lookup misses we retry with the arity-suffixed name. Mirrors
    // `interp/exec_vcall.rs::vcall`. Subsumes the legacy `Value::Str`
    // hardcoded `__str_*` fallback (review2 §2.2).
    if let Some(class_name) = crate::interp::primitive_class_name(&obj_val) {
        // Materialise (this, args…) once: the lazy-loader fallback below passes
        // a `&[Value]` to the interpreter, so this block genuinely needs a Vec.
        let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);
        call_args.push(obj_val.clone());
        call_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
        let arity = argc; // exclude `this`
        let primary = format!("{}.{}", class_name, method);
        let overload = format!("{}.{}${}", class_name, method, arity);
        // Std.Object 兜底（镜像 interp）：基元未 override 的协议方法（尤 GetType）走基类实现。
        let obj_primary = format!("Std.Object.{}", method);
        let obj_overload = format!("Std.Object.{}${}", method, arity);
        for func_name in [primary.as_str(), overload.as_str(),
                          obj_primary.as_str(), obj_overload.as_str()] {
            if let Some(entry) = ctx_ref.resolve_fn_by_name(func_name) {
                // add-jit-primitive-vcall-ic: install (synthetic prim type id →
                // module fn_idx) so the next call at this site with the same
                // primitive receiver takes the IC fast path above — skipping the
                // `format!`×4 candidate names + `Vec` this slow path pays. Only
                // intra-module funcs (the fast path resolves fn_idx through
                // `resolve_fn_by_id_tiered`); cross-zpkg / lazy funcs are left
                // uninstalled, mirroring interp `exec_vcall.rs`.
                if !ic_ptr.is_null() {
                    if let (Some(synth_id), Some(&idx)) =
                        (crate::interp::value_synthetic_type_id(&obj_val),
                         module.func_index.get(func_name))
                    {
                        crate::metadata::resolver::vcall_ic_install(
                            &*ic_ptr, synth_id,
                            crate::metadata::tokens::UNRESOLVED,
                            idx as u32,
                        );
                    }
                }
                let mut callee = JitFrame::new(entry.max_reg, &call_args);
                let jit_fn: JitFn = std::mem::transmute(entry.ptr);
                let vm_ctx = vm_ctx_ref(ctx);
                vm_ctx.push_frame(crate::exception::VmFrame::new(
                    entry.name.clone(), entry.file.clone(),
                    &callee.regs as *const _, &callee.env_arena as *const _));
                let r = jit_fn(&mut callee, ctx);
                vm_ctx.pop_frame();
                if r != 0 { callee.recycle(); return 1; }
                frame_ref.regs[dst as usize] = callee.ret.take().unwrap_or(Value::Null);
                callee.recycle();
                return 0;
            }
            // Reach VmContext through the JIT module ctx pointer (set by
            // JitModule::run for the duration of this entry call).
            let vm_ctx = vm_ctx_ref(ctx);
            // fix-jit-cross-zpkg-transitive-eager (2026-06-20): merged-module
            // fallback. The method may live in the merged module but have been
            // skipped by `compile_module` (interp-only opcode), so it's absent
            // from `fn_entries` yet `try_lookup_function` (lazy-loader only)
            // can't see it either. Resolve via `module.func_index` and interp it.
            if let Some(callee) = module.func_index
                .get(func_name).and_then(|&idx| module.functions.get(idx))
            {
                match crate::interp::exec_function(vm_ctx, module, callee, &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
                        return 0;
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); return 1; }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
            // Lazy loader fallback — callee lives in a not-yet-loaded zpkg.
            if let Some(lazy_fn) = vm_ctx.try_lookup_function(func_name) {
                match crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &call_args) {
                    Ok(outcome) => match outcome {
                        crate::interp::ExecOutcome::Returned(ret) => {
                            frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
                            return 0;
                        }
                        crate::interp::ExecOutcome::Thrown(val) => {
                            set_exception(vm_ctx, val);
                            return 1;
                        }
                    },
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
                }
            }
        }
        // `call_args` drops here; the vtable path below re-reads args from the
        // caller's registers directly (no hand-off needed).
    }

    // fix-jit-vcall-overload-dispatch: resolve the override name via the type's
    // `vtable_index`/`vtable` FIRST — mirroring interp's primary path
    // (`interp::exec_vcall::vcall`). The `vtable_index` maps a (possibly
    // overloaded) method name to the CORRECT override slot the compiler bound the
    // call site to. `resolve_virtual`'s naive `Class.method` string walk instead
    // grabs whichever same-named function it hits first — the WRONG overload when a
    // type has two methods sharing a name (e.g. a type implementing
    // `IEquatable<T>` has both `Equals(T)` and the `Object.Equals(object)`
    // override; a generic/object-typed receiver's call site carries the unmangled
    // `Equals`, which resolve_virtual maps to `Class.Equals` = `Equals(T)` instead
    // of the intended `Object.Equals` override). Fall back to resolve_virtual when
    // the type has no vtable entry (fallback synthetic descriptors / cross-zpkg).
    let (class_name, recv_type_id, vtable_name) = match &obj_val {
        Value::Object(rc) => {
            let b = rc.borrow();
            let vt_name = b.type_desc.vtable_index.get(method)
                .and_then(|&slot| b.type_desc.vtable.get(slot))
                .map(|entry| entry.1.clone());
            (b.type_desc.name.clone(), b.type_desc.id.0, vt_name)
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("VCall: expected object, got {:?}", other).into()));
            return 1;
        }
    };

    let func_name = match vtable_name {
        Some(n) => n,
        None => match resolve_virtual(vm_ctx_ref(ctx), module, &class_name, method) {
            Ok(n)  => n,
            Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
        },
    };

    // PIC install: cache (recv_type_id, fn_idx) in the next available slot
    // for next time this site sees the same receiver type. Slot index is
    // UNRESOLVED (resolve_virtual walks by name, not vtable index — the
    // PIC fast path only consults fn_idx for native dispatch).
    if !ic_ptr.is_null() && recv_type_id != crate::metadata::tokens::UNRESOLVED {
        if let Some(&fn_idx) = module.func_index.get(&func_name) {
            crate::metadata::resolver::vcall_ic_install(
                &*ic_ptr, recv_type_id,
                crate::metadata::tokens::UNRESOLVED,
                fn_idx as u32,
            );
        }
    }

    let entry = match ctx_ref.resolve_fn_by_name_tiered(func_name.as_str()) {
        Some(e) => e,
        None => {
            // runtime-jit-tiering Phase 1b: tiered — a cold (below-threshold) method
            // resolves to None here and runs on the interpreter via the arms below
            // (receiver + args), exactly like an untranslatable method. At the
            // threshold it compiles and subsequent calls take the native path.
            // fix-jit-cross-zpkg-transitive-eager (2026-06-20): the resolved
            // virtual method lives in the merged module but was not JIT-compiled
            // (it contains an interp-only opcode such as `LoadLocalAddr`, so
            // `compile_module` skipped it). Run it on the interpreter — mirrors
            // the primitive-receiver lazy fallback above and `jit_call`'s
            // `cross_zpkg_via_interp` Case 1. Without this an `out`/`ref` virtual
            // method would abort under `--mode jit`.
            let vm_ctx = vm_ctx_ref(ctx);
            // fix-crosspkg-interface-impl: lazily-loaded function (injected component)
            // -- not in the main module at all; fetch from the lazy loader and interp-exec.
            if let Some(lazy_fn) = (!module.func_index.contains_key(&func_name))
                .then(|| vm_ctx.try_lookup_function(&func_name)).flatten()
            {
                let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);
                call_args.push(obj_val);
                call_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
                return match crate::interp::exec_function(vm_ctx, module, lazy_fn.as_ref(), &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
                        0
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
                };
            }
            if let Some(callee) = module.func_index.get(&func_name)
                .and_then(|&idx| module.functions.get(idx))
            {
                let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);
                call_args.push(obj_val);
                call_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
                return match crate::interp::exec_function(vm_ctx, module, callee, &call_args) {
                    Ok(crate::interp::ExecOutcome::Returned(ret)) => {
                        frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
                        0
                    }
                    Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
                    Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
                };
            }
            set_exception(vm_ctx, Value::Str(format!("VCall: compiled entry for `{}` not found", func_name).into()));
            return 1;
        }
    };

    let mut callee = JitFrame::new_method_args_from(
        entry.max_reg, obj_val, &frame_ref.regs, arg_regs);
    let jit_fn: JitFn = std::mem::transmute(entry.ptr);
    let vm_ctx = vm_ctx_ref(ctx);
    vm_ctx.push_frame(crate::exception::VmFrame::new(
        entry.name.clone(), entry.file.clone(),
        &callee.regs as *const _, &callee.env_arena as *const _));
    let r = jit_fn(&mut callee, ctx);
    vm_ctx.pop_frame();
    if r != 0 { callee.recycle(); return 1; }
    frame_ref.regs[dst as usize] = callee.ret.take().unwrap_or(Value::Null);
    callee.recycle();
    0
}

fn resolve_virtual(
    vm: &crate::vm_context::VmContext, module: &crate::metadata::Module, class_name: &str, method: &str,
) -> anyhow::Result<String> {
    let mut cur: String = class_name.to_string();
    loop {
        let qualified = format!("{}.{}", cur, method);
        if module.functions.iter().any(|f| f.name == qualified) { return Ok(qualified); }
        // fix-crosspkg-interface-impl (dynamic-component-registration): lazy-loader
        // fallback -- reflectively/lazily loaded receivers (ModuleLoader.Load +
        // Activator) have neither functions nor classes in the MAIN module; without
        // this the injected component's methods were unreachable under JIT.
        if vm.try_lookup_function(&qualified).is_some() { return Ok(qualified); }
        let base = module.classes.iter().find(|c| c.name == cur)
            .and_then(|c| c.base_class.clone())
            .or_else(|| vm.try_lookup_type(cur.as_str()).and_then(|td| td.base_name.clone()));
        match base {
            Some(b) => cur = b,
            None => anyhow::bail!("VCall: no implementation of `{}` found in hierarchy of `{}`", method, class_name),
        }
    }
}
