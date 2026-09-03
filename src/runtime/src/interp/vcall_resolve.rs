//! Shared virtual-dispatch **target resolution** for the interpreter (`exec_vcall::vcall`)
//! and the JIT helper (`jit/helpers/vcall.rs::jit_vcall`).
//!
//! unify-vcall-resolution (2026-09-03): before this module the two engines each carried a
//! full copy of the receiver-kind ladder (boxed primitive → boxed struct → primitive-as-struct
//! → object vtable / hierarchy walk), the candidate-name generation and the PIC install rules —
//! ~900 lines that had to be kept in lock-step by hand (the JIT copy even had its own
//! `resolve_virtual` doing a linear scan over `module.functions`). Now *what* to call is
//! decided here exactly once; each engine only decides *how* to invoke the returned target
//! (interp frame vs. native `FnEntry`, with the engine-specific cold / cross-zpkg fallbacks).
//!
//! Resolution ladder (order is semantic — earlier rungs win):
//!   1. **Boxed primitive** (`BoxedStruct` whose payload is a scalar): `GetType` with no args is
//!      answered natively (the box keeps the precise wrapper class); anything else unboxes `this`
//!      to the scalar and resolves `{wrapper}.{m}` / `Std.Object.{m}` candidates.
//!   2. **Boxed struct**: `GetType` / `GetHashCode` / non-record `ToString` (no args) are native
//!      intercepts; otherwise resolve `{struct}.{m}` / `Std.Object.{m}` with `this = box`.
//!   3. **Primitive / array receiver** (`primitive_class_name`): resolve `{Std.Int32|…}.{m}` /
//!      `Std.Object.{m}` with `this = value`; a miss falls through to rung 4, which reports the
//!      non-object receiver.
//!   4. **Object**: `vtable_index` → `dispatch::resolve_virtual` (module classes) → lazy
//!      hierarchy walk through `ctx.try_lookup_type` (cross-zpkg bases).
//!
//! PIC install: whenever the resolved callee is module-local and the receiver has a type id
//! (real `TypeDesc.id` for objects, synthetic `PRIM_TYPE_*` for primitives; boxes have none),
//! the `(type_id, slot, fn_idx)` triple is written to the site's `VCallIC` so the next call
//! with that receiver type takes `vcall_ic_hit` and never reaches this module.

use std::sync::Arc;

use anyhow::{bail, Result};

use crate::metadata::resolver::{vcall_ic_install, vcall_ic_lookup, VCallIC};
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::{Function, Module, Value};
use crate::vm_context::VmContext;

use super::dispatch::resolve_virtual;
use super::exec_vcall::{primitive_class_name, value_synthetic_type_id};

/// What a virtual call resolved to. The engine decides how to run it.
pub(crate) enum VCallTarget {
    /// Result computed natively without invoking any z42 function (protocol intercepts on
    /// boxed receivers: `GetType` / `GetHashCode` / default struct `ToString`).
    Immediate(Value),
    /// `module.functions[idx]` of the entry module (the JIT may hold a compiled entry for it).
    Local(usize),
    /// A lazily-loaded / cross-zpkg function that is not in the entry module's table.
    Lazy(Arc<Function>),
}

pub(crate) struct ResolvedVCall {
    pub target: VCallTarget,
    /// The `this` the callee receives: the unboxed scalar for a boxed primitive, otherwise the
    /// receiver value itself.
    pub this: Value,
}

/// Receiver type id used to key the PIC: real `TypeDesc.id` for objects, synthetic id for
/// primitives / arrays; `None` for boxes and null (not cacheable).
#[inline]
pub(crate) fn receiver_type_id(obj_val: &Value) -> Option<u32> {
    match obj_val {
        Value::Object(rc) => Some(rc.type_desc().id.0),
        other => value_synthetic_type_id(other),
    }
}

/// PIC fast path shared by both engines: hit → module-local function index. `None` on miss,
/// on a non-cacheable receiver, or when the cached target is `UNRESOLVED`.
#[inline]
pub(crate) fn vcall_ic_hit(ic: Option<&VCallIC>, obj_val: &Value) -> Option<usize> {
    let ic = ic?;
    let recv_type = receiver_type_id(obj_val)?;
    let (_slot, fn_idx) = vcall_ic_lookup(ic, recv_type)?;
    if fn_idx == UNRESOLVED { return None; }
    Some(fn_idx as usize)
}

/// Slow path (PIC miss): walk the receiver-kind ladder and resolve the callee. `arity` is the
/// explicit argument count (excluding `this`). Installs the PIC entry when possible.
pub(crate) fn resolve_vcall(
    ctx: &VmContext, module: &Module, obj_val: &Value, method: &str, arity: usize,
    ic: Option<&VCallIC>,
) -> Result<ResolvedVCall> {
    // ── 1. boxed primitive (add-primitive-value-boxing → unify Phase 2 R3) ────────────────
    // `boxed_prim_i64` splits the scalar back out; methods run against the primitive struct
    // body with `this = scalar` (same source as the unboxed primitive). GetType must keep the
    // box: unboxing would report the scalar's *default* class (I64 → Int32) and lose the
    // precise width (Std.Int64 …), so it goes straight to `builtin_obj_get_type`.
    if let Value::BoxedStruct(gc) = obj_val {
        if let Some(scalar) = gc.borrow().boxed_prim_i64() {
            if method == "GetType" && arity == 0 {
                let ty = crate::corelib::object::builtin_obj_get_type(ctx, &[obj_val.clone()])?;
                return Ok(ResolvedVCall { target: VCallTarget::Immediate(ty), this: obj_val.clone() });
            }
            let class_name = gc.type_desc().name.clone();
            match resolve_by_candidates(ctx, module, &class_name, method, arity, true, None, ic) {
                Some(target) => return Ok(ResolvedVCall { target, this: Value::I64(scalar) }),
                None => bail!("VCall on boxed `{}`: method `{}` (arity {}) not found", class_name, method, arity),
            }
        }
    }

    // ── 2. boxed struct (add-struct-object-methods PR2b) ──────────────────────────────────
    // GetType → precise struct type; GetHashCode → native FNV; ToString → short type name
    // (C# ValueType default) except for records, whose compiler-synthesized `<Type>.ToString`
    // is reached through the candidate lookup; Equals / user methods → `{type}.{m}$arity`
    // synthesized/declared methods (`this = box`, the body's AsCast unboxes), fallback Std.Object.
    if let Value::BoxedStruct(b) = obj_val {
        if arity == 0 {
            if method == "GetType" {
                let ty = crate::corelib::object::builtin_obj_get_type(ctx, &[obj_val.clone()])?;
                return Ok(ResolvedVCall { target: VCallTarget::Immediate(ty), this: obj_val.clone() });
            }
            if method == "GetHashCode" {
                let h = crate::corelib::convert::builtin_struct_hash_code(ctx, &[obj_val.clone()])?;
                return Ok(ResolvedVCall { target: VCallTarget::Immediate(h), this: obj_val.clone() });
            }
            if method == "ToString" && !b.type_desc().is_record() {
                let n: &str = &b.type_desc().name;
                let short = n.rsplit('.').next().unwrap_or(n);
                return Ok(ResolvedVCall {
                    target: VCallTarget::Immediate(Value::Str(short.into())), this: obj_val.clone(),
                });
            }
        }
        let type_name = b.type_desc().name.to_string();
        match resolve_by_candidates(ctx, module, &type_name, method, arity, true, None, ic) {
            Some(target) => return Ok(ResolvedVCall { target, this: obj_val.clone() }),
            None => bail!("VCall on boxed struct `{}`: method `{}` (arity {}) not found", type_name, method, arity),
        }
    }

    // ── 3. primitive-as-struct (L3-G4b) ───────────────────────────────────────────────────
    // Primitives dispatch through their stdlib struct's method (`Value::I64.CompareTo` →
    // `Std.Int32.CompareTo`). The IR may carry the unmangled name when the static receiver
    // type is `object`, while IrGen emits overloads with a `$<arity>` suffix — both spellings
    // are probed. `Std.Object.{m}` is the fallback for protocol methods a primitive does not
    // override (notably `GetType`). A primitive that resolves nothing falls through to the
    // object rung, which reports the non-object receiver (unchanged diagnostics).
    if let Some(class_name) = primitive_class_name(obj_val) {
        let synth = value_synthetic_type_id(obj_val);
        if let Some(target) = resolve_by_candidates(ctx, module, class_name, method, arity, false, synth, ic) {
            return Ok(ResolvedVCall { target, this: obj_val.clone() });
        }
    }

    // ── 4. object: vtable → resolve_virtual → lazy hierarchy walk ─────────────────────────
    let type_desc = match obj_val {
        Value::Object(rc) => rc.type_desc_arc().clone(),
        other => bail!("VCall: expected object, got {:?}", other),
    };
    let recv_type = type_desc.id.0;

    // 4a. vtable_index (fastest; pre-built descriptor). fix-jit-vcall-overload-dispatch: this
    //     is consulted FIRST because it maps a (possibly overloaded) method name to the exact
    //     override slot the compiler bound the site to; `resolve_virtual`'s `Class.method`
    //     string walk would pick whichever same-named function it hits first.
    if let Some(&slot) = type_desc.vtable_index.get(method) {
        let n = type_desc.vtable[slot].1.as_str();
        if let Some(&idx) = module.func_index.get(n) {
            install_ic(ic, recv_type, slot as u32, idx);
            return Ok(ResolvedVCall { target: VCallTarget::Local(idx), this: obj_val.clone() });
        }
        if let Some(f) = ctx.try_lookup_function(n) {
            return Ok(ResolvedVCall { target: VCallTarget::Lazy(f), this: obj_val.clone() });
        }
    }
    // 4b. module class hierarchy (`<class>.<method>` at each level, intra-zpkg).
    if let Ok(f) = resolve_virtual(module, &type_desc.name, method) {
        if let Some(&idx) = module.func_index.get(f.name.as_str()) {
            install_ic(ic, recv_type, UNRESOLVED, idx);
            return Ok(ResolvedVCall { target: VCallTarget::Local(idx), this: obj_val.clone() });
        }
        if let Some(lazy) = ctx.try_lookup_function(&f.name) {
            return Ok(ResolvedVCall { target: VCallTarget::Lazy(lazy), this: obj_val.clone() });
        }
    }
    // 4c. lazy hierarchy walk: base chain via `module.classes` first, then the global type
    //     registry for cross-zpkg bases (e.g. `Stream` in z42.io, subclass in z42.net;
    //     `Std.Object.GetType` reached through a `Std.TestFailure` receiver).
    let mut cur = type_desc.name.clone();
    loop {
        let candidate = format!("{}.{}", cur, method);
        if let Some(&idx) = module.func_index.get(candidate.as_str()) {
            install_ic(ic, recv_type, UNRESOLVED, idx);
            return Ok(ResolvedVCall { target: VCallTarget::Local(idx), this: obj_val.clone() });
        }
        if let Some(lazy) = ctx.try_lookup_function(&candidate) {
            return Ok(ResolvedVCall { target: VCallTarget::Lazy(lazy), this: obj_val.clone() });
        }
        let next = module.classes.iter()
            .find(|c| c.name == cur)
            .and_then(|c| c.base_class.clone())
            .or_else(|| {
                if cur == type_desc.name {
                    type_desc.base_name.clone()
                } else {
                    ctx.try_lookup_type(&cur).and_then(|td| td.base_name.clone())
                }
            });
        match next {
            Some(b) => cur = b,
            None => break,
        }
    }
    bail!("VCall: function `{}.{}` not found", type_desc.name, method)
}

/// Probe the `{class}.{method}` candidate spellings for a non-object receiver, in order:
/// `{c}.{m}$arity` / `{c}.{m}` (or the reverse when `arity_first == false`, the primitive path's
/// historical order), then the bare canonical slot when the operand is a resolved full key
/// (`Name$arity$types`, stabilize-instance-dispatch-keys PR-1 — inert while operands carry no
/// `$`), then the same three against `Std.Object`. Module-local hits install the PIC when
/// `ic_key` (synthetic receiver id) is given; cross-zpkg hits resolve through the lazy loader.
fn resolve_by_candidates(
    ctx: &VmContext, module: &Module, class_name: &str, method: &str, arity: usize,
    arity_first: bool, ic_key: Option<u32>, ic: Option<&VCallIC>,
) -> Option<VCallTarget> {
    let bare = method.split_once('$').map(|(b, _)| b);
    let mut candidates: Vec<String> = Vec::with_capacity(6);
    let mut push_pair = |cls: &str| {
        let mangled = format!("{}.{}${}", cls, method, arity);
        let plain = format!("{}.{}", cls, method);
        if arity_first { candidates.push(mangled); candidates.push(plain); }
        else { candidates.push(plain); candidates.push(mangled); }
        if let Some(b) = bare { candidates.push(format!("{}.{}", cls, b)); }
    };
    push_pair(class_name);
    push_pair("Std.Object");
    for name in &candidates {
        if let Some(&idx) = module.func_index.get(name.as_str()) {
            if module.functions.get(idx).is_some() {
                if let Some(key) = ic_key { install_ic(ic, key, UNRESOLVED, idx); }
                return Some(VCallTarget::Local(idx));
            }
        }
        if let Some(f) = ctx.try_lookup_function(name) {
            return Some(VCallTarget::Lazy(f));
        }
    }
    None
}

#[inline]
fn install_ic(ic: Option<&VCallIC>, recv_type: u32, slot: u32, fn_idx: usize) {
    if let Some(ic) = ic {
        vcall_ic_install(ic, recv_type, slot, fn_idx as u32);
    }
}
