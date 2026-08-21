/// Address-load instructions (spec impl-ref-out-in-runtime) and runtime
/// generic-default resolution.
///
/// Address-load: `LoadLocalAddr` / `LoadElemAddr` / `LoadFieldAddr` produce
/// `Value::Ref` pointing at the named location. Callers emit these for
/// `ref`/`out`/`in` arg expressions before the Call; the Ref flows through
/// `Call.args`; callee's `Frame::get_deref` / `set_thru_ref` transparently
/// follow it.
///
/// `DefaultOf` resolves `default(T)` for a generic type-parameter at runtime
/// (D-8b-3 Phase 2).

use crate::metadata::Value;
use crate::metadata::types::RefKind;
use crate::vm_context::VmContext;
use crate::interp::transient_arena::TransientPayload;
use anyhow::{bail, Result};

use super::ops::to_usize;
use super::Frame;

/// make-value-copy: allocate a `RefKind` into the per-context transient arena and
/// return the `Value::Ref { idx, frame_id }` handle (payload lives in the arena, so
/// `Value` stays `Copy`). LIFO-freed when the creating frame pops.
fn mk_ref(ctx: &VmContext, frame_id: u32, kind: RefKind) -> Value {
    let idx = ctx.transient_alloc(frame_id, TransientPayload::Ref(kind));
    Value::Ref { idx, frame_id }
}

pub(super) fn load_local_addr(ctx: &VmContext, frame: &mut Frame, dst: u32, slot: u32) {
    let depth = ctx.frame_stack_depth();
    // Current frame is the most recent push (depth - 1).
    let frame_idx = (depth.saturating_sub(1)) as u32;
    let r = mk_ref(ctx, frame.frame_id, RefKind::Stack { frame_idx, slot });
    frame.set(dst, r);
}

pub(super) fn load_elem_addr(ctx: &VmContext, frame: &mut Frame, dst: u32, arr: u32, idx: u32) -> Result<()> {
    let arr_val = frame.get(arr)?;
    let idx_val = to_usize(frame.get(idx)?, "LoadElemAddr index")?;
    match arr_val {
        Value::Array(rc) => {
            let r = mk_ref(ctx, frame.frame_id, RefKind::Array { gc_ref: *rc, idx: idx_val });
            frame.set(dst, r);
            Ok(())
        }
        other => bail!("LoadElemAddr: expected array, got {:?}", other),
    }
}

pub(super) fn load_field_addr(ctx: &VmContext, frame: &mut Frame, dst: u32, obj: u32, field_name: &str) -> Result<()> {
    let obj_val = frame.get(obj)?;
    match obj_val {
        Value::Object(rc) => {
            let r = mk_ref(ctx, frame.frame_id, RefKind::Field {
                gc_ref: *rc, field_name: field_name.to_string(),
            });
            frame.set(dst, r);
            Ok(())
        }
        other => bail!("LoadFieldAddr: expected object, got {:?}", other),
    }
}

/// 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): runtime
/// resolution of `default(T)` where T is a generic type-parameter of the
/// receiver class. Reads `frame.regs[0]` (this) → `Object → instance.type_args[idx]`,
/// looks up the resolved tag via `default_value_for(tag)`, writes result to dst.
/// Non-Object reg 0 / OOB index / empty type_args → graceful Null.
/// type_args is per-instance (populated by `obj.new`), not per-TypeDesc, so
/// `Foo<int>` and `Foo<string>` instances differ at runtime despite sharing
/// the same TypeDesc Arc (z42 erasure with per-instance type-arg view).
pub(super) fn default_of(frame: &mut Frame, dst: u32, param_index: u8) {
    let val = match frame.get(0) {
        Ok(Value::Object(rc)) => {
            let borrowed = rc.borrow();
            borrowed.type_args.get(param_index as usize)
                .map(|tag| crate::metadata::types::default_value_for(tag))
                .unwrap_or(Value::Null)
        }
        _ => Value::Null,
    };
    frame.set(dst, val);
}

/// add-generic-methods: materialize a **method-level** type parameter into a concrete
/// `Std.Type`, reading `frame.method_type_args[param_index]` (set at call time from
/// `CallInsn::method_type_args`). Feeds `typeof(T)` (result directly) and `new T()`
/// (`__activator_create` consumes this Type). OOB/empty → placeholder constructed type
/// named "T" (graceful, mirrors the class-level Typeof placeholder path).
pub(super) fn method_type_arg(ctx: &VmContext, frame: &mut Frame, dst: u32, param_index: u8) {
    let val = match frame.method_type_args.get(param_index as usize) {
        Some(name) => crate::corelib::reflection::make_type_from_name(ctx, name),
        None => crate::corelib::reflection::make_constructed_type(ctx, "T", &[]),
    };
    frame.set(dst, val);
}

/// add-generic-methods: method-level `default(T)` zero value — mirrors `default_of`
/// but reads `frame.method_type_args[param_index]` instead of the receiver's instance
/// type_args. OOB/empty → `Value::Null`.
pub(super) fn method_default(frame: &mut Frame, dst: u32, param_index: u8) {
    let val = frame.method_type_args.get(param_index as usize)
        .map(|tag| crate::metadata::types::default_value_for(tag))
        .unwrap_or(Value::Null);
    frame.set(dst, val);
}
