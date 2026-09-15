#![allow(dangerous_implicit_autorefs)]
//! Direct call (`jit_call`) and corelib builtin dispatch (`jit_builtin`).

use crate::metadata::Value;

use super::super::frame::{FnEntry, JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref, JitFn};

/// `jit_call` after formalize-jit-method-token Phase 2.C (2026-05-08):
/// hot path takes pre-resolved `MethodId` and indexes `fn_entries_by_id`
/// directly. On `UNRESOLVED` (cross-zpkg), falls back to name-based
/// HashMap lookup. Name pointer kept for diagnostics + fallback.
///
/// `caller_line` / `caller_col` (jit-stack-trace + span-column-propagate,
/// 2026-05-10) are the source position of this call site — codegen passes
/// both as constants. Stamped onto the caller's FrameInfo before descending
/// so a downstream throw's snapshot shows the precise call site.
/// `caller_col == 0` means unknown (zbc < 1.1) — formatter degrades to
/// `(file:line)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_call(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    method_id: u32,
    fn_name_ptr: *const u8, fn_name_len: usize,
    args_ptr: *const u32, argc: usize,
    ic_ptr: *const std::sync::atomic::AtomicU32,
    caller_line: u32,
    caller_col:  u32,
    caller_offset: u32, // add-offline-symbolication: linearized code offset
) -> u8 {
    use crate::metadata::tokens::UNRESOLVED;
    let ctx_ref   = &*ctx;
    let frame_ref = &mut *frame;

    // Resolve the callee to a slot id, then to its (lazily-compiled) FnEntry.
    // Three tiers, cheapest first:
    //   1. `method_id` resolved at codegen (intra-module) → by-id, lock-free.
    //   2. per-site IC hit (make-vm-loading-lazy) → cached id, by-id, lock-free.
    //   3. cold: resolve the name (registers a lazy slot if cross-zpkg), then
    //      cache the id in the IC so tiers 2 wins next time.
    // Borrow the FnEntry (don't clone): it lives in the read-only `ctx_ref`;
    // cloning copied two `Arc<str>` (name + file) per call re-cloned in
    // push_frame anyway.
    let entry_ref: Option<&FnEntry> = if method_id != UNRESOLVED {
        ctx_ref.resolve_fn_by_id_tiered(method_id as usize)
    } else {
        // Tier 2: per-site inline cache.
        let cached = if ic_ptr.is_null() {
            UNRESOLVED
        } else {
            (*ic_ptr).load(std::sync::atomic::Ordering::Relaxed)
        };
        if cached != UNRESOLVED {
            ctx_ref.resolve_fn_by_id_tiered(cached as usize)
        } else {
            // Tier 3: resolve by name (may register a synthetic lazy id), then
            // populate the IC. `resolve_id_by_name` returns None for a target
            // that is untranslatable or reachable only via the interpreter.
            let func_name = std::str::from_utf8(std::slice::from_raw_parts(fn_name_ptr, fn_name_len))
                .unwrap_or("<invalid>");
            match ctx_ref.resolve_id_by_name(func_name) {
                Some(id) => {
                    // fix-call-arity-skew：tier 3 是 tier 2 IC 的**唯一**写入者 ⇒ 首次绑定点，必须在写 IC 之前判。
                    // 不能用 FnEntry 上预算的 arity：被调方还没到 JIT 阈值时 `resolve_fn_by_id_tiered` 返回 None，
                    // 而 IC 照样写回 —— 等它变热 tier 2 直接命中就绕过了校验。所以按名字取 `Function` 自己判。
                    // （`None` 分支不缓存、直接走 `cross_zpkg_via_interp`，那里自己判，这里不重复付查找代价。）
                    {
                        let vm = vm_ctx_ref(ctx);
                        let module = &*ctx_ref.module;
                        let mismatch = match module.func_index.get(func_name).and_then(|&i| module.functions.get(i)) {
                            Some(f) => crate::vm_context::symres::wrong_arity_exception(
                                vm, module, func_name, crate::vm_context::symres::call_arity(f), argc),
                            None => vm.try_lookup_function(func_name).and_then(|f|
                                crate::vm_context::symres::wrong_arity_exception(
                                    vm, module, func_name, crate::vm_context::symres::call_arity(f.as_ref()), argc)),
                        };
                        if let Some(exc) = mismatch {
                            set_exception(vm, exc);
                            return 1;
                        }
                    }
                    if !ic_ptr.is_null() {
                        (*ic_ptr).store(id, std::sync::atomic::Ordering::Relaxed);
                    }
                    ctx_ref.resolve_fn_by_id_tiered(id as usize)
                }
                None => None,
            }
        }
    };

    let entry: &FnEntry = match entry_ref {
        Some(e) => e,
        None => {
            // Cross-zpkg lazy-loader fallback: the callee is untranslatable or
            // lives in another zpkg not JIT-compiled into this module, so there
            // is no `FnEntry`. Resolve it via the VM context and run it through
            // the interpreter — mirrors interp `exec_call::call`'s
            // `try_lookup_function` path and `jit_vcall`'s lazy fallback.
            // Without this, a static cross-package call (e.g.
            // `Std.Toml.TomlValue.Parse`) aborts under `--mode jit` while
            // working under interp.
            let func_name = std::str::from_utf8(std::slice::from_raw_parts(fn_name_ptr, fn_name_len))
                .unwrap_or("<invalid>");
            return cross_zpkg_via_interp(
                frame_ref, ctx, dst, func_name, args_ptr, argc, caller_line, caller_col, caller_offset);
        }
    };

    // add-static-constructors：调用该类型的静态方法是 C# 的类型初始化触发点之一。
    // 与 interp 的 exec_call 屏障对称，共用 `ensure_callee_owner_init`。
    // 门在函数内短路（any_cctor_pending），故稳态下就是一次 relaxed load。
    // fix-crosspkg-static-call-cctor：必须在上面的**解析之后**——Tier 3 解析可能正是加载依赖包、
    // 登记其类型 cctor 的那一步；放在前面，首次跨包静态调用读到的门是 0，会跳过静态构造器。
    // align-jit-arity-cctor-order：只覆盖**本地 FnEntry** 这条去路；不可翻译回落
    // （`cross_zpkg_via_interp`）在它自己的签名判定**之后**过屏障。于是两个后端、所有去路的顺序统一为
    // 「解析 → 签名判定 → cctor 屏障 → 执行」（interp `exec_call::call` 同序）——签名对不上的调用本身非法，
    // 不应先触发类型初始化。此前屏障在分叉之前，回落路径的顺序是「屏障 → 判定」。
    {
        let vm = vm_ctx_ref(ctx);
        if vm.any_cctor_pending() {
            let name = std::str::from_utf8(
                std::slice::from_raw_parts(fn_name_ptr, fn_name_len)).unwrap_or("");
            if let Err(msg) = vm.ensure_callee_owner_init(name) {
                let module = &*(*ctx).module;
                let exc = crate::vm_context::cctor::make_type_init_exception(vm, module, &msg);
                set_exception(vm, exc);
                return 1;
            }
        }
    }

    // Fill the callee frame directly from the caller's registers — no
    // intermediate `Vec<Value>` alloc, args cloned once instead of twice.
    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let mut callee_frame = JitFrame::new_args_from(entry.max_reg, &frame_ref.regs, arg_regs);
    let jit_fn: JitFn = std::mem::transmute(entry.ptr);
    let vm_ctx = vm_ctx_ref(ctx);

    // jit-stack-trace + span-column-propagate: stamp caller's site pos + offset.
    vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);
    // 2026-05-10 unify-frame-chain: one push covering GC roots + trace.
    vm_ctx.push_frame(crate::exception::VmFrame::new(
        entry.name.clone(),
        entry.file.clone(),
        &callee_frame.regs as *const _,
        &callee_frame.env_arena as *const _,
    ));
    let result = jit_fn(&mut callee_frame, ctx);
    vm_ctx.pop_frame();
    if result != 0 { callee_frame.recycle(); return 1; }
    frame_ref.regs[dst as usize] = callee_frame.ret.take().unwrap_or(Value::Null);
    callee_frame.recycle();
    0
}

/// Direct-call fallback when the target has no JIT machine-code `FnEntry`.
/// Two cases land here, both mirroring `interp::exec_call::call`'s resolution
/// order:
///   1. the callee lives in the eagerly-merged main `module` but was not
///      JIT-compiled (so it's absent from `fn_entries`) — resolve it via
///      `module.func_index` and run it on the interpreter;
///   2. the callee lives in a dependency zpkg only reachable through the lazy
///      loader (`try_lookup_function`) — load + interp it.
/// Either way the callee runs interpreted and the result is spliced back into
/// the JIT caller's frame. Without case 1, a static cross-package call (e.g.
/// `Std.Toml.TomlValue.Parse`) aborts under `--mode jit` while working under
/// interp, because the lazy loader doesn't own already-merged functions.
unsafe fn cross_zpkg_via_interp(
    frame_ref: &mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, func_name: &str,
    args_ptr: *const u32, argc: usize,
    caller_line: u32, caller_col: u32, caller_offset: u32,
) -> u8 {
    let vm_ctx = vm_ctx_ref(ctx);
    let module = &*(*ctx).module;

    let arg_regs = std::slice::from_raw_parts(args_ptr, argc);
    let args: Vec<Value> = arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()).collect();
    // jit-stack-trace: stamp the caller's site + offset before descending.
    vm_ctx.update_top_frame_pos(caller_line, caller_col, caller_offset);

    // runtime-ambiguous-use-site：与 interp `exec_call` 对称（两后端同判据）。
    // 常态 = 一次 relaxed 原子读，进程内没发生过碰撞时恒 false。
    if let Some(exc) = crate::vm_context::symres::ambiguous_function_exception(
        vm_ctx, module, func_name,
    ) {
        set_exception(vm_ctx, exc);
        return 1;
    }
    // Case 1: function present in the merged main module (interp's hot path).
    // Case 2: cross-zpkg target reachable only through the lazy loader.
    // align-jit-arity-cctor-order：先把目标解析出来（Case 2 的查找可能正是加载依赖包的那一步），
    // 再判签名，最后过 cctor 屏障 —— 与 interp `exec_call::call` 同序。
    let mut lazy_holder: Option<std::sync::Arc<crate::metadata::Function>> = None;
    let callee: &crate::metadata::Function = if let Some(f) = module.func_index.get(func_name)
        .and_then(|&idx| module.functions.get(idx))
    {
        f
    } else if let Some(lazy_fn) = vm_ctx.try_lookup_function(func_name) {
        &**lazy_holder.insert(lazy_fn)
    } else {
        // fix-silent-symbol-resolution：与 interp 统一，抛类型化 MissingSymbolException。
        // 裸 Value::Str 只能被无类型 `catch {}` 捕获，匹配不上 `catch (Exception e)`。
        set_exception(vm_ctx, crate::exception::make_missing_symbol_exception(
            vm_ctx, module, format!("undefined function `{}`", func_name)));
        return 1;
    };
    // fix-call-arity-skew：与 interp `exec_call` 对称。
    if let Some(exc) = crate::vm_context::symres::wrong_arity_exception(
        vm_ctx, module, func_name, crate::vm_context::symres::call_arity(callee), argc,
    ) {
        set_exception(vm_ctx, exc);
        return 1;
    }
    // add-static-constructors：静态方法调用是类型初始化触发点（本回落路径的屏障，见 jit_call 注释）。
    if vm_ctx.any_cctor_pending() {
        if let Err(msg) = vm_ctx.ensure_callee_owner_init(func_name) {
            set_exception(vm_ctx, crate::vm_context::cctor::make_type_init_exception(vm_ctx, module, &msg));
            return 1;
        }
    }
    let outcome = crate::interp::exec_function(vm_ctx, module, callee, &args);

    match outcome {
        Ok(crate::interp::ExecOutcome::Returned(ret)) => {
            frame_ref.regs[dst as usize] = ret.unwrap_or(Value::Null);
            0
        }
        Ok(crate::interp::ExecOutcome::Thrown(val)) => { set_exception(vm_ctx, val); 1 }
        Err(e) => { set_exception(vm_ctx, Value::Str(e.to_string().into())); 1 }
    }
}

/// `jit_builtin` after `formalize-jit-method-token` (2026-05-08): receives
/// pre-resolved `BuiltinId` directly (not name pointers). Resolver
/// guarantees every `Instruction::Builtin.name` resolves at module load
/// (closed set; panic on miss), so JIT codegen embeds the id as an i32
/// constant in the generated machine code. Helper indexes
/// `BUILTINS[id]` directly — zero hash on every call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_builtin(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32,
    builtin_id: u32,
    name_ptr: *const u8, name_len: usize,
    args_ptr: *const u32, argc: usize,
) -> u8 {
    let frame_ref = &mut *frame;
    let arg_regs  = std::slice::from_raw_parts(args_ptr, argc);
    let args: Vec<Value> = arg_regs.iter().map(|&r| frame_ref.regs[r as usize].clone()).collect();

    let vm = vm_ctx_ref(ctx);
    // fix-jit-builtin-ext-fallback: `UNRESOLVED` means the resolver could not bind this
    // site's name to a static or ext builtin at resolve time (e.g. a native-ext facade
    // resolved before its lib was loaded in this VM). Resolve by name now, which re-checks
    // the per-VM ext registry — mirrors interp `exec_call::builtin`'s name fallback. The
    // hot path (resolved id) still dispatches by index with no hashing.
    let result = if builtin_id != crate::metadata::tokens::UNRESOLVED {
        crate::corelib::exec_builtin_by_id(vm, crate::metadata::tokens::BuiltinId(builtin_id), &args)
    } else {
        let name = std::str::from_utf8(std::slice::from_raw_parts(name_ptr, name_len)).unwrap_or("<invalid>");
        crate::corelib::exec_builtin(vm, name, &args)
    };
    match result {
        Ok(v)  => { frame_ref.regs[dst as usize] = v; 0 }
        Err(e) => {
            // A callback builtin (reflection `MethodInfo.Invoke`) that ran z42
            // code which threw stashed the ORIGINAL exception value — propagate
            // it with its real type, not wrapped into Std.Exception (parity with
            // interp `exec_call::builtin`).
            if let Some(thrown) = vm.take_pending_thrown() {
                set_exception(vm, thrown);
                return 1;
            }
            // make-corelib-errors-catchable parity (this path was interp-only;
            // jit_builtin previously set a raw `Value::Str`). Wrap the builtin
            // error in a `Std.Exception` so JIT-compiled code can catch it with
            // `catch (Exception e)` — a raw string never matches the catch type.
            // Falls back to the raw string if `Std.Exception` isn't loaded.
            let module = &*(*ctx).module;
            let exc = match crate::exception::make_stdlib_exception(
                vm, module, "Std.Exception", e.to_string(),
            ) {
                Ok(exc) => exc,
                Err(_)  => Value::Str(e.to_string().into()),
            };
            set_exception(vm, exc);
            1
        }
    }
}
