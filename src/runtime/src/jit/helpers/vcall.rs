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
) -> u8 {

    let method    = std::str::from_utf8(std::slice::from_raw_parts(method_ptr, method_len))
        .unwrap_or("<invalid>");
    let ctx_ref   = &*ctx;
    let module    = &*ctx_ref.module;
    let frame_ref = &mut *frame;

    // jit-stack-trace: stamp caller's call-site line once at entry; each
    // invoke path below pushes the callee frame info before running.
    vm_ctx_ref(ctx).update_top_frame_pos(caller_line, caller_col);

    let obj_val = frame_ref.regs[obj as usize].clone();
    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    // Args are read directly from the caller's registers at each call site via
    // `new_method_args_from` (no per-vcall `Vec<Value>` collect); only the
    // primitive-dispatch block below materialises a Vec, and only because its
    // lazy-loader fallback hands a `&[Value]` to the interpreter.

    // ── IC fast path ────────────────────────────────────────────────────
    // Only applies when (1) IC pointer non-null, (2) receiver is an object
    // (primitives go through the dedicated primitive_class_name path
    // below), (3) receiver TypeId matches cache, (4) cached_fn_idx
    // resolves to an entry in `fn_entries_by_id`.
    if !ic_ptr.is_null() {
        // Read the receiver TypeId without keeping a borrow into `obj_val`, so
        // the hit path below can *move* `obj_val` into the callee frame (one
        // receiver clone per vcall instead of two — `jit_vcall` is the #1
        // hotspot in compiler workloads). `recv_type` is a Copy u32.
        let recv_type = match &obj_val {
            Value::Object(rc) => Some(rc.type_desc().id.0),
            _ => None,
        };
        if let Some(recv_type) = recv_type {
            // PIC fast path (review.md C5 P2 — 4-slot linear scan).
            if let Some((_slot, fn_idx)) =
                crate::metadata::resolver::vcall_ic_lookup(&*ic_ptr, recv_type)
            {
                if fn_idx != crate::metadata::tokens::UNRESOLVED {
                    if let Some(entry) = ctx_ref.fn_entries_by_id.get(fn_idx as usize).and_then(|o| o.as_ref()) {
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

    // add-primitive-value-boxing: 装箱基元方法调用（镜像 interp/exec_vcall.rs Boxed 臂）。
    // GetType 保留装箱类（不拆箱 this，否则按 inner 默认类报告丢宽度）；其余方法拆箱
    // `this = inner` 交基元 struct 方法体（+ arity 重载，fallback Std.Object）。
    if let Value::Boxed(b) = &obj_val {
        let vm_ctx = vm_ctx_ref(ctx);
        if method == "GetType" && argc == 0 {
            match crate::corelib::object::builtin_obj_get_type(vm_ctx, &[obj_val.clone()]) {
                Ok(ty) => { frame_ref.regs[dst as usize] = ty; return 0; }
                Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); return 1; }
            }
        }
        let class_name = b.class.clone();
        let mut call_args: Vec<Value> = Vec::with_capacity(argc + 1);
        call_args.push(b.inner.clone());
        call_args.extend(arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()));
        let arity = argc;
        let candidates = [
            format!("{}.{}${}", class_name, method, arity),
            format!("{}.{}", class_name, method),
            format!("Std.Object.{}${}", method, arity),
            format!("Std.Object.{}", method),
        ];
        for func_name in &candidates {
            if let Some(entry) = ctx_ref.fn_entries.get(func_name.as_str()) {
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
            if let Some(entry) = ctx_ref.fn_entries.get(func_name) {
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

    let (class_name, recv_type_id) = match &obj_val {
        Value::Object(rc) => {
            let b = rc.borrow();
            (b.type_desc.name.clone(), b.type_desc.id.0)
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("VCall: expected object, got {:?}", other).into()));
            return 1;
        }
    };

    let func_name = match resolve_virtual(vm_ctx_ref(ctx), module, &class_name, method) {
        Ok(n)  => n,
        Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
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

    let entry = match ctx_ref.fn_entries.get(&func_name) {
        Some(e) => e,
        None => {
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
