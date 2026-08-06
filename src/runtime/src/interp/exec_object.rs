/// Object instructions excluding VCall (which lives in `exec_vcall.rs` due
/// to its size). Covers: ObjNew (allocate + ctor), FieldGet / FieldSet,
/// IsInstance / AsCast (runtime type checks), StaticGet / StaticSet.

use crate::metadata::{Module, NativeData, ScriptObject, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::dispatch::{is_subclass_or_eq_td, make_fallback_type_desc};
use super::exec_vcall::is_array_isa;
use super::ops::collect_args;
use super::Frame;

/// `ObjNew` dispatch. Currently still goes through `module.type_registry`
/// (HashMap by name) since registry isn't a Vec-by-TypeId — the cache
/// `type_token` enables future fast-path + cross-zpkg observability:
/// when the slot starts as UNRESOLVED and the lazy loader resolves the
/// class, we write the resolved id back so subsequent diagnostics /
/// reflection see it.
///
/// Return `Ok(Some(val))` when the ctor `throw`s a user exception, so
/// the caller's `try`/`catch` can match — mirrors the `Call` / `Builtin`
/// propagation pattern in `exec_instr.rs`. `Ok(None)` = success.
/// `Err(...)` = internal anyhow error (separate from user exceptions).
///
/// fix-ctor-throw-propagation (2026-05-24): pre-fix, the ctor was
/// invoked via `exec_function(...)?;` and the `ExecOutcome::Thrown`
/// branch was silently dropped — `try { new C(badArg) } catch (X) {}`
/// could never match because the throw never propagated out of
/// `ObjNew`. The partially-constructed object was even written into
/// `dst`!
pub(super) fn obj_new(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, class_name: &str, ctor_name: &str, args: &[u32], type_args: &[String],
    type_token: Option<&std::sync::atomic::AtomicU32>,
    stack_alloc: bool,
) -> Result<Option<Value>> {
    use std::sync::atomic::Ordering;
    // L3-G4d: for imported classes (e.g. Std.Collections.Stack) the TypeDesc
    // may only exist in the lazy loader until first use; probe it before
    // falling back to a blank synthetic descriptor.
    let type_desc = module.type_registry
        .get(class_name)
        .cloned()
        .or_else(|| ctx.try_lookup_type(class_name))
        .unwrap_or_else(|| {
            std::sync::Arc::new(make_fallback_type_desc(module, class_name))
        });

    // Refresh the type_token cache if it was UNRESOLVED at load (cross-zpkg
    // lazy class). Not strictly needed for current dispatch (we still go
    // through type_registry lookup above) but gives forward observability
    // and prepares the slot for Phase X where ObjNew may use TypeId-keyed
    // caches.
    if let Some(slot) = type_token {
        if slot.load(Ordering::Relaxed) == crate::metadata::tokens::UNRESOLVED
            && type_desc.id.is_resolved()
        {
            slot.store(type_desc.id.0, Ordering::Relaxed);
        }
    }

    // 2026-05-02 fix-class-field-default-init: 按字段声明类型选默认值
    // （int → I64(0)、bool → Bool(false)、str/ref → Null …），不再
    // 一律 Null。有显式 init 的字段在 ctor 入口被 FieldSet 覆写。
    let slots: Vec<Value> = type_desc.fields.iter()
        .map(|f| crate::metadata::default_value_for(&f.type_tag))
        .collect();

    // add-escape-analysis-stack-alloc: when the compiler proved this `new` does
    // not escape its frame AND the ctor does not leak `this`, allocate in the
    // per-context stack arena (no GC region lock / tracking / sweep). The ctor
    // runs on the stack object exactly as on a heap one — `this` is a
    // `Value::StackObject { idx, frame_id }` handle that FieldGet/FieldSet resolve
    // through `ctx.stack_arena`, so the ctor's child frame reaches it fine.
    // `Z42_STACKALLOC=off` bypasses this at runtime (heap) for triage.
    let obj_val = if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let obj = ScriptObject {
            type_desc,
            slots: slots.into_boxed_slice(),
            native: NativeData::None,
            type_args: if type_args.is_empty() {
                Box::new([])
            } else {
                Box::<[String]>::from(type_args)
            },
        };
        let idx = ctx.stack_arena.lock().alloc_obj(frame.frame_id, obj);
        Value::StackObject { idx, frame_id: frame.frame_id }
    } else {
        let obj_val = ctx.heap().alloc_object(type_desc, slots, NativeData::None);

        // add-gc-oom-exception: alloc_object returns Null only under strict OOM.
        // Temporarily disable strict OOM so the exception object can be allocated;
        // since alloc_object only returns Null in strict mode, re-enable is safe.
        if matches!(obj_val, Value::Null) {
            ctx.heap().set_strict_oom(false);
            let exc = crate::exception::make_stdlib_exception(
                ctx, module, "Std.OutOfMemoryException",
                format!("cannot allocate `{class_name}`: heap limit exceeded"),
            ).unwrap_or(Value::Null);
            ctx.heap().set_strict_oom(true);
            return Ok(Some(exc));
        }

        // 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): populate
        // per-instance type_args from the IR instruction. Read by `DefaultOf`.
        if !type_args.is_empty() {
            if let Value::Object(ref rc) = obj_val {
                rc.borrow_mut().type_args = Box::<[String]>::from(type_args);
            }
        }
        obj_val
    };

    // 直查 ctor_name (TypeChecker 已 overload-resolve)；无名字推断。
    // L3-G4d: fall back to lazy loader when the ctor lives in a stdlib zpkg
    // (imported generic class ctor isn't in the main module's function table).
    let ctor_fn = module.func_index.get(ctor_name)
        .and_then(|&i| module.functions.get(i));
    let outcome = if let Some(ctor) = ctor_fn {
        let mut ctor_args = vec![obj_val.clone()];
        ctor_args.extend(collect_args(&frame.regs, args)?);
        Some(super::exec_function(ctx, module, ctor, &ctor_args)?)
    } else if let Some(lazy_ctor) = ctx.try_lookup_function(ctor_name) {
        let mut ctor_args = vec![obj_val.clone()];
        ctor_args.extend(collect_args(&frame.regs, args)?);
        Some(super::exec_function(ctx, module, lazy_ctor.as_ref(), &ctor_args)?)
    } else {
        None
    };

    // fix-ctor-throw-propagation (2026-05-24): if the ctor threw a user
    // exception, surface it via Ok(Some(val)) so the enclosing try/catch
    // can match. Do NOT write `obj_val` into `dst` — the object is
    // partially constructed and the caller is about to jump to a catch
    // handler that won't read it.
    if let Some(super::ExecOutcome::Thrown(val)) = outcome {
        return Ok(Some(val));
    }

    frame.set(dst, obj_val);
    Ok(None)
}

/// `FieldGet` dispatch with monomorphic inline cache. When `field_ic`
/// is provided and the receiver type matches the cached `TypeId`, the
/// field slot is fetched directly from `obj.slots[cached_slot]` (no hash).
/// On cache miss / first hit, walks `field_index` then writes back the
/// (TypeId, slot) pair so subsequent hits with the same receiver type
/// are fast. Polymorphic sites overwrite the slot each time (Phase 1
/// mono IC; Phase X may add poly).
///
/// Non-Object receivers (Str / Array / PinnedView) bypass the IC since
/// their field set is hardcoded (`Length` / `ptr` / `len`).
pub(super) fn field_get(
    ctx: &VmContext, frame: &mut Frame, dst: u32, obj: u32, field_name: &str,
    field_ic: Option<&crate::metadata::resolver::FieldIC>,
) -> Result<()> {
    use crate::metadata::resolver::{field_ic_lookup, field_ic_install};
    let val = match frame.get(obj)? {
        // add-escape-analysis-stack-alloc: stack object — resolve via the
        // per-context arena (validated: idx in range + frame_id matches, else a
        // clear stale-handle diagnostic).
        // add-stack-field-ic: reuse the same monomorphic FieldIC as the heap path.
        // Stack objects carry a resolved `type_desc.id`, and `field_index` is
        // per-type — so the cached `(TypeId → slot)` is identical whether the
        // receiver is heap or stack. Skipping the per-access `field_index` hashmap
        // lookup is the win: heavy cross-frame field access (object passed to a
        // callee that reads its fields many times) was dominated by that lookup,
        // which made escape/cross-proc stack-alloc lose to heap (heap had the IC).
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_obj(idx, frame_id, |obj| {
                if let Some(ic) = field_ic {
                    let recv_type = obj.type_desc.id.0;
                    if let Some(slot) = field_ic_lookup(ic, recv_type) {
                        return obj.slots.get(slot as usize).cloned().unwrap_or(Value::Null);
                    }
                    if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        field_ic_install(ic, recv_type, slot as u32);
                        return obj.slots.get(slot).cloned().unwrap_or(Value::Null);
                    }
                    return Value::Null;
                }
                obj.type_desc.field_index.get(field_name)
                    .and_then(|&slot| obj.slots.get(slot).cloned())
                    .unwrap_or(Value::Null)
            })?
        }
        Value::Object(rc) => {
            let borrowed = rc.borrow();
            // PIC fast path: 4-slot linear scan with UNRESOLVED early-exit.
            if let Some(ic) = field_ic {
                let recv_type = borrowed.type_desc.id.0;
                if let Some(slot) = field_ic_lookup(ic, recv_type) {
                    let v = borrowed.slots.get(slot as usize).cloned().unwrap_or(Value::Null);
                    drop(borrowed);
                    frame.set(dst, v);
                    return Ok(());
                }
                // Miss: walk field_index + install in PIC.
                if let Some(&slot) = borrowed.type_desc.field_index.get(field_name) {
                    field_ic_install(ic, recv_type, slot as u32);
                    borrowed.slots.get(slot).cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            } else if let Some(&slot) = borrowed.type_desc.field_index.get(field_name) {
                borrowed.slots.get(slot).cloned().unwrap_or(Value::Null)
            } else {
                Value::Null
            }
        }
        Value::Str(s) => match field_name {
            "Length"     => Value::I64(crate::corelib::str_meta::char_len(s) as i64),
            "ByteLength" => Value::I64(s.len() as i64),
            other        => bail!("string has no field `{}`", other),
        },
        Value::Array(rc) => match field_name {
            "Length" | "Count" => Value::I64(rc.borrow().len() as i64),
            other => bail!("array has no field `{}`", other),
        },
        // add-escape-analysis-stack-alloc: a stack array's `.Length`/`.Count`
        // routes through FieldGet (a neutral use in the escape rules), so it can
        // reach here — resolve the length via the arena.
        Value::StackArray { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            let len = ctx.stack_arena.lock().with_arr(idx, frame_id, |a| a.len())?;
            match field_name {
                "Length" | "Count" => Value::I64(len as i64),
                other => bail!("array has no field `{}`", other),
            }
        }
        Value::PinnedView(pv) => match field_name {
            // Spec C4 — only `ptr` / `len` are exposed; element type
            // information (kind) stays internal.
            "ptr" => Value::I64(pv.ptr as i64),
            "len" => Value::I64(pv.len as i64),
            other => bail!("PinnedView has no field `{}` (only `ptr` / `len`)", other),
        },
        other => bail!("FieldGet: not an object or known value type, got {:?}", other),
    };
    frame.set(dst, val);
    Ok(())
}

/// `FieldSet` dispatch — mirror of `field_get` IC pattern.
///
/// **add-write-barriers (2026-05-21)**: dispatches `write_barrier_field`
/// to the GC after each successful slot write *iff* the new value is a
/// heap reference (`v.is_heap_ref()`). Primitive writes skip the
/// dispatch (Decision 1 of the spec). Both IC fast and slow paths must
/// fire the barrier (Decision 5) — otherwise concurrent / generational
/// backends would miss writes on hot code.
pub(super) fn field_set(
    ctx: &VmContext, frame: &mut Frame, obj: u32, field_name: &str, val: u32,
    field_ic: Option<&crate::metadata::resolver::FieldIC>,
) -> Result<()> {
    use crate::metadata::resolver::{field_ic_lookup, field_ic_install};
    let v = frame.get(val)?.clone();
    // add-escape-analysis-stack-alloc (diagnostic #2): FieldSet.val is an escape
    // sink — the compiler must never let a stack handle be stored into a field
    // (it would outlive its frame). Assert the analysis kept that invariant.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into a field — escape analysis unsound (FieldSet.val)"
    );
    let owner = frame.get(obj)?.clone();
    match &owner {
        // add-escape-analysis-stack-alloc: stack object — write the slot in the
        // arena (validated). No GC write barrier: the stack object is not a heap
        // slot; its heap-ref fields are kept live by root-scanning the arena.
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_obj_mut(idx, frame_id, |obj| {
                // add-stack-field-ic: IC on the stack write path (same cache as heap;
                // no write barrier — stack slots aren't heap slots, heap-ref fields
                // are kept live by root-scanning the arena). Resolve the slot first
                // (releases the `field_index` borrow) before the mutable slot write.
                let slot_opt: Option<usize> = if let Some(ic) = field_ic {
                    let recv_type = obj.type_desc.id.0;
                    if let Some(slot) = field_ic_lookup(ic, recv_type) {
                        Some(slot as usize)
                    } else if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        field_ic_install(ic, recv_type, slot as u32);
                        Some(slot)
                    } else {
                        None
                    }
                } else {
                    obj.type_desc.field_index.get(field_name).copied()
                };
                if let Some(slot) = slot_opt {
                    if slot < obj.slots.len() {
                        obj.slots[slot] = v.clone();
                    }
                }
            })?;
            Ok(())
        }
        Value::Object(rc) => {
            let mut borrowed = rc.borrow_mut();
            // PIC fast path
            if let Some(ic) = field_ic {
                let recv_type = borrowed.type_desc.id.0;
                if let Some(slot) = field_ic_lookup(ic, recv_type) {
                    let slot = slot as usize;
                    if slot < borrowed.slots.len() {
                        borrowed.slots[slot] = v.clone();
                        drop(borrowed);
                        if v.is_heap_ref() {
                            ctx.heap().write_barrier_field(&owner, slot, &v);
                        }
                    }
                    return Ok(());
                }
                // Miss: walk + install in PIC
                let slot_opt = borrowed.type_desc.field_index.get(field_name).copied();
                if let Some(slot) = slot_opt {
                    field_ic_install(ic, recv_type, slot as u32);
                    if slot < borrowed.slots.len() {
                        borrowed.slots[slot] = v.clone();
                        drop(borrowed);
                        if v.is_heap_ref() {
                            ctx.heap().write_barrier_field(&owner, slot, &v);
                        }
                    }
                }
            } else if let Some(&slot) = borrowed.type_desc.field_index.get(field_name) {
                if slot < borrowed.slots.len() {
                    borrowed.slots[slot] = v.clone();
                    drop(borrowed);
                    if v.is_heap_ref() {
                        ctx.heap().write_barrier_field(&owner, slot, &v);
                    }
                }
            }
            Ok(())
        }
        other => bail!("FieldSet: expected object, got {:?}", other),
    }
}

/// fix-boxed-primitive-is-as: 基元值是否 is-a `class_name`。z42 不装箱基元，故 `object o = "hi"`
/// 里 o 仍是裸 `Value::Str` —— `is`/`as` 须按其 stdlib 类名（`primitive_class_name`，如
/// `Std.String`）匹配，外加 `Std.Object` 基类（所有基元 is-a object）。编译器 `QualifyTypeName`
/// 发 FQ 形（`Std.String`/`Std.Int32`/`Std.Object`），此处直接比 FQ。
pub(crate) fn prim_isa(val: &Value, class_name: &str) -> bool {
    // 所有基元 is-a object。
    if class_name == "Std.Object" || class_name == "Object" {
        return super::exec_vcall::primitive_class_name(val).is_some();
    }
    match val {
        // 整数宽度不可辨：z42 运行时用单一 Value::I64 表示 int/long/short/byte/…（boxed 后
        // 无宽度信息），故一个 boxed 整数 is-a **任意整数类型**——值的声明类型永不假阴，
        // 跨宽度松匹配是该表示的必然（`9L is long` / `(byte)7 is byte` 才不会误判 false）。
        Value::I64(_) => is_integer_class(class_name),
        // 非整数基元：精确匹配其 stdlib 类名（string/double/bool/char）。
        other => super::exec_vcall::primitive_class_name(other) == Some(class_name),
    }
}

/// 整数族类名（编译器 QualifyTypeName 发 FQ 形；带非限定形兜底）。
fn is_integer_class(cn: &str) -> bool {
    matches!(cn,
        "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte"
        | "Std.Byte" | "Std.UInt16" | "Std.UInt32" | "Std.UInt64"
        | "Int32" | "Int64" | "Int16" | "SByte"
        | "Byte" | "UInt16" | "UInt32" | "UInt64")
}

pub(super) fn is_instance(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
) -> Result<()> {
    let result = match frame.get(obj)? {
        Value::Object(rc) => {
            is_subclass_or_eq_td(ctx, &module.type_registry, &rc.type_desc().name, class_name)
        }
        // 2026-05-07 add-array-base-class: T[] is-a Std.Array is-a Std.Object.
        // VM hardcodes the chain since Value::Array doesn't carry a TypeDesc.
        Value::Array(_) => is_array_isa(class_name),
        Value::Null => false,
        // add-primitive-value-boxing: 装箱基元走**真 type_desc**（class + base + 接口链）——
        // 精确强类型：`9L`(Std.Int64) is long → true；`5`(Std.Int32) is long → false。
        // object 短路：所有装箱基元 is-a object（基元类名 Std.Int32 未必在 type_registry 可解析
        // 到 Std.Object 基链——其真名为 Std.Primitives.Int32——故显式判，镜像 prim_isa）。
        Value::Boxed(b) => {
            class_name == "Std.Object" || class_name == "Object"
                || is_subclass_or_eq_td(ctx, &module.type_registry, &b.class, class_name)
        }
        // fix-boxed-primitive-is-as: 未装箱裸基元（未经 object 边界）→ stdlib 类名松匹配兜底。
        other => prim_isa(other, class_name),
    };
    frame.set(dst, Value::Bool(result));
    Ok(())
}

pub(super) fn as_cast(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, obj: u32, class_name: &str,
) -> Result<()> {
    let val = frame.get(obj)?.clone();
    // add-primitive-value-boxing: Boxed 特判——命中**精确基元类** → 拆箱返 inner（`o as long`
    // 拿回裸 long）；命中 base/接口 → 保持 Boxed（多态用）；否则 Null。
    if let Value::Boxed(b) = &val {
        let is_obj = class_name == "Std.Object" || class_name == "Object";
        let out = if class_name == &*b.class {
            b.inner.clone()
        } else if is_obj || is_subclass_or_eq_td(ctx, &module.type_registry, &b.class, class_name) {
            val.clone()
        } else {
            Value::Null
        };
        frame.set(dst, out);
        return Ok(());
    }
    let is_match = match &val {
        Value::Object(rc) => {
            is_subclass_or_eq_td(ctx, &module.type_registry, &rc.type_desc().name, class_name)
        }
        Value::Array(_) => is_array_isa(class_name),
        Value::Null => true,
        // fix-boxed-primitive-is-as: 未装箱裸基元按其 stdlib 类名松匹配兜底。
        other => prim_isa(other, class_name),
    };
    frame.set(dst, if is_match { val } else { Value::Null });
    Ok(())
}

/// `StaticGet` hot path. Resolver populates `static_field_tokens[site_idx]`
/// with the lazy-allocated `StaticFieldId` at module load (always succeeds).
/// `field_id` Some → direct Vec index (no hash); None → name fallback.
pub(super) fn static_get(
    ctx: &VmContext, frame: &mut Frame, dst: u32, field: &str,
    field_id: Option<u32>,
) {
    let v = match field_id {
        Some(id) => ctx.static_get_by_id(crate::metadata::tokens::StaticFieldId(id)),
        None     => ctx.static_get(field),
    };
    frame.set(dst, v);
}

pub(super) fn static_set(
    ctx: &VmContext, frame: &Frame, field: &str, val: u32,
    field_id: Option<u32>,
) -> Result<()> {
    let v = frame.get(val)?.clone();
    // add-escape-analysis-stack-alloc (diagnostic #2): StaticSet.val is an escape
    // sink — a stack handle stored into a static would outlive its frame.
    debug_assert!(
        !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
        "stack-alloc handle stored into a static field — escape analysis unsound (StaticSet.val)"
    );
    match field_id {
        Some(id) => ctx.static_set_by_id(crate::metadata::tokens::StaticFieldId(id), v),
        None     => ctx.static_set(field, v),
    }
    Ok(())
}
