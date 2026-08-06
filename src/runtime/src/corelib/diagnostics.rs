//! Heap retention diagnostic builtins — backing `Std.Diagnostics.Heap`
//! (add-heap-retention-diagnostics).
//!
//! `DirectReferrers(obj)` (L1) / `RetainingRoots(obj)` (L2) resolve the target's
//! heap identity and delegate to the GC's reverse-graph query (which forces a
//! full GC first for accuracy). Results are projected into z42 `Retainer` /
//! `RootRef` objects.

use anyhow::Result;

use crate::gc::retention::RootKind;
use crate::metadata::types::NativeData;
use crate::metadata::Value;
use crate::vm_context::VmContext;

const STD_RETAINER: &str = "Std.Diagnostics.Retainer";
const STD_ROOTREF: &str = "Std.Diagnostics.RootRef";

/// The heap identity (data ptr as usize) of a target `Value`, or `None` for
/// non-heap values (primitive / null → no retention to report).
fn target_ptr(v: &Value) -> Option<usize> {
    match v {
        Value::Object(gc) => Some(gc.data_ptr_unlocked() as usize),
        Value::Array(gc) => Some(gc.data_ptr_unlocked() as usize),
        _ => None,
    }
}

/// Allocate a z42 class instance, writing named slots by `field_index`.
fn alloc_named(ctx: &VmContext, type_name: &str, named: &[(&str, Value)]) -> Result<Value> {
    let td = ctx
        .try_lookup_type(type_name)
        .ok_or_else(|| anyhow::anyhow!("diagnostics: {type_name} not loaded (z42.core missing?)"))?;
    let mut slots = vec![Value::Null; td.fields.len()];
    for (k, v) in named {
        if let Some(&i) = td.field_index.get(*k) {
            slots[i] = v.clone();
        }
    }
    Ok(ctx.heap().alloc_object(td, slots, NativeData::None))
}

/// `Std.Diagnostics.Heap.DirectReferrers(object target)` (static) → `Retainer[]`.
pub fn builtin_heap_direct_referrers(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let Some(ptr) = args.first().and_then(target_ptr) else {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    };
    let referrers = ctx.heap().retention_direct_referrers(ptr);
    let mut out = Vec::with_capacity(referrers.len());
    for r in referrers {
        out.push(alloc_named(
            ctx,
            STD_RETAINER,
            &[
                ("TypeName", Value::Str(r.type_name.into())),
                ("Id", Value::I64(r.id as i64)),
            ],
        )?);
    }
    Ok(ctx.heap().alloc_array(out))
}

/// `Std.Diagnostics.Heap.RetainingRoots(object target)` (static) → `RootRef[]`.
pub fn builtin_heap_retaining_roots(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let Some(ptr) = args.first().and_then(target_ptr) else {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    };
    let roots = ctx.heap().retention_roots(ptr);
    let mut out = Vec::with_capacity(roots.len());
    for r in roots {
        // `RootRef.Kind` is a z42 `RootKind` enum, i64-backed; the discriminant
        // matches the Rust `RootKind` repr (StaticField=0 … Pinned=3).
        out.push(alloc_named(
            ctx,
            STD_ROOTREF,
            &[("Kind", Value::I64(root_kind_ord(r.kind)))],
        )?);
    }
    Ok(ctx.heap().alloc_array(out))
}

fn root_kind_ord(k: RootKind) -> i64 {
    match k {
        RootKind::StaticField => 0,
        RootKind::StackFrame => 1,
        RootKind::FuncRefSlot => 2,
        RootKind::Pinned => 3,
    }
}
