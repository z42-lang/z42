//! Heap inspection result contracts; algorithms live in the profiler component.
use crate::{
    gc::walk::{PausedHeap, RootKind},
    object::{ArrRef, ObjRef, Value},
    vm::VmContext,
};
/// Whether a heap referrer is a script object or an array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RetainerKind {
    Object = 0,
    Array = 1,
}

/// A direct heap referrer of the target (an object/array that holds a reference).
#[derive(Debug, Clone)]
pub struct RetainerInfo {
    pub kind: RetainerKind,
    /// FQ type name of the referrer (e.g. `"Demo.Holder"`, `"int[]"`).
    pub type_name: String,
    /// The referrer's heap identity (data ptr as usize) — dedup + stable id.
    pub id: usize,
}

/// A GC root that retains the target (category-level).
#[derive(Debug, Clone)]
pub struct RootInfo {
    pub kind: RootKind,
}

pub(crate) fn direct_referrers(
    ctx: &VmContext,
    target: &Value,
) -> anyhow::Result<Vec<RetainerInfo>> {
    let Some(ptr) = target_ptr(target) else {
        return Ok(Vec::new());
    };
    let backend = super::profiler(ctx)?;
    Ok(with_collected_heap(ctx, |heap| {
        backend.direct_referrers(heap, ptr)
    }))
}

pub(crate) fn retaining_roots(ctx: &VmContext, target: &Value) -> anyhow::Result<Vec<RootInfo>> {
    let Some(ptr) = target_ptr(target) else {
        return Ok(Vec::new());
    };
    let backend = super::profiler(ctx)?;
    Ok(with_collected_heap(ctx, |heap| {
        backend.retaining_roots(heap, ptr)
    }))
}

fn target_ptr(value: &Value) -> Option<usize> {
    match value {
        Value::Object(object) => Some(ObjRef::as_ptr(object) as usize),
        Value::Array(array) => Some(ArrRef::as_ptr(array) as usize),
        _ => None,
    }
}

fn with_collected_heap<T>(ctx: &VmContext, query: impl FnOnce(PausedHeap<'_>) -> T) -> T {
    // If another collector is active, the pause request parks us until it finishes.
    loop {
        if let Some(pause) = crate::threads::safepoint::request_gc_pause(ctx) {
            ctx.heap().collect_full();
            return query(pause.inspect_heap());
        }
    }
}
