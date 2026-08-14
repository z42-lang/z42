//! GC control builtins exposed to z42 scripts.
//!
//! Wired to `Std.GC.*` static class declared in `src/libraries/z42.core/src/GC.z42`.
//! Each method has `[Native("__gc_*")]` attribute mapping to the dispatch entries
//! in `super::dispatch_table()`.
//!
//! 2026-04-29 expose-gc-to-scripts (Phase 3d.2): 让脚本能显式触发 cycle
//! collection 与查询堆使用量，用于端到端验证 GC 与 host 集成应用。

use crate::gc::GcHandleKind;
use crate::metadata::{FieldSlot, NativeData, TypeDesc, Value};
use crate::vm_context::VmContext;
use anyhow::{anyhow, Result};
use std::io::Write as _;
use std::sync::{Arc, OnceLock};

/// `Std.GC.Collect()` —— 触发环检测（不阻断；no-op 当 paused）。
/// 调用 `ctx.heap().collect_cycles()`，进而走 `ArcMagrGC` 的 trial-deletion 算法。
///
/// **add-gc-safepoint (2026-05-20)**: stop-the-world via
/// [`crate::gc::safepoint::request_gc_pause`] before mark+sweep. All other
/// VmContexts park at their next safepoint check; this thread runs collect;
/// the RAII guard's Drop releases everyone.
///
/// **add-multi-collector-arbitration (2026-05-21)**: best-effort —
/// silently no-op if another collector is already active. The active
/// collector's pending work covers our intent; calling again redundantly
/// would just run an immediate-no-op collect with nothing reachable to
/// reclaim. Matches typical `GC.Collect()` semantics in C# / Java where
/// concurrent calls may coalesce.
pub fn builtin_gc_collect(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    // add-concurrent-gc P4b (2026-05-22): dispatch via
    // collect_cycles_with_context so the heap can choose STW or concurrent
    // path based on its current GcMode. STW mode (default) keeps the
    // pre-this-spec behavior exactly; ConcurrentMarkSweep runs the
    // multi-phase flow internally.
    ctx.heap().collect_cycles_with_context(ctx);
    Ok(Value::Null)
}

/// `Std.GC.UsedBytes()` —— 返回当前 `HeapStats.used_bytes`（i64）。
/// RC 模式 + cycle collector：alloc 累加；cycle collect 后按 freed_bytes 减去。
pub fn builtin_gc_used_bytes(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    Ok(Value::I64(ctx.heap().used_bytes() as i64))
}

/// `Std.GC.ForceCollect()` —— 强制完整 collect，返回 `freed_bytes`（i64）。
/// 与 `Collect()` 区别：前者是建议性（pause 时跳过），后者总是触发并返回数据。
///
/// **add-gc-safepoint (2026-05-20)**: same stop-the-world wrapper as
/// `builtin_gc_collect`.
///
/// **add-multi-collector-arbitration (2026-05-21)**: if another collector
/// is already active, we skip our own force_collect and return 0 freed
/// bytes (the active collector's work covers our intent). Same
/// best-effort semantics as `builtin_gc_collect`.
pub fn builtin_gc_force_collect(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    if let Some(_pause) = crate::gc::safepoint::request_gc_pause(ctx) {
        let stats = ctx.heap().force_collect();
        Ok(Value::I64(stats.freed_bytes as i64))
    } else {
        Ok(Value::I64(0))
    }
}

/// `Std.GC.Finalize(target)` — **add-custom-allocator P2 (2026-05-22)**.
///
/// Fires the registered finalizer on `target` immediately (one-shot)
/// and tombstones the region slot. Use this when RAII-style prompt
/// resource release matters (file handles, network sockets, native
/// FFI handles). For most objects (no finalizer registered), this is
/// a no-op return.
///
/// Returns `true` if a finalizer was actually fired; `false` if
/// `target` had no finalizer registered, was not a heap reference,
/// or was already tombstoned. Null target → false.
///
/// **Contract**: after this call, any other strong reference to the
/// same object panics on borrow (generation mismatch detection,
/// per add-custom-allocator design D5). Weak refs return None on
/// upgrade. Slot becomes reusable on next alloc.
pub fn builtin_gc_finalize(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let target = args.first().unwrap_or(&Value::Null);
    let fired = ctx.heap().finalize_now(target);
    Ok(Value::Bool(fired))
}

// ── reorganize-gc-stdlib (2026-05-07) ────────────────────────────────────────
//
// `Std.GCHandle` (struct, single `_slot: long`) and `Std.HeapStats` (class,
// 7 long fields) builtins. The handle slot itself lives in the GC layer; the
// builtins below are thin shims that project Rust enums / structs into z42
// `Value::Object`s with the corresponding TypeDesc. See:
// - `src/libraries/z42.core/src/GC/GCHandle.z42`
// - `src/libraries/z42.core/src/GC/HeapStats.z42`
// - `src/runtime/src/gc/heap.rs` HandleTable trait API

/// `Std.GCHandleType.Weak` enum value (corresponds to z42 `enum` first variant
/// being numbered 0). `GCHandleType.Strong` is 1.
const GC_HANDLE_TYPE_WEAK: i64   = 0;
const GC_HANDLE_TYPE_STRONG: i64 = 1;

fn gc_handle_kind_from_i64(v: i64) -> GcHandleKind {
    match v {
        GC_HANDLE_TYPE_WEAK => GcHandleKind::Weak,
        _                   => GcHandleKind::Strong,
    }
}

fn gc_handle_kind_to_i64(k: GcHandleKind) -> i64 {
    match k {
        GcHandleKind::Weak   => GC_HANDLE_TYPE_WEAK,
        GcHandleKind::Strong => GC_HANDLE_TYPE_STRONG,
    }
}

/// `Std.GCHandle` TypeDesc — single `_slot: long` field. Cached so that
/// repeated `Alloc` calls share a single Arc.
fn gc_handle_type_desc() -> Arc<TypeDesc> {
    static CACHE: OnceLock<Arc<TypeDesc>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut field_index = crate::metadata::NameIndex::new();
        field_index.insert("_slot".to_string(), 0usize);
        let fields = vec![FieldSlot { name: "_slot".to_string().into(), type_tag: "long".to_string().into(), visibility: 0 }];
        Arc::new(TypeDesc {
            name: "Std.GCHandle".to_string(),
            base_name: None,
            class_flags: 0,
            visibility: 0,
            cold: Some(Box::new(crate::metadata::types::TypeDescCold {
                own_fields: fields.clone().into(),
                ..Default::default()
            })),
            fields,
            field_index,
            vtable: Vec::new(),
            vtable_index: crate::metadata::NameIndex::new(),
            id: crate::metadata::tokens::TypeId::UNRESOLVED,
        })
    }).clone()
}

/// `Std.HeapStats` TypeDesc — 7 long fields projecting Rust `HeapStats`.
///
/// The script declares each as an auto-property `public long X { get; }`, which
/// the compiler desugars to a private backing field `__prop_X` + `get_X()`
/// accessor. So slot names here carry the `__prop_` prefix.
fn heap_stats_type_desc() -> Arc<TypeDesc> {
    static CACHE: OnceLock<Arc<TypeDesc>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let names = [
            "__prop_Allocations", "__prop_GcCycles", "__prop_UsedBytes", "__prop_MaxBytes",
            "__prop_RootsPinned", "__prop_FinalizersPending", "__prop_Observers",
        ];
        let mut field_index = crate::metadata::NameIndex::new();
        let mut fields = Vec::with_capacity(names.len());
        for (i, n) in names.iter().enumerate() {
            field_index.insert(n.to_string(), i);
            fields.push(FieldSlot { name: n.to_string().into(), type_tag: "long".to_string().into(), visibility: 0 });
        }
        Arc::new(TypeDesc {
            name: "Std.HeapStats".to_string(),
            base_name: None,
            class_flags: 0,
            visibility: 0,
            cold: Some(Box::new(crate::metadata::types::TypeDescCold {
                own_fields: fields.clone().into(),
                ..Default::default()
            })),
            fields,
            field_index,
            vtable: Vec::new(),
            vtable_index: crate::metadata::NameIndex::new(),
            id: crate::metadata::tokens::TypeId::UNRESOLVED,
        })
    }).clone()
}

/// Read the `_slot: long` field out of a `Std.GCHandle` receiver. Treats null /
/// non-Object / missing field as slot 0 ("unallocated").
fn extract_gc_handle_slot(arg: &Value) -> u64 {
    let Value::Object(rc) = arg else { return 0 };
    let obj = rc.borrow();
    match obj.field_value(0) {
        Value::I64(i) => i.max(0) as u64,
        _ => 0,
    }
}

fn make_gc_handle(ctx: &VmContext, slot: u64) -> Value {
    ctx.heap().alloc_object(
        gc_handle_type_desc(),
        vec![Value::I64(slot as i64)],
        NativeData::None,
    )
}

/// `Std.GCHandle.Alloc(target, GCHandleType type)` — returns a new GCHandle
/// struct. Atomic-Weak / null target → `_slot=0` (`IsAllocated=false`).
pub fn builtin_gc_handle_alloc(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let target = args.first().ok_or_else(|| anyhow!("__gc_handle_alloc: missing target arg"))?;
    let kind_int = match args.get(1) {
        Some(Value::I64(v)) => *v,
        _ => return Err(anyhow!("__gc_handle_alloc: expected GCHandleType (int) arg")),
    };
    let slot = ctx.heap().handle_alloc(target, gc_handle_kind_from_i64(kind_int));
    Ok(make_gc_handle(ctx, slot))
}

/// `Std.GCHandle.Target { get; }` — returns null when slot freed or weak target collected.
pub fn builtin_gc_handle_target(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = extract_gc_handle_slot(args.first().unwrap_or(&Value::Null));
    Ok(ctx.heap().handle_target(slot).unwrap_or(Value::Null))
}

/// `Std.GCHandle.IsAllocated { get; }`.
pub fn builtin_gc_handle_is_alloc(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = extract_gc_handle_slot(args.first().unwrap_or(&Value::Null));
    Ok(Value::Bool(ctx.heap().handle_is_alloc(slot)))
}

/// `Std.GCHandle.Kind { get; }` — returns `GCHandleType` enum int. Freed slot
/// returns `Strong` (default) — callers should check `IsAllocated` first.
pub fn builtin_gc_handle_kind(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = extract_gc_handle_slot(args.first().unwrap_or(&Value::Null));
    let kind = ctx.heap().handle_kind(slot).unwrap_or(GcHandleKind::Strong);
    Ok(Value::I64(gc_handle_kind_to_i64(kind)))
}

/// `Std.GCHandle.Free()` — releases the slot. Idempotent (also no-op on slot 0).
pub fn builtin_gc_handle_free(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = extract_gc_handle_slot(args.first().unwrap_or(&Value::Null));
    ctx.heap().handle_free(slot);
    Ok(Value::Null)
}

/// `Std.GC.PauseHistogram()` — **add-gc-pause-histogram (2026-05-22)**.
///
/// Returns a `long[]` of length 8 where `result[i]` is the count of
/// collects whose `pause_us` fell into bucket `i`. Bucket boundaries
/// (half-open `[lower, upper)`, microseconds):
///
/// | i | range                       |
/// |---|-----------------------------|
/// | 0 | `[0, 10) µs`                |
/// | 1 | `[10, 100) µs`              |
/// | 2 | `[100 µs, 1 ms)`            |
/// | 3 | `[1, 10) ms`                |
/// | 4 | `[10, 100) ms`              |
/// | 5 | `[100 ms, 1 s)`             |
/// | 6 | `[1, 10) s`                 |
/// | 7 | `[10 s, ∞)`                 |
pub fn builtin_gc_pause_histogram(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let h = ctx.heap().stats().pause_histogram;
    let arr: Vec<Value> = h.buckets.iter()
        .map(|&n| Value::I64(n as i64))
        .collect();
    Ok(ctx.heap().alloc_array(arr))
}

/// `Std.GC.PauseStatsRaw()` — **add-gc-pause-histogram (2026-05-22)**.
///
/// Returns a `long[]` of length 4: `[min_us, max_us, total_us, count]`.
/// When no collects have occurred, `min_us == i64::MAX` (sentinel) and
/// the other three fields are 0. Callers should check `count == 0`
/// before reading `min_us`.
pub fn builtin_gc_pause_stats_raw(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let h = ctx.heap().stats().pause_histogram;
    Ok(ctx.heap().alloc_array(vec![
        Value::I64(h.min_us   as i64),
        Value::I64(h.max_us   as i64),
        Value::I64(h.total_us as i64),
        Value::I64(h.count    as i64),
    ]))
}

/// `Std.GC.RecentPauses()` — **add-gc-pause-window (2026-05-24)**.
///
/// Returns the rolling pause-time window as a `long[]` (microseconds,
/// oldest first). Length is at most `PauseWindowCapacity()`; 0 when no
/// collect has occurred yet.
pub fn builtin_gc_recent_pauses(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let h = ctx.heap().stats().pause_histogram;
    let arr: Vec<Value> = h.recent_pauses.iter()
        .map(|&us| Value::I64(us as i64))
        .collect();
    Ok(ctx.heap().alloc_array(arr))
}

/// `Std.GC.PauseWindowCapacity()` — **add-gc-pause-window (2026-05-24)**.
///
/// Returns the configured rolling-window capacity (`Z42_GC_PAUSE_WINDOW`
/// env or default 1024). Useful for distinguishing "only N collects have
/// happened" from "the window saturated at N".
pub fn builtin_gc_pause_window_capacity(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let cap = ctx.heap().stats().pause_histogram.window_cap;
    Ok(Value::I64(cap as i64))
}

/// `Std.GC.WriteHeapSnapshot(path)` — **add-gc-heap-snapshot-export (2026-05-24)**.
///
/// Walks the live heap once via `MagrGC::iterate_live_objects` +
/// `scan_object_refs` + `for_each_root`, builds the V8 `.heapsnapshot`
/// graph, serialises it to JSON, and writes the result to `path`.
/// Returns the number of bytes written (as `long`).
///
/// The resulting file opens directly in Chrome DevTools → Memory →
/// Load, speedscope, and heapviewer.com.
pub fn builtin_gc_write_heap_snapshot(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err(anyhow!("__gc_write_heap_snapshot: expected string path")),
    };
    // add-gc-snapshot-streaming (2026-05-25): write JSON directly to
    // a BufWriter<File> via the streaming serializer — avoids the
    // ~30 MB intermediate String for large heaps.
    let snap = crate::gc::snapshot::build_graph_snapshot(ctx.heap());
    let file = std::fs::File::create(&*path)
        .map_err(|e| anyhow!("__gc_write_heap_snapshot: create {}: {}", path, e))?;
    let mut writer = std::io::BufWriter::new(file);
    let n_bytes = crate::gc::snapshot::serialize_v8_heapsnapshot_to(&snap, &mut writer)
        .map_err(|e| anyhow!("__gc_write_heap_snapshot: write {}: {}", path, e))?;
    // Explicit flush so we surface IO errors before returning success;
    // BufWriter's Drop-flush silently drops errors otherwise.
    writer.flush()
        .map_err(|e| anyhow!("__gc_write_heap_snapshot: flush {}: {}", path, e))?;
    Ok(Value::I64(n_bytes as i64))
}

/// `Std.GC.SetMaxHeapBytes(bytes)` — **add-gc-oom-exception (2026-05-25)**.
/// Sets the heap upper limit. `bytes <= 0` clears the limit (unbounded).
pub fn builtin_gc_set_max_heap_bytes(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let bytes = match args.first() {
        Some(Value::I64(n)) if *n > 0 => Some(*n as u64),
        _ => None,
    };
    ctx.heap().set_max_heap_bytes(bytes);
    Ok(Value::Null)
}

/// `Std.GC.SetStrictOOM(enabled)` — **add-gc-oom-exception (2026-05-25)**.
/// Enables / disables strict OOM mode. When enabled, alloc over
/// `max_heap_bytes` throws `Std.OutOfMemoryException` instead of
/// returning `Value::Null` silently.
pub fn builtin_gc_set_strict_oom(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let enabled = matches!(args.first(), Some(Value::Bool(true)));
    ctx.heap().set_strict_oom(enabled);
    Ok(Value::Null)
}

// ── Soft reference builtins (add-gc-softref, 2026-05-26) ─────────────────────

/// `Std.SoftHandle` TypeDesc — single `_key: long` field holding the soft-ref key.
fn soft_handle_type_desc() -> Arc<TypeDesc> {
    static CACHE: OnceLock<Arc<TypeDesc>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut field_index = crate::metadata::NameIndex::new();
        field_index.insert("_key".to_string(), 0usize);
        let fields = vec![FieldSlot { name: "_key".to_string().into(), type_tag: "long".to_string().into(), visibility: 0 }];
        Arc::new(TypeDesc {
            name: "Std.SoftHandle".to_string(),
            base_name: None,
            class_flags: 0,
            visibility: 0,
            cold: Some(Box::new(crate::metadata::types::TypeDescCold {
                own_fields: fields.clone().into(),
                ..Default::default()
            })),
            fields,
            field_index,
            vtable: Vec::new(),
            vtable_index: crate::metadata::NameIndex::new(),
            id: crate::metadata::tokens::TypeId::UNRESOLVED,
        })
    }).clone()
}

/// Read the `_key: long` field from a `Std.SoftHandle` receiver.
fn soft_handle_key(handle: &Value) -> Result<u64> {
    match handle {
        Value::Object(gc) => {
            let obj = gc.borrow();
            match obj.field_value(0) {
                Value::I64(k) => Ok(k as u64),
                _ => Err(anyhow!("SoftHandle._key: expected long slot")),
            }
        }
        Value::Null => Err(anyhow!("SoftHandle._key: null receiver")),
        _ => Err(anyhow!("SoftHandle._key: not an object")),
    }
}

/// `Std.SoftHandle.Create(object target)` — **add-gc-softref (2026-05-26)**.
/// Registers a soft reference to `target` and returns a new `Std.SoftHandle`
/// object whose `_key` field holds the opaque registry key.
/// Returns a SoftHandle with `_key = 0` if `target` is null or atomic.
pub fn builtin_soft_handle_create(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let target = args.first().unwrap_or(&Value::Null);
    let key = ctx.heap().register_soft_ref(target);
    Ok(ctx.heap().alloc_object(
        soft_handle_type_desc(),
        vec![Value::I64(key as i64)],
        NativeData::None,
    ))
}

/// `Std.SoftHandle.Get()` — **add-gc-softref (2026-05-26)**.
/// Returns the soft-ref target, or `Value::Null` if it was cleared by GC.
pub fn builtin_soft_handle_get(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let receiver = args.first().unwrap_or(&Value::Null);
    let key = soft_handle_key(receiver)?;
    if key == 0 { return Ok(Value::Null); }
    Ok(ctx.heap().soft_ref_get(key))
}

/// `Std.GC.GetStats()` — projects Rust `HeapStats` into a `Std.HeapStats` instance.
/// `MaxBytes` uses `-1` as the unlimited sentinel (z42 has no `Optional<T>`).
pub fn builtin_gc_stats(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let s = ctx.heap().stats();
    Ok(ctx.heap().alloc_object(
        heap_stats_type_desc(),
        vec![
            Value::I64(s.allocations        as i64),
            Value::I64(s.gc_cycles          as i64),
            Value::I64(s.used_bytes         as i64),
            Value::I64(s.max_bytes.map(|v| v as i64).unwrap_or(-1)),
            Value::I64(s.roots_pinned       as i64),
            Value::I64(s.finalizers_pending as i64),
            Value::I64(s.observers          as i64),
        ],
        NativeData::None,
    ))
}

