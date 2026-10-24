//! Heap retention diagnostic builtins — backing `Std.Diagnostics.Heap`
//! (add-heap-retention-diagnostics).
//!
//! `DirectReferrers(obj)` (L1) / `RetainingRoots(obj)` (L2) resolve the target's
//! heap identity and delegate to the GC's reverse-graph query (which forces a
//! full GC first for accuracy). Results are projected into z42 `Retainer` /
//! `RootRef` objects. All three builtins are pure queries from the caller's
//! viewpoint — the world stop is an implementation detail, no z42 code runs.

use crate::corelib::abi::BuiltinError;
use crate::gc::retention::RootKind;
use crate::metadata::types::NativeData;
use crate::metadata::Value;
use crate::vm_context::VmContext;

const STD_RETAINER: &str = "Std.Diagnostics.Retainer";
const STD_ROOTREF: &str = "Std.Diagnostics.RootRef";
/// expose-diagnostics-counters : return type of
/// `Std.Diagnostics.Runtime.Counters()` — 11 read-only auto-properties
/// (7 runtime counters + `allocations` + 3 generational GC fields).
const STD_RUNTIME_COUNTERS: &str = "Std.Diagnostics.RuntimeCounters";

/// The heap identity (data ptr as usize) of a target `Value`, or `None` for
/// non-heap values (primitive / null → no retention to report).
fn target_ptr(v: &Value) -> Option<usize> {
    match v {
        Value::Object(gc) => Some(crate::gc::ObjRef::as_ptr(gc) as usize),
        Value::Array(gc) => Some(crate::gc::ArrRef::as_ptr(gc) as usize),
        _ => None,
    }
}

/// Run a retention query with the world stopped. The query forces a collection and
/// then walks every thread's frames for roots, so the other mutators must be parked
/// first — the same `request_gc_pause` that `GC.ForceCollect()` uses. If another
/// collector is active, `request_gc_pause` parks us until it is done; retry.
fn with_world_stopped<R>(ctx: &VmContext, query: impl FnOnce() -> R) -> R {
    loop {
        if let Some(_pause) = crate::gc::safepoint::request_gc_pause(ctx) {
            return query();
        }
    }
}

/// Allocate a z42 class instance, writing named slots by `field_index`. A class the
/// caller's own package declares not being loaded is a loader / VM bug.
fn alloc_named(ctx: &VmContext, type_name: &str, named: &[(&str, Value)]) -> Result<Value, BuiltinError> {
    let td = ctx
        .try_lookup_type(type_name)
        .ok_or_else(|| BuiltinError::Internal(format!(
            "diagnostics: {type_name} not loaded (z42.core missing?)")))?;
    let mut slots = vec![Value::Null; td.fields.len()];
    for (k, v) in named {
        if let Some(&i) = td.field_index.get(*k) {
            slots[i] = v.clone();
        }
    }
    Ok(ctx.heap().alloc_object(td, slots, NativeData::None))
}

/// `Std.Diagnostics.Heap.DirectReferrers(object target)` (static) → `Retainer[]`.
pub fn heap_direct_referrers(ctx: &VmContext, target: &Value) -> Result<Value, BuiltinError> {
    let Some(ptr) = target_ptr(target) else {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    };
    let referrers = with_world_stopped(ctx, || ctx.heap().retention_direct_referrers(ptr));
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
pub fn heap_retaining_roots(ctx: &VmContext, target: &Value) -> Result<Value, BuiltinError> {
    let Some(ptr) = target_ptr(target) else {
        return Ok(ctx.heap().alloc_array(Vec::new()));
    };
    let roots = with_world_stopped(ctx, || ctx.heap().retention_roots(ptr));
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

/// `Std.Diagnostics.Runtime.Counters()` (static) → `RuntimeCounters`.
///
/// expose-diagnostics-counters (script-profiling P1c): surfaces the
/// runtime counter snapshot + heap-derived numbers to z42 scripts. Same two
/// sources `app.rs` merges into its `ProfileSnapshot` for `--print-stats-on-exit`
/// (`RuntimeCounters::snapshot` + `HeapStats`), so a script reading these fields
/// sees values consistent with the exit JSON (modulo the natural sampling-instant
/// skew). `allocations` reuses `HeapStats.allocations` (the SoT) — never
/// re-counted on the hot path.
pub fn diag_counters(ctx: &VmContext) -> Result<Value, BuiltinError> {
    let c = ctx.counters_snapshot();
    let h = ctx.heap().stats();
    alloc_named(
        ctx,
        STD_RUNTIME_COUNTERS,
        &[
            ("__prop_BuiltinCalls", Value::I64(c.builtin_calls as i64)),
            ("__prop_NativeCalls", Value::I64(c.native_calls as i64)),
            ("__prop_JitMethodsCompiled", Value::I64(c.jit_methods_compiled as i64)),
            ("__prop_JitCompileUsTotal", Value::I64(c.jit_compile_us_total as i64)),
            ("__prop_JitNativeFromInterp", Value::I64(c.jit_native_from_interp as i64)),
            ("__prop_ExceptionsThrown", Value::I64(c.exceptions_thrown as i64)),
            ("__prop_ExceptionsCaught", Value::I64(c.exceptions_caught as i64)),
            ("__prop_Allocations", Value::I64(h.allocations as i64)),
            ("__prop_MinorCollections", Value::I64(h.minor_collections as i64)),
            ("__prop_MajorCollections", Value::I64(h.major_collections as i64)),
            ("__prop_ReclaimedBytes", Value::I64(h.reclaimed_bytes as i64)),
        ],
    )
}

fn root_kind_ord(k: RootKind) -> i64 {
    match k {
        RootKind::StaticField => 0,
        RootKind::StackFrame => 1,
        RootKind::Pinned => 2,
    }
}

/// 堆滞留诊断 / 计数器投影迁 builtin ABI v2(P1-8 批次 12b)。两个 retention 查询
/// 停世界但不执行 z42 代码;非堆目标按 lenient 约定回空数组。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__heap_direct_referrers")]
    pub fn direct_referrers(ctx: &VmContext, target: &Value) -> Result<Value, BuiltinError> {
        heap_direct_referrers(ctx, target)
    }

    #[builtin(name = "__heap_retaining_roots")]
    pub fn retaining_roots(ctx: &VmContext, target: &Value) -> Result<Value, BuiltinError> {
        heap_retaining_roots(ctx, target)
    }

    #[builtin(name = "__diag_counters")]
    pub fn diag_counters(ctx: &VmContext) -> Result<Value, BuiltinError> {
        super::diag_counters(ctx)
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod diagnostics_tests;
