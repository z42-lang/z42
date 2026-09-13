/// Object instructions excluding VCall (which lives in `exec_vcall.rs` due
/// to its size). Covers: ObjNew (allocate + ctor), FieldGet / FieldSet,
/// IsInstance / AsCast (runtime type checks), StaticGet / StaticSet.

use crate::metadata::{Module, NativeData, ScriptObject, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::dispatch::{isa_td, make_fallback_type_desc};
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
    // cache-ctorless-objnew: per-site mark; see `ResolvedTokens::ctorless_marks`.
    ctorless_mark: Option<&std::sync::atomic::AtomicUsize>,
    stack_alloc: bool,
) -> Result<Option<Value>> {
    use std::sync::atomic::Ordering;
    // L3-G4d: for imported classes (e.g. Std.Collections.Stack) the TypeDesc
    // may only exist in the lazy loader until first use; probe it before
    // falling back to a blank synthetic descriptor.
    let resolved = module.type_registry
        .get(class_name)
        .cloned()
        .or_else(|| ctx.try_lookup_type(class_name));
    let type_desc = match resolved {
        Some(td) => td,
        None => {
            // 站点 ② fix-silent-symbol-resolution：defer-class-initialization (2026-09-04)
            // 在这里放了一条 `tracing::warn!`——合成空描述符是**静默数据损坏**的温床（没有
            // 字段槽 ⇒ 构造器的 FieldSet 被丢弃、后续 FieldGet 全读 Null）。判据既然已经
            // 确定，就不该只是记一行日志：`missing_type_exception` 用同一判据改为抛异常，
            // 回落描述符只留给它的正当用途（合并模块带 ClassDesc / 编译器合成的本地类）。
            if let Some(exc) = crate::vm_context::symres::missing_type_exception(
                ctx, module, class_name,
            ) {
                return Ok(Some(exc));
            }
            std::sync::Arc::new(make_fallback_type_desc(module, class_name))
        }
    };
    // fix-crosspkg-base-fields-in-eager-module：急切加载的主模块注册表在**构建期**就把
    // 跨包基类合不进来（那时依赖还没加载），于是 `Derived : CrossPkgBase` 的这份描述符
    // 「只有自己的字段」。惰性加载器那份被 `try_fixup_inheritance` 补齐过，而
    // `Arc::make_mut` 的写时复制只让**惰性注册表**拿到修好的副本——主模块那份永远残缺。
    // ObjNew 又是先查主模块，于是继承字段整体丢失（实测 `Derived` 急切 1 槽 / 惰性 2 槽，
    // `d.A = 7` 被丢弃、`d.A` 读出 Null，而 VCall 因为另有基类回落路径**看起来是好的**，
    // 把这个洞盖了很久）。旗子在手，这里换成修好的那份。
    let type_desc = if type_desc.base_unmerged() {
        match ctx.try_lookup_type(class_name) {
            Some(fixed) if !fixed.base_unmerged() => fixed,
            _ => type_desc,
        }
    } else { type_desc };
    // 站点 ④ fix-silent-symbol-resolution：惰性解析都走完了还是残缺 ⇒ 基类确定不存在。
    // 继续走下去只会分配一个丢掉整片继承面的对象，错误现场离根因可以隔上任意远。
    // runtime-ambiguous-use-site：**用**一个被两个包各自声明的类型 → 报错（与上面
    // missing_type / missing_base 同族，都是派发点的符号完整性判定）。
    if let Some(exc) = crate::vm_context::symres::ambiguous_type_exception(ctx, module, class_name) {
        return Ok(Some(exc));
    }
    if let Some(exc) = crate::vm_context::symres::missing_base_exception(ctx, module, &type_desc) {
        return Ok(Some(exc));
    }

    // add-static-constructors：创建实例是 C# 的类型初始化触发点之一。此处 TypeDesc
    // 已在手 → 检查代价就是一次 `Option` 判断（没有 cctor 的类型的冷区多半是 None），
    // 不需要 `pending` 门。
    if let Err(msg) = ctx.ensure_type_init(&type_desc) {
        return Ok(Some(crate::vm_context::cctor::make_type_init_exception(ctx, module, &msg)));
    }

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

    // unify-object-byte-layout (PR-2): fields default to zero-initialized bytes +
    // `Null` refs (= int→0 / bool→false / '\0' / ref→Null, the old per-field
    // defaults), produced by `object_regions()`. Explicit initializers are written by
    // `FieldSet` at the ctor entry.

    // add-escape-analysis-stack-alloc: when the compiler proved this `new` does
    // not escape its frame AND the ctor does not leak `this`, allocate in the
    // per-context stack arena (no GC region lock / tracking / sweep). The ctor
    // runs on the stack object exactly as on a heap one — `this` is a
    // `Value::StackObject { idx, frame_id }` handle that FieldGet/FieldSet resolve
    // through `ctx.stack_arena`, so the ctor's child frame reaches it fine.
    // `Z42_STACKALLOC=off` bypasses this at runtime (heap) for triage.
    let obj_val = if stack_alloc && crate::interp::stack_alloc::stack_alloc_enabled() {
        let storage = type_desc.object_storage();
        let mut obj = ScriptObject::new(type_desc, storage);
        obj.set_type_args(if type_args.is_empty() {
            Box::new([])
        } else {
            Box::<[String]>::from(type_args)
        });
        let idx = ctx.stack_alloc_obj(frame.frame_id, obj);
        Value::StackObject { idx, frame_id: frame.frame_id }
    } else {
        let obj_val = ctx.heap().alloc_object(type_desc, Vec::new(), NativeData::None);

        // add-gc-oom-exception: alloc_object returns Null only under strict OOM.
        // make_oom_exception toggles strict OOM off while building the exception
        // object (which itself allocates) and restores it after.
        if matches!(obj_val, Value::Null) {
            return Ok(Some(crate::exception::make_oom_exception(
                ctx, module,
                format!("cannot allocate `{class_name}`: heap limit exceeded"),
            )));
        }

        // 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): populate
        // per-instance type_args from the IR instruction. Read by `DefaultOf`.
        if !type_args.is_empty() {
            if let Value::Object(ref rc) = obj_val {
                rc.borrow_mut().set_type_args(Box::<[String]>::from(type_args));
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
        // 站点 ⑤ fix-ctor-arity-skew：解析**成功**也要查——裸键在版本 skew 下会命中**错的**
        // 构造器（单构造器必是 primary、必占裸键），而 `exec_function` 不做 arity 校验。
        if let Some(exc) = crate::vm_context::symres::wrong_ctor_arity_exception(
            ctx, module, class_name, ctor_name,
            crate::vm_context::symres::ctor_arity(ctor), args.len(),
        ) {
            return Ok(Some(exc));
        }
        let mut ctor_args = vec![obj_val.clone()];
        ctor_args.extend(collect_args(&frame.regs, args)?);
        Some(super::exec_function(ctx, module, ctor, &ctor_args)?)
    } else if crate::metadata::resolver::ctorless_hit(ctorless_mark, ctx.fn_registration_mark()) {
        // cache-ctorless-objnew: the merged module was already probed above, and
        // this site proved `ctor_name` resolves nowhere in the loader either, with
        // nothing registered since — skip the lookup.
        None
    } else {
        let mark = ctx.fn_registration_mark();
        match ctx.try_lookup_function(ctor_name) {
            Some(lazy_ctor) => {
                // 站点 ⑤：惰性加载来的构造器同样查（跨包构造器正是 skew 的主战场）。
                if let Some(exc) = crate::vm_context::symres::wrong_ctor_arity_exception(
                    ctx, module, class_name, ctor_name,
                    crate::vm_context::symres::ctor_arity(lazy_ctor.as_ref()), args.len(),
                ) {
                    return Ok(Some(exc));
                }
                let mut ctor_args = vec![obj_val.clone()];
                ctor_args.extend(collect_args(&frame.regs, args)?);
                Some(super::exec_function(ctx, module, lazy_ctor.as_ref(), &ctor_args)?)
            }
            None => {
                // 站点 ③ fix-silent-symbol-resolution：带实参却解析不到构造器 = 定案缺失，
                // 不能照常把「未经构造」的对象写进 dst（字段全零值，错误现场离根因十万八千里）。
                if let Some(exc) = crate::vm_context::symres::missing_ctor_exception(
                    ctx, module, class_name, ctor_name, args.len(),
                ) {
                    return Ok(Some(exc));
                }
                crate::metadata::resolver::ctorless_note(ctorless_mark, mark);
                None
            }
        }
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
                        crate::metadata::resolver::assert_field_ic_slot(&obj.type_desc, field_name, slot);
                        return obj.field_value(slot as usize);
                    }
                    if let Some(&slot) = obj.type_desc.field_index.get(field_name) {
                        field_ic_install(ic, recv_type, slot as u32);
                        return obj.field_value(slot);
                    }
                    return Value::Null;
                }
                match obj.type_desc.field_index.get(field_name) {
                    Some(&slot) => obj.field_value(slot),
                    None => Value::Null,
                }
            })?
        }
        Value::Object(rc) => {
            let borrowed = rc.borrow();
            // PIC fast path: 4-slot linear scan with UNRESOLVED early-exit.
            if let Some(ic) = field_ic {
                let recv_type = borrowed.type_desc.id.0;
                if let Some(slot) = field_ic_lookup(ic, recv_type) {
                    crate::metadata::resolver::assert_field_ic_slot(&borrowed.type_desc, field_name, slot);
                    let v = borrowed.field_value(slot as usize);
                    drop(borrowed);
                    frame.set(dst, v);
                    return Ok(());
                }
                // Miss: walk field_index + install in PIC.
                if let Some(&slot) = borrowed.type_desc.field_index.get(field_name) {
                    field_ic_install(ic, recv_type, slot as u32);
                    borrowed.field_value(slot)
                } else {
                    Value::Null
                }
            } else if let Some(&slot) = borrowed.type_desc.field_index.get(field_name) {
                borrowed.field_value(slot)
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
        // make-value-copy: PinnedView is a transient-arena handle — resolve ptr/len via arena.
        Value::PinnedView { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            let (ptr, len) = ctx.transient_arena.lock().with(idx, frame_id, |p| match p {
                crate::interp::transient_arena::TransientPayload::PinView(pv) => (pv.ptr, pv.len),
                _ => (0u64, 0u64),
            })?;
            match field_name {
                // Spec C4 — only `ptr` / `len` are exposed; element type (kind) stays internal.
                "ptr" => Value::I64(ptr as i64),
                "len" => Value::I64(len as i64),
                other => bail!("PinnedView has no field `{}` (only `ptr` / `len`)", other),
            }
        }
        other => bail!("FieldGet: not an object or known value type, got {:?} (field `{}`)", other, field_name),
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
                        crate::metadata::resolver::assert_field_ic_slot(&obj.type_desc, field_name, slot);
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
                    // unify-object-byte-layout (PR-2): encode into bytes / refs. No
                    // write barrier — stack slots aren't heap slots (arena root-scanned).
                    obj.set_field_value(slot, &v);
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
                    crate::metadata::resolver::assert_field_ic_slot(&borrowed.type_desc, field_name, slot);
                    let slot = slot as usize;
                    // unify-object-byte-layout (PR-2): `set_field_value` returns whether
                    // a reference slot was written — fire the barrier only for a heap ref.
                    let wrote_ref = borrowed.set_field_value(slot, &v);
                    drop(borrowed);
                    if wrote_ref && v.is_heap_ref() {
                        ctx.heap().write_barrier_field(&owner, slot, &v);
                    }
                    return Ok(());
                }
                // Miss: walk + install in PIC
                let slot_opt = borrowed.type_desc.field_index.get(field_name).copied();
                if let Some(slot) = slot_opt {
                    field_ic_install(ic, recv_type, slot as u32);
                    let wrote_ref = borrowed.set_field_value(slot, &v);
                    drop(borrowed);
                    if wrote_ref && v.is_heap_ref() {
                        ctx.heap().write_barrier_field(&owner, slot, &v);
                    }
                }
            } else if let Some(&slot) = borrowed.type_desc.field_index.get(field_name) {
                let wrote_ref = borrowed.set_field_value(slot, &v);
                drop(borrowed);
                if wrote_ref && v.is_heap_ref() {
                    ctx.heap().write_barrier_field(&owner, slot, &v);
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

#[path = "exec_object_isa.rs"]
mod isa;
pub(super) use isa::{as_cast, is_instance};
use isa::is_integer_class;

/// `StaticGet` hot path. Resolver populates `static_field_tokens[site_idx]`
/// with the lazy-allocated `StaticFieldId` at module load (always succeeds).
/// `field_id` Some → direct Vec index (no hash); None → name fallback.
pub(super) fn static_get(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, field: &str,
    field_id: Option<u32>,
) -> Result<Option<Value>> {
    if let Some(exc) = ensure_owner_type_init(ctx, module, field) { return Ok(Some(exc)); }
    let v = match field_id {
        Some(id) => ctx.static_get_by_id(crate::metadata::tokens::StaticFieldId(id)),
        None     => ctx.static_get(field),
    };
    // fix-silent-symbol-resolution（站点 ①）：读到 Null 才确证该字段是否真的声明过。
    // 不能用「读到 Null 就报错」——Null 本身是合法值（未赋值的引用型静态字段就是 Null）。
    let mut v = v;
    if matches!(v, Value::Null) {
        use crate::vm_context::symres::StaticNullVerdict as V;
        match crate::vm_context::symres::verify_static_field(ctx, module, field) {
            V::Ok => {}
            V::Missing(exc) => return Ok(Some(exc)),
            // 惰性零初始化：回写槽位，后续读不再走这条确证路径。
            V::Default(d) => {
                match field_id {
                    Some(id) => ctx.static_set_by_id(
                        crate::metadata::tokens::StaticFieldId(id), d.clone()),
                    None => ctx.static_set(field, d.clone()),
                }
                v = d;
            }
        }
    }
    frame.set(dst, v);
    Ok(None)
}

/// add-static-constructors：静态字段读写前的 cctor 屏障。实现在
/// `VmContext::ensure_static_owner_init`，**与 JIT 侧共用同一份**（两后端语义一致
/// 是本特性最易错处，各写一份迟早漂移）。
/// 失败时返回**可 catch 的**类型化异常值（`Ok(Some(exc))` 是 interp 的 throw 通道）。
/// 用 `bail!` 会变成 anyhow Err —— 那条路不经 find_handler，用户 `catch` 抓不到。
fn ensure_owner_type_init(
    ctx: &VmContext, module: &Module, field: &str,
) -> Option<Value> {
    match ctx.ensure_static_owner_init(field) {
        Ok(()) => None,
        Err(msg) => Some(crate::vm_context::cctor::make_type_init_exception(ctx, module, &msg)),
    }
}

pub(super) fn static_set(
    ctx: &VmContext, module: &Module, frame: &Frame, field: &str, val: u32,
    field_id: Option<u32>,
) -> Result<Option<Value>> {
    if let Some(exc) = ensure_owner_type_init(ctx, module, field) { return Ok(Some(exc)); }
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
    Ok(None)
}
