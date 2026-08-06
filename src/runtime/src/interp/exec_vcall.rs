/// Virtual dispatch (`VCall`) plus its dedicated helpers.
///
/// `VCall` is split out from `exec_object.rs` because it carries:
///   • The L3-G4b primitive-as-struct dispatch path (Value::I64 → `Std.Int32.<m>`)
///   • The `add-array-base-class` is-a hardcoded chain for `Value::Array`
///   • A 3-way fallback search (vtable_index → resolve_virtual → lazy hierarchy walk)
/// Together ~140 LOC; keeping it isolated makes the rest of `exec_object.rs`
/// fit comfortably under the file-size soft limit.

use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use super::dispatch::resolve_virtual;
use super::ops::collect_args;

/// runtime-jit-tiering Phase 1.5 (mixed-mode): receiver-aware analogue of
/// `exec_call::try_native_static_call`. If method `idx` is already JIT-compiled,
/// invoke it natively (receiver in reg 0 via `new_method_args_from`) and marshal the
/// result. `None` → not published / cold / untranslatable → caller stays interp.
#[cfg(feature = "jit")]
fn try_native_method_call(
    ctx: &VmContext, frame: &mut Frame, dst: u32, idx: usize,
    receiver: &Value, args: &[u32],
) -> Option<Result<Option<Value>>> {
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }
    // runtime-jit-tiering Phase 1.5 safety: never marshal a `Ref(Stack)` (out/ref
    // address, which only an INTERP frame can hold) into native code — see
    // `exec_call::try_native_static_call` for the full rationale. Guard receiver + args.
    if matches!(receiver, Value::Ref(_))
        || args.iter().any(|&r| matches!(frame.regs.get(r as usize), Some(Value::Ref(_)))) {
        return None;
    }
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    let (max_reg, ptr, name, file) = {
        let entry = unsafe { (*jit_ctx).resolve_fn_by_id_tiered(idx) }?;
        (entry.max_reg, entry.ptr, entry.name.clone(), entry.file.clone())
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut callee = crate::jit::frame::JitFrame::new_method_args_from(
        max_reg, receiver.clone(), &frame.regs, args);
    let jit_fn: crate::jit::helpers::JitFn = unsafe { std::mem::transmute(ptr) };
    ctx.push_frame(crate::exception::VmFrame::new(
        name, file, &callee.regs as *const _, &callee.env_arena as *const _));
    let r = unsafe { jit_fn(&mut callee, jit_ctx) };
    ctx.pop_frame();
    if r != 0 {
        callee.recycle();
        return Some(Ok(Some(ctx.take_exception().unwrap_or(Value::Null))));
    }
    let ret = callee.ret.take().unwrap_or(Value::Null);
    callee.recycle();
    frame.set(dst, ret);
    Some(Ok(None))
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_method_call(
    _ctx: &VmContext, _frame: &mut Frame, _dst: u32, _idx: usize,
    _receiver: &Value, _args: &[u32],
) -> Option<Result<Option<Value>>> {
    None
}
use super::{ExecOutcome, Frame};

/// L3-G4b primitive-as-struct: maps a primitive `Value` variant to its stdlib
/// struct's qualified class name (e.g. `Value::I64` → `"Std.Int32"`). The VM
/// dispatches primitive method calls by constructing `{class}.{method}` and
/// looking up the function in `module.func_index` — replacing the old
/// hardcoded `(Value, method)` → builtin-name switch.
///
/// Returns None for non-primitive values (objects, null, etc.).
pub(crate) fn primitive_class_name(obj: &Value) -> Option<&'static str> {
    use crate::metadata::well_known_names::*;
    match obj {
        // rename-primitives-to-pascal-case (2026-05-24): VM dispatch on
        // Value::I64 routes to Std.Int32 by default (narrow int / long values
        // are tagged with class FQN at compile-time in VCall instructions).
        Value::I64(_)  => Some(STD_INT32),
        Value::F64(_)  => Some(STD_DOUBLE),
        Value::Bool(_) => Some(STD_BOOLEAN),
        Value::Char(_) => Some(STD_CHAR),
        Value::Str(_)  => Some(STD_STRING),
        // 2026-05-07 add-array-base-class: T[] dispatches to Std.Array methods
        // (Clone / GetType / ToString / Equals / GetHashCode). The lookup path
        // below tries `Std.Array.<method>` first, then falls through to base
        // `Std.Object.<method>` via the existing primitive overload retry logic.
        Value::Array(_) => Some(STD_ARRAY),
        _ => None,
    }
}

/// refactor-vcall-ic-primitives (2026-05-17): synthetic TypeId for IC keying.
/// Primitives don't have a real `TypeDesc.id` (they're built-in runtime values,
/// not user-defined classes). Returning a stable `PRIM_TYPE_*` lets `VCallIC`
/// cache them with the same `cached_type_id` mechanism used for object
/// receivers — no extra slot, no separate cache path.
///
/// Returns None for objects (which use real `type_desc.id.0`) and `Value::Null`.
#[inline]
pub(crate) fn value_synthetic_type_id(obj: &Value) -> Option<u32> {
    use crate::metadata::tokens::*;
    match obj {
        Value::I64(_)   => Some(PRIM_TYPE_I64),
        Value::F64(_)   => Some(PRIM_TYPE_F64),
        Value::Bool(_)  => Some(PRIM_TYPE_BOOL),
        Value::Char(_)  => Some(PRIM_TYPE_CHAR),
        Value::Str(_)   => Some(PRIM_TYPE_STR),
        Value::Array(_) => Some(PRIM_TYPE_ARRAY),
        _ => None,
    }
}

/// 2026-05-07 add-array-base-class: hardcoded is-a check for `Value::Array`.
/// Class name comparison accepts both unqualified and `Std.`-qualified forms
/// because IR-emitted class names depend on TypeChecker's qualification path
/// (imported classes use FQ; bare references unqualified).
pub(super) fn is_array_isa(class_name: &str) -> bool {
    matches!(class_name, "Array" | "Object" | "Std.Array" | "Std.Object")
}

pub(super) fn vcall(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, obj: u32, method: &str, args: &[u32],
    // vcall_ic: monomorphic inline cache (TypeId, vtable slot, MethodId)
    // populated on first dispatch with this receiver type at this site.
    // Subsequent hits with the same receiver type take the fast path
    // (single u32 compare + direct module.functions index).
    vcall_ic: Option<&crate::metadata::resolver::VCallIC>,
) -> Result<Option<Value>> {
    let obj_val = frame.get(obj)?.clone();
    // perf-vm-iteration Phase 1 (Decision 3): `collect_args` is no longer
    // unconditional — the object/primitive IC fast path below fills the callee
    // frame directly from caller regs + arg indices (zero args Vec). The cold
    // boxing / primitive-name / vtable paths materialize `extra_args` locally.

    // ── add-primitive-value-boxing: 装箱基元方法调用 ────────────────────
    // 按 boxed.class 解析方法（+ arity 重载），fallback Std.Object 基类；`this = inner`（拆箱后
    // 交基元 struct 方法体，与未装箱基元同源）。GetType/ToString/Equals/GetHashCode 皆经此。
    if let Value::Boxed(b) = &obj_val {
        let mut extra_args = collect_args(&frame.regs, args)?;
        // GetType 须保留装箱类：一般方法拆箱 `this=inner` 交基元 struct 方法体，但 GetType
        // 若也拆箱，其 __get_type 会按 inner 的**默认** primitive_class_name 报告（I64→Int32），
        // 丢掉 Std.Int64 等精确宽度。故 GetType 直接交 builtin_obj_get_type（Boxed 臂用 b.class）。
        if method == "GetType" && extra_args.is_empty() {
            let ty = crate::corelib::object::builtin_obj_get_type(ctx, &[obj_val.clone()])?;
            frame.set(dst, ty);
            return Ok(None);
        }
        let class_name = b.class.clone();
        let mut call_args = vec![b.inner.clone()];
        call_args.append(&mut extra_args);
        let arity = call_args.len() - 1;
        let candidates = [
            format!("{}.{}${}", class_name, method, arity),
            format!("{}.{}", class_name, method),
            format!("Std.Object.{}${}", method, arity),
            format!("Std.Object.{}", method),
        ];
        for func_name in &candidates {
            let callee = module.func_index.get(func_name.as_str())
                .and_then(|&idx| module.functions.get(idx));
            if let Some(callee) = callee {
                return match super::exec_function(ctx, module, callee, &call_args)? {
                    ExecOutcome::Returned(ret) => { frame.set(dst, ret.unwrap_or(Value::Null)); Ok(None) }
                    ExecOutcome::Thrown(v) => Ok(Some(v)),
                };
            }
            if let Some(lazy_fn) = ctx.try_lookup_function(func_name) {
                return match super::exec_function(ctx, module, lazy_fn.as_ref(), &call_args)? {
                    ExecOutcome::Returned(ret) => { frame.set(dst, ret.unwrap_or(Value::Null)); Ok(None) }
                    ExecOutcome::Thrown(v) => Ok(Some(v)),
                };
            }
        }
        bail!("VCall on boxed `{}`: method `{}` (arity {}) not found", class_name, method, arity);
    }

    // ── Fast path: IC hit (object OR primitive receiver) ────────────────
    // Fires when (1) caller passed an IC, (2) receiver's TypeId — real for
    // Value::Object, synthetic `PRIM_TYPE_*` for primitives (refactor-vcall-
    // ic-primitives, 2026-05-17) — matches cache, (3) cached MethodId
    // resolves to a module function. Anything else falls through to the slow
    // path, which also updates the IC for next time.
    if let Some(ic) = vcall_ic {
        let recv_type = match &obj_val {
            Value::Object(rc) => rc.type_desc().id.0,
            other => value_synthetic_type_id(other)
                .unwrap_or(crate::metadata::tokens::UNRESOLVED),
        };
        // PIC fast path: 4-slot linear scan
        if let Some((_slot, fn_idx)) =
            crate::metadata::resolver::vcall_ic_lookup(ic, recv_type)
        {
            if fn_idx != crate::metadata::tokens::UNRESOLVED {
                // runtime-jit-tiering Phase 1.5: route an already-compiled method to
                // native (receiver-aware). No-op when cold/untranslatable/interp-only.
                if let Some(res) = try_native_method_call(
                    ctx, frame, dst, fn_idx as usize, &obj_val, args)
                {
                    return res;
                }
                if let Some(callee) = module.functions.get(fn_idx as usize) {
                    // Direct fill: regs[0]=receiver, regs[1+i]=caller args. No
                    // vec![receiver] / collect_args allocation on the hot path.
                    let outcome = super::exec_function_from_receiver_regs(
                        ctx, module, callee, &obj_val, &frame.regs, args)?;
                    return match outcome {
                        ExecOutcome::Returned(ret) => {
                            frame.set(dst, ret.unwrap_or(Value::Null));
                            Ok(None)
                        }
                        ExecOutcome::Thrown(val) => Ok(Some(val)),
                    };
                }
            }
        }
    }

    // L3-G4b primitive-as-struct: primitives dispatch through their stdlib
    // struct's method (e.g. `Value::I64.CompareTo` → call `Std.Int32.CompareTo`
    // IR function, which contains a BuiltinInstr for `__int32_compare_to`).
    // This replaces the old hardcoded `(Value, method) → builtin` table —
    // method-to-native binding is now entirely data-driven via stdlib source.
    //
    // Overload resolution: when the receiver type is statically `object`
    // (e.g. `Std.Assert.Equal(object, object)` calling `expected.Equals(actual)`),
    // the C# emit can't pick an overload at compile time; the IR carries the
    // unmangled method name `Equals`. But IrGen emits overloaded methods with
    // a `$N` arity suffix (e.g. `Std.String.Equals$1`). We retry with `$<arity>`
    // when the unmangled lookup misses — covers `Equals` (arity 1) and any
    // other overloaded primitive method without per-Value-type special cases.
    // This subsumes the legacy `Value::Str` hardcoded block (review2 §2.2).
    // avoid-vcall-vtable-arg-vec: only the primitive/array path needs a
    // materialized args `Vec` (it hands a `&[Value]` to `exec_function`). Object
    // receivers reach the vtable path below, which dispatches through the pooled,
    // Vec-free `exec_function_from_receiver_regs` (mirrors the IC fast path + the
    // JIT `jit_vcall`). So `collect_args` is deferred into this block instead of
    // allocating an args Vec for every object vcall that misses the IC —
    // megamorphic sites (z42c's visitor dispatch) were the top interp alloc
    // source after the lazy-lookup clone fix.
    if let Some(class_name) = primitive_class_name(&obj_val) {
        let extra_args = collect_args(&frame.regs, args)?;
        let mut call_args = Vec::with_capacity(extra_args.len() + 1);
        call_args.push(obj_val.clone());
        call_args.extend(extra_args);
        let arity = call_args.len() - 1; // exclude `this`
        let primary = format!("{}.{}", class_name, method);
        let overload = format!("{}.{}${}", class_name, method, arity);
        // Std.Object 兜底：基元未 override 的协议方法（尤 GetType——仅定义在 Std.Object
        // 的 [Native("__obj_get_type")]，String/Int32 等不 override）走基类实现，`this=基元`。
        // 无此兜底则 `object s="hi"; s.GetType()` 落到下方 vtable 路径 bail（expected object）。
        // 类专属候选优先，故 override 仍解析到子类实现，兜底只补「仅 Std.Object 有」的方法。
        let obj_primary = format!("Std.Object.{}", method);
        let obj_overload = format!("Std.Object.{}${}", method, arity);
        // refactor-vcall-ic-primitives (2026-05-17): on intra-module resolve,
        // populate VCallIC so the next call at this site with the same primitive
        // receiver type takes the IC fast path above — skips both format!()
        // calls + the HashMap lookup. Cross-zpkg (lazy_fn) skips populate
        // because IC cached_fn_idx must index into THIS module's functions table.
        for func_name in [primary.as_str(), overload.as_str(),
                          obj_primary.as_str(), obj_overload.as_str()] {
            if let Some(&idx) = module.func_index.get(func_name) {
                if let Some(callee) = module.functions.get(idx) {
                    if let (Some(ic), Some(synth_id)) =
                        (vcall_ic, value_synthetic_type_id(&obj_val))
                    {
                        // PIC install — slot is UNRESOLVED for primitives
                        // (they don't have vtables; we go direct to fn_idx).
                        crate::metadata::resolver::vcall_ic_install(
                            ic, synth_id,
                            crate::metadata::tokens::UNRESOLVED,
                            idx as u32,
                        );
                    }
                    let outcome = super::exec_function(ctx, module, callee, &call_args)?;
                    return match outcome {
                        ExecOutcome::Returned(ret) => {
                            frame.set(dst, ret.unwrap_or(Value::Null));
                            Ok(None)
                        }
                        ExecOutcome::Thrown(val) => Ok(Some(val)),
                    };
                }
            }
            if let Some(lazy_fn) = ctx.try_lookup_function(func_name) {
                let outcome = super::exec_function(ctx, module, lazy_fn.as_ref(), &call_args)?;
                return match outcome {
                    ExecOutcome::Returned(ret) => {
                        frame.set(dst, ret.unwrap_or(Value::Null));
                        Ok(None)
                    }
                    ExecOutcome::Thrown(val) => Ok(Some(val)),
                };
            }
        }
        // A primitive/array that resolved none of its candidates falls through
        // to the vtable path below, which bails on the non-object receiver — no
        // args carry-over needed.
    }

    // O(1) vtable dispatch using pre-computed TypeDesc.
    let type_desc = match &obj_val {
        Value::Object(rc) => rc.type_desc_arc().clone(),
        other => bail!("VCall: expected object, got {:?}", other),
    };
    // Try paths in order:
    //   1. vtable_index hit (fastest path; pre-built type descriptor)
    //   2. resolve_virtual: walk module.classes hierarchy looking up
    //      `<class>.<method>` in module.func_index at each level
    //   3. (NEW 2026-05-05) lazy hierarchy walk: same hierarchy traversal
    //      but using ctx.try_lookup_function — covers methods inherited
    //      from cross-zpkg base classes (e.g. `e.GetType()` when
    //      `e: Std.TestFailure` and `GetType` is on Std.Object in z42.core)
    //   4. fallback: `<most-derived>.<method>` (likely fails downstream)
    // Object receiver + arg reg indices dispatch through the pooled, Vec-free
    // frame builder at the invocation below — no `call_args` Vec materialized.
    let mut callee_module_idx: Option<usize> = None;
    let mut callee_lazy: Option<Arc<crate::metadata::Function>> = None;
    let mut chosen_name: Option<String> = None;

    if let Some(&slot) = type_desc.vtable_index.get(method) {
        let n = type_desc.vtable[slot].1.clone();
        if let Some(&idx) = module.func_index.get(n.as_str()) {
            callee_module_idx = Some(idx);
            // Populate IC for next time this site sees the same receiver type.
            // Only cache when receiver's TypeId is resolved (not for fallback
            // synthetic descriptors where id == UNRESOLVED).
            if let Some(ic) = vcall_ic {
                let recv_type = type_desc.id.0;
                crate::metadata::resolver::vcall_ic_install(
                    ic, recv_type, slot as u32, idx as u32,
                );
            }
        } else if let Some(fn_) = ctx.try_lookup_function(&n) {
            callee_lazy = Some(fn_);
        }
        chosen_name = Some(n);
    }
    if callee_module_idx.is_none() && callee_lazy.is_none() {
        if let Ok(f) = resolve_virtual(module, &type_desc.name, method) {
            let n = f.name.clone();
            if let Some(&idx) = module.func_index.get(n.as_str()) {
                callee_module_idx = Some(idx);
            } else if let Some(fn_) = ctx.try_lookup_function(&n) {
                callee_lazy = Some(fn_);
            }
            chosen_name = Some(n);
        }
    }
    // Lazy hierarchy walk: walk type_desc's base chain via
    // module.classes, trying ctx.try_lookup_function at each level.
    // Critical for cross-zpkg inherited methods (Std.Object.GetType
    // accessed via Std.TestFailure receiver).
    if callee_module_idx.is_none() && callee_lazy.is_none() {
        let mut cur = type_desc.name.clone();
        loop {
            let candidate = format!("{}.{}", cur, method);
            if let Some(&idx) = module.func_index.get(candidate.as_str()) {
                callee_module_idx = Some(idx);
                chosen_name = Some(candidate);
                break;
            }
            if let Some(fn_) = ctx.try_lookup_function(&candidate) {
                callee_lazy = Some(fn_);
                chosen_name = Some(candidate);
                break;
            }
            // Walk base via module.classes first (intra-zpkg), then fall
            // back to ctx registry (cross-zpkg base not in module.classes).
            // This fixes cross-zpkg virtual dispatch where the base class
            // lives in a different zpkg (e.g. Stream in z42.io, subclass
            // in z42.net): after the first level we can no longer rely on
            // module.classes; ctx.try_lookup_type covers deeper levels.
            let next = module.classes.iter()
                .find(|c| c.name == cur)
                .and_then(|c| c.base_class.clone())
                .or_else(|| {
                    if cur == type_desc.name {
                        type_desc.base_name.clone()
                    } else {
                        // Cross-zpkg base: look up from global type registry.
                        ctx.try_lookup_type(&cur).and_then(|td| td.base_name.clone())
                    }
                });
            match next {
                Some(b) => cur = b,
                None => break,
            }
        }
    }

    let func_name = chosen_name.unwrap_or_else(|| format!("{}.{}", type_desc.name, method));
    let outcome = if let Some(idx) = callee_module_idx {
        let callee = &module.functions[idx];
        super::exec_function_from_receiver_regs(ctx, module, callee, &obj_val, &frame.regs, args)?
    } else if let Some(lazy_fn) = callee_lazy {
        super::exec_function_from_receiver_regs(ctx, module, lazy_fn.as_ref(), &obj_val, &frame.regs, args)?
    } else {
        bail!("VCall: function `{}` not found", func_name);
    };
    match outcome {
        ExecOutcome::Returned(ret) => {
            frame.set(dst, ret.unwrap_or(Value::Null));
            Ok(None)
        }
        ExecOutcome::Thrown(val) => Ok(Some(val)),
    }
}
