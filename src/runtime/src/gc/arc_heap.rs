//! `ArcMagrGC` —— 默认 GC backend（接口完整 + Trial-deletion 环回收器）。
//!
//! 通过 [`GcRef<T>`] 句柄抽象走 `Rc<GcAllocation<T>>` backing（GcAllocation
//! 含 `inner: RefCell<T>` + `finalizer: RefCell<Option<FinalizerFn>>` + 自定义
//! `Drop`，Phase 3e 起 Drop 时自动触发 finalizer），同时实现 MMTk porting
//! contract 形状的全部 host-side 嵌入接口（roots / observers / profiler /
//! weak refs / finalizers / heap config / strict OOM / ...）。
//!
//! **Phase 3a/3b/3c/3d/3d.1/3f/3e/3f-2/3-OOM 后已知限制**：（无）
//!
//! GC 子系统主功能至此完整。所有原始限制已解决：
//! - 接口（trait + GcRef + heap registry）✅
//! - 环回收（trial-deletion）✅
//! - Finalizer（drop-time + cycle collect 双路径）✅
//! - 自动 collect（内存压力）✅
//! - VmContext 级 roots（static_fields + pending_exception）✅
//! - 栈扫描（interp + JIT 全部 frame.regs）✅
//! - **OOM 真拒绝（strict 模式可选启用）✅**
//! - 端到端验证（`Std.GC.*` 暴露 + golden tests）✅
//!
//! **已解决**：
//! - Phase 3b（add-heap-registry）：snapshot/iterate Full coverage
//! - Phase 3c（add-cycle-breaking-collector）：环引用真实回收 + `used_bytes` 精确
//! - Phase 3d（add-finalizer-and-auto-collect）：finalizer 真触发 + 内存压力自动 collect
//! - Phase 3d.1（add-external-root-scanning）：**external root scanner 机制 +
//!   VmContext 的 `static_fields` / `pending_exception` 自动暴露为 GC roots**，
//!   修复 cycle collector 漏扫导致 static 字段持有的对象被误清的 bug
//! - Phase 3f（add-interp-stack-scanning）：interp `exec_function` 通过
//!   `FrameGuard` RAII 把 `frame.regs` Vec 指针注册到 `VmContext.exec_stack`，
//!   scanner 闭包遍历喂给 mark 阶段。修复脚本执行中调 GC 时
//!   "outer 在 reg + outer.slot → inner 间接可达 → inner 被误清" 的 bug
//! - Phase 3e（add-drop-time-finalizer）：`GcRef<T>` backing 升级为
//!   `Rc<GcAllocation<T>>`，wrapper 含 `finalizer: RefCell<Option<FinalizerFn>>`
//!   + 自定义 `Drop`。**所有 Rc Drop 路径**（含纯链式 drop / cycle 断环
//!   后 alive_vec drop / 普通 scope 退出）都自动触发已注册 finalizer，
//!   one-shot via take。`finalizers: HashMap` 字段移除，`stats()` 即时遍历
//!   registry 重算 finalizers_pending。
//! - Phase 3f-2（add-jit-stack-scanning）：6 个 JitFrame::new callsite 在
//!   jit_fn 调用前后 push/pop frame.regs 到 VmContext.exec_stack（与 interp
//!   共用同一数据结构）。修复 JIT 路径下 transitive 可达对象（如返回值穿过
//!   函数边界后通过 outer.slot 间接持有）被误清的 bug。
//! - Phase 3-OOM（add-strict-oom-rejection）：trait 加 `set_strict_oom(bool)`
//!   默认 no-op（向后兼容）。ArcMagrGC 启用 strict 模式后 alloc 越过
//!   max_heap_bytes 时返回 `Value::Null` 不入 registry / 不 bump used_bytes
//!   （撤销分配），同时 fire OutOfMemory 事件。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::metadata::{ScriptObject, Value};
use crate::metadata::types::ArrayObj;

use super::refs::{GcRef, WeakGcRef};
use super::var_region::{BlockType, VarRegion};
use super::types::{
    AllocSamplerFn, GcHandleKind, GcObserver, HeapStats, ObserverId, RootHandle,
};

// ── Handle table（reorganize-gc-stdlib，2026-05-07）─────────────────────────

/// One slot in [`HandleSlab`]. Strong slots store cloneable references that
/// anchor their target across collection; weak slots store a `Weak<...>` that
/// silently nulls out when the target drops elsewhere.
///
/// `Strong(Atomic)` covers `AllocStrong` for atomic values (`I64` / `Str` / ...)
/// — not Rc-backed, so we just hold the cloned `Value`. `AllocWeak` rejects
/// atomic values at the `handle_alloc` layer (returns slot 0).
enum HandleEntry {
    StrongObject(GcRef<ScriptObject>),
    StrongArray(GcRef<ArrayObj>),
    /// Atomic Value clone (I64 / F64 / Str / Bool / Char / FuncRef / ...).
    /// Strong-only — AllocWeak on atomics rejects at the alloc layer.
    StrongAtomic(Value),
    WeakObject(WeakGcRef<ScriptObject>),
    WeakArray(WeakGcRef<ArrayObj>),
}

impl HandleEntry {
    fn kind(&self) -> GcHandleKind {
        match self {
            HandleEntry::StrongObject(_)
            | HandleEntry::StrongArray(_)
            | HandleEntry::StrongAtomic(_) => GcHandleKind::Strong,
            HandleEntry::WeakObject(_) | HandleEntry::WeakArray(_) => GcHandleKind::Weak,
        }
    }

    /// Read the slot's current target; weak slots return None once collected.
    fn target(&self) -> Option<Value> {
        match self {
            HandleEntry::StrongObject(g) => Some(Value::Object(g.clone())),
            HandleEntry::StrongArray(g)  => Some(Value::Array(g.clone())),
            HandleEntry::StrongAtomic(v) => Some(v.clone()),
            HandleEntry::WeakObject(w)   => w.upgrade().map(Value::Object),
            HandleEntry::WeakArray(w)    => w.upgrade().map(Value::Array),
        }
    }
}

/// `Vec<Option<HandleEntry>>` slab + `Vec<u64>` free list. Slot id 0 is reserved
/// as the "unallocated" sentinel — `entries[0]` is never read or written.
#[derive(Default)]
struct HandleSlab {
    entries:   Vec<Option<HandleEntry>>,
    free_list: Vec<u64>,
}

impl HandleSlab {
    fn alloc(&mut self, entry: HandleEntry) -> u64 {
        // Lazy-init: reserve index 0 as the unallocated sentinel on first use.
        if self.entries.is_empty() {
            self.entries.push(None);
        }
        if let Some(slot) = self.free_list.pop() {
            self.entries[slot as usize] = Some(entry);
            slot
        } else {
            let slot = self.entries.len() as u64;
            self.entries.push(Some(entry));
            slot
        }
    }

    fn get(&self, slot: u64) -> Option<&HandleEntry> {
        self.entries.get(slot as usize).and_then(|e| e.as_ref())
    }

    fn free(&mut self, slot: u64) {
        if slot == 0 { return; }
        let idx = slot as usize;
        if idx >= self.entries.len() { return; }
        if self.entries[idx].take().is_some() {
            self.free_list.push(slot);
        }
    }
}

// ── Internal state ───────────────────────────────────────────────────────────

#[derive(Default)]
struct RcHeapInner {
    stats:             HeapStats,
    roots:             HashMap<RootHandle, Value>,
    /// 每个 frame 的 pin 列表，用于 leave_frame 时整批 unpin。
    frame_pins:        Vec<Vec<RootHandle>>,
    observers:         Vec<(ObserverId, Arc<dyn GcObserver>)>,
    alloc_sampler:     Option<AllocSamplerFn>,
    pause_count:       u32,
    next_root_id:      u64,
    next_observer_id:  u64,
    /// 防止 NearHeapLimit 事件刷屏（Phase 3d 后 collect_cycles 完成且使用降到
    /// 阈值以下时会自动 reset，下次跨阈值能再发事件）。
    near_limit_warned: bool,
    /// **add-custom-allocator P1 (2026-05-22)**: heap_registry deleted.
    /// Authoritative liveness store is now `ArcMagrGC.region_object` +
    /// `region_array`. Sweep + iterate walk the regions directly.
    /// **Phase 3d**: 上次 auto-collect 触发时的 `used_bytes`，用于 throttle
    /// 自动 collect —— 仅当当前 used 距上次增长 >= 10% limit 才再次自动触发。
    last_auto_collect_used: u64,
    /// **Phase 3-OOM**: strict OOM 模式开关。true 时 alloc 越界返回 Value::Null
    /// 不入 registry / 不 bump used_bytes（撤销分配）；false（默认）兼容历史
    /// 行为：alloc 仍成功，只 fire 事件。
    strict_oom: bool,
    /// **reorganize-gc-stdlib（2026-05-07）**: GCHandle slab。Slot 0 reserved
    /// 作"未分配" sentinel；其他 slot 由 `Std.GCHandle._slot: long` 引用。
    handle_slab: HandleSlab,
    // **Phase 3e**: finalizers 不再集中存 HashMap；改存到每个 GcAllocation 的
    // finalizer Cell 上。Drop 时自动 take + fire（含 cycle 断环后 alive_vec
    // drop 链）。register_finalizer / cancel_finalizer 走 GcRef 方法。
    // finalizers_pending 由 stats() 即时遍历 registry 重算。

    /// **add-gc-softref (2026-05-26)**: registry of all active soft
    /// references. Populated by `register_soft_ref`; entries are removed
    /// on `unregister_soft_ref`. The revive pass (between mark + sweep)
    /// iterates this to re-mark alive targets when heap pressure is below
    /// the soft threshold.
    soft_registry: super::soft_registry::SoftRegistry,
}

impl std::fmt::Debug for RcHeapInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RcHeapInner")
            .field("stats",             &self.stats)
            .field("roots_count",       &self.roots.len())
            .field("frame_count",       &self.frame_pins.len())
            .field("observers_count",   &self.observers.len())
            .field("alloc_sampler",     &self.alloc_sampler.is_some())
            .field("pause_count",       &self.pause_count)
            .field("near_limit_warned", &self.near_limit_warned)
            .field("last_auto_collect_used", &self.last_auto_collect_used)
            .finish()
    }
}

// ── ArcMagrGC ─────────────────────────────────────────────────────────────────

/// External root scanner type. 宿主（典型情况是 `VmCore` / `VmContext`）
/// 通过 `set_external_root_scanner` 注册的闭包，在 mark 阶段被调用以暴露
/// 自己持有的 Value（如 static_fields / pending_exception / interp 栈帧 regs），
/// 让 cycle collector 不会把这些可达对象误判为 unreachable。
///
/// **add-multithreading-foundation Phase 3 (2026-05-20)**：要求 `Send + Sync`
/// —— 闭包内部捕获 Arc<VmCore> Weak 等 Send-safe handle；GC 后续可能在
/// 独立 worker 线程上跑收集。
pub type ExternalRootScanner = Box<dyn Fn(&mut dyn FnMut(&Value)) + Send + Sync>;

/// **add-lazy-context-unload (2026-08-05)**: lets the GC drive collectible
/// `AssemblyLoadContext` reclamation. Wired by `VmCore` (captures `Weak<VmCore>`),
/// routes to `VmCore.context_registry`. On a major STW collect: if `is_unloading`,
/// the GC takes a `snapshot`, scans marked objects for retained contexts (see
/// `scan_marked_contexts`), and calls `reclaim` post-sweep to free unreferenced
/// `Unloading` contexts' arenas. `None` when no reclaimer is wired (tests).
pub trait ContextReclaim: Send + Sync {
    /// Cheap gate — true iff ≥1 context is in `Unloading` state (atomic read).
    fn is_unloading(&self) -> bool;
    /// Snapshot the context↔object association for this collect (registry lock once).
    fn snapshot(&self) -> crate::metadata::context::ContextLiveness;
    /// Free every `Unloading` context NOT in `live` (post-sweep, STW).
    fn reclaim(&self, live: &std::collections::HashSet<crate::metadata::context::ContextId>);
}

pub type ContextReclaimHook = Box<dyn ContextReclaim>;

/// **add-heap-retention-diagnostics (2026-08-06)**: like `ExternalRootScanner`
/// but yields each root **with its category** (`RootKind`). Used only by the
/// on-demand retention query (`retention_*`) to report retaining roots at the
/// category level — NOT on the mark hot path (mark uses the anonymous scanner).
pub type CategorizedRootScanner =
    Box<dyn Fn(&mut dyn FnMut(&Value, super::retention::RootKind)) + Send + Sync>;

/// **add-heap-retention-diagnostics (2026-08-06)**: the heap identity (data ptr
/// as usize) of a heap-ref `Value`, or `None` for primitives / stack refs. Used
/// to key the reverse reference graph. Mirrors `mark_if_unmarked`'s variant set.
fn value_heap_ptr(v: &Value) -> Option<usize> {
    match v {
        Value::Object(gc) => Some(gc.data_ptr_unlocked() as usize),
        // add-boxed-struct-identity (P4b): boxed struct is a heap object (region_object).
        Value::BoxedStruct(gc) => Some(gc.data_ptr_unlocked() as usize),
        Value::Array(gc) => Some(gc.data_ptr_unlocked() as usize),
        // unify-gc-heap PR-2: the closure is itself a heap object (region_var block) — its own
        // block address is its identity.
        Value::Closure(c) => Some(c.addr()),
        // unify-gc-heap PR-4: strings are region_var blocks now — the block header address is
        // their heap identity. `FuncRef` likewise carries a `Str`.
        Value::Str(s) => Some(s.as_ptr() as usize),
        Value::FuncRef(s) => Some(s.as_ptr() as usize),
        // make-value-copy: a `Ref` handle has no stable heap identity of its own (its
        // payload lives in the transient arena); like `StructRef`/`StackObject` → None.
        _ => None,
    }
}

/// **unify-gc-heap PR-2/PR-3/PR-5**: payload finalizer for the variable-length region, dispatched
/// by block type. Injected into the region so `var_region.rs` stays a pure byte allocator (no
/// `metadata::types` dependency). Only blocks whose payload owns non-POD data need a drop:
/// - `ArrayValue` → `size / size_of::<Value>()` inline `Value`s (a `Boxed` array's elements or a
///   `struct[]`'s reference side-table). Since PR-4 `Value::Str` is itself a GC handle (no
///   refcount), so these Values are all trivially droppable — but the arm is retained because
///   `Value`'s `Drop` is not statically a no-op (other embedders' variants), and `drop_in_place`
///   on a POD `Value` is a cheap no-op anyway.
/// - `Str` / `ArrayPrim` / `ArrayStruct` (packed bytes) / `Closure` → POD leaves, nothing to drop.
///   PR-5 migrated `ClosureData.fn_name` `String` → GC `Str`, so a `ClosureData` now owns no heap
///   outside the GC (`env: GcRef` + `fn_name: Str` are both no-op/`Copy` drops) → the former
///   `Closure` drop-glue arm is gone.
///
/// # Safety
/// Called once per block reclaim with a valid pointer to that block's initialized `size`-byte
/// payload (upheld by `VarRegion`).
unsafe fn var_drop_glue(bt: BlockType, payload: *mut u8, size: usize) {
    match bt {
        BlockType::ArrayValue => {
            let n = size / std::mem::size_of::<Value>();
            let base = payload as *mut Value;
            for i in 0..n {
                // SAFETY: `base[i]` is one of `n` initialized `Value`s in the block payload.
                unsafe { std::ptr::drop_in_place(base.add(i)) }
            }
        }
        // Str / ArrayPrim / ArrayStruct (packed bytes) / Closure (POD since PR-5): nothing to drop.
        BlockType::Str | BlockType::ArrayPrim | BlockType::ArrayStruct | BlockType::Closure => {}
    }
}

pub struct ArcMagrGC {
    inner: Mutex<RcHeapInner>,
    external_root_scanner: Mutex<Option<ExternalRootScanner>>,
    /// **add-lazy-context-unload**: collectible-context reclaimer hook. `None`
    /// until wired by `VmCore` (tests leave it None → no context reclamation).
    context_reclaimer: Mutex<Option<ContextReclaimHook>>,
    /// **add-heap-retention-diagnostics**: categorized root scanner for the
    /// on-demand retention query. `None` until wired by `VmCore`.
    categorized_root_scanner: Mutex<Option<CategorizedRootScanner>>,
    /// **add-concurrent-gc P0 (2026-05-22)**: selectable GC algorithm.
    /// Encoded as `u8` (`GcMode::from_u8` for round-trip). Read on the
    /// barrier-override hot path and at the entrance of
    /// `run_cycle_collection`. `Relaxed` ordering is sufficient — mode
    /// changes don't synchronize with collect; in-progress collect
    /// completes with its original mode, next collect picks up new
    /// mode (per spec scenario "Mode switch is observable but cannot
    /// interrupt a running collect"). Initialized from
    /// `GcMode::from_env()` so `Z42_GC_MODE=concurrent` selects
    /// concurrent path at process start.
    mode: std::sync::atomic::AtomicU8,
    /// **add-custom-allocator P1 (2026-05-22)**: chunked region for
    /// `Value::Object` script-object storage. Replaces the previous
    /// per-object `Arc<GcAllocation<ScriptObject>>` backing. Sweep
    /// walks this region directly (no separate heap_registry).
    region_object: Mutex<super::region::Region<ScriptObject>>,
    /// **add-custom-allocator P1 (2026-05-22)**: chunked region for
    /// `Value::Array` storage (heap-allocated `Vec<Value>`).
    region_array: Mutex<super::region::Region<ArrayObj>>,
    /// **unify-gc-heap PR-2/3/4/5**: variable-length GC block region for the managed payloads that
    /// don't fit the fixed-size `Region<T>` — `ClosureData` (`Value::Closure`), array backings
    /// (`Value::Array`), and strings (`Value::Str`/`FuncRef`). Constructed with `var_drop_glue`
    /// (only `ArrayValue` blocks need a finalizer; closures/strings/packed arrays are POD leaves
    /// since PR-5). Swept alongside `region_object` / `region_array`.
    region_var: Mutex<VarRegion>,
    /// **add-concurrent-gc P2 (2026-05-22)**: gray-object queue for the
    /// concurrent mark path. Populated by (1) the STW root snapshot at
    /// the start of a concurrent collect, (2) the write-barrier
    /// override (P3) when mutators write heap-ref values into slots,
    /// and (3) the mark thread (P4) when tracing children discovers
    /// newly-reachable objects. Drained by the mark thread + the
    /// termination handshake. `parking_lot::Mutex` is sufficient v1
    /// (z42 typical 1-2 mutators); lock-free upgrade is a deferred
    /// perf spec. Stays empty when mode == StwMarkSweep.
    mark_queue: Mutex<Vec<Value>>,
    /// **add-gc-pause-histogram (2026-05-22)**: aggregate pause-time
    /// histogram. Recorded into at the end of every `collect_cycles` /
    /// `collect_cycles_with_context` / `force_collect` path, right
    /// before the `AfterCollect` event fires. Surfaced via
    /// `stats().pause_histogram` and the `Std.GC.PauseHistogram()` /
    /// `Std.GC.PauseStatsRaw()` z42 builtins. Single histogram per
    /// heap (per-mode split is a deferred perf spec).
    pause_histogram: Mutex<super::types::PauseHistogram>,
    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: external flag the
    /// `maybe_auto_collect` path sets (instead of running collect inline)
    /// when allocation pressure trips the threshold. Drained by the next
    /// `check_safepoint(ctx)` which runs a stop-the-world collect.
    ///
    /// `None` when not wired (e.g. GC unit tests that construct
    /// `ArcMagrGC::new()` standalone without a VmCore) — `maybe_auto_collect`
    /// then falls back to the legacy inline `collect_cycles()` call,
    /// preserving the pre-2026-05-20 single-threaded behaviour.
    external_needs_collect: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// **add-write-barriers (2026-05-21)**: test-only sink for barrier
    /// dispatch events. Production builds (no `cfg(test)`) compile this
    /// field out entirely, so the override on `write_barrier_field` /
    /// `write_barrier_array_elem` collapses to a true no-op.
    #[cfg(test)]
    barrier_observer: Mutex<Option<std::sync::Arc<BarrierObserver>>>,
    #[cfg(debug_assertions)]
    debug_stw_no_push: std::sync::atomic::AtomicBool,
    /// **fix-wasm-string-ops**: process-unique, monotonically-increasing epoch for this heap's
    /// address space (see [`MagrGC::heap_epoch`]). Assigned once at construction from
    /// [`NEXT_HEAP_EPOCH`] and never reused, so an address-keyed cache outliving a torn-down
    /// heap (`corelib::str_meta`) can detect a heap switch and drop entries that would otherwise
    /// false-hit a recycled address (the wasm32 cross-VM string-corruption bug).
    epoch: u64,
    /// **add-gc-tlab (option B, 2026-08-29)**: live allocation counters moved OUT of the
    /// `inner`-locked `HeapStats` onto lock-free atomics, so the per-alloc hot path
    /// (`record_alloc`) no longer takes the `inner` Mutex (removes 2 of the 4–5 inner locks
    /// per `new`). `stats()` reads these to fill the returned `HeapStats` snapshot; collect
    /// paths `sub_used_bytes` the reclaimed bytes. `Relaxed` suffices — these are monotone
    /// counters / heuristic pressure thresholds, not synchronization for other heap state.
    used_bytes: std::sync::atomic::AtomicU64,
    allocations: std::sync::atomic::AtomicU64,
}

/// **fix-wasm-string-ops**: process-global monotonic source for [`ArcMagrGC::epoch`]. Starts at
/// `1` so `0` stays the "no ambient heap" sentinel ([`crate::gc::ambient::current_heap_epoch`]).
static NEXT_HEAP_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// **add-concurrent-gc P0 (2026-05-22)**: manual `Default` impl so the
/// `mode` field is initialized from `GcMode::from_env()` (reads
/// `Z42_GC_MODE`). Other fields fall back to their own `Default`.
impl Default for ArcMagrGC {
    fn default() -> Self {
        Self {
            inner: Mutex::new(RcHeapInner::default()),
            external_root_scanner: Mutex::new(None),
            context_reclaimer: Mutex::new(None),
            categorized_root_scanner: Mutex::new(None),
            external_needs_collect: Mutex::new(None),
            mode: std::sync::atomic::AtomicU8::new(super::GcMode::from_env() as u8),
            region_object: Mutex::new(super::region::Region::new()),
            region_array:  Mutex::new(super::region::Region::new()),
            region_var:    Mutex::new(VarRegion::with_drop_glue(var_drop_glue)),
            mark_queue: Mutex::new(Vec::new()),
            pause_histogram: Mutex::new(super::types::PauseHistogram::default()),
            #[cfg(test)]
            barrier_observer: Mutex::new(None),
            #[cfg(debug_assertions)]
            debug_stw_no_push: std::sync::atomic::AtomicBool::new(false),
            // fix-wasm-string-ops: claim a fresh, never-reused epoch for this heap.
            epoch: NEXT_HEAP_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            // add-gc-tlab (option B): live counters start at 0 (no allocations yet).
            used_bytes: std::sync::atomic::AtomicU64::new(0),
            allocations: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

/// **add-write-barriers (2026-05-21)**: discriminant for a single
/// barrier dispatch event captured by [`BarrierObserver`]. Used by
/// `arc_heap_tests::write_barriers` to prove that interp / JIT call
/// sites invoke the barrier with the expected arguments.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarrierEvent {
    Field { owner_addr: usize, slot: usize, new_is_heap: bool },
    ArrayElem { arr_addr: usize, idx: usize, new_is_heap: bool },
}

/// **add-write-barriers (2026-05-21)**: test-only barrier event sink.
/// Wrap in `Arc` for sharing across the heap and the test assertion
/// closure. Construct via `BarrierObserver::new()`, install via
/// [`ArcMagrGC::install_barrier_observer`], read recorded events via
/// [`BarrierObserver::events`].
#[cfg(test)]
#[derive(Debug, Default)]
pub struct BarrierObserver {
    events: Mutex<Vec<BarrierEvent>>,
}

#[cfg(test)]
impl BarrierObserver {
    pub fn new() -> Self { Self::default() }
    pub fn events(&self) -> Vec<BarrierEvent> { self.events.lock().clone() }
    pub fn count(&self) -> usize { self.events.lock().len() }
    pub(crate) fn push(&self, ev: BarrierEvent) { self.events.lock().push(ev); }
}

impl std::fmt::Debug for ArcMagrGC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scanner_set = self.external_root_scanner.try_lock()
            .map(|s| s.is_some())
            .unwrap_or(false);
        let mut d = f.debug_struct("ArcMagrGC");
        match self.inner.try_lock() {
            Some(i) => { d.field("inner", &*i); }
            None    => { d.field("inner", &"<borrowed>"); }
        }
        d.field("external_scanner", &scanner_set).finish()
    }
}

// ── concern 子模块（refactor-arc-heap-modularization）─────────────────────────
// 每个子模块以 `impl ArcMagrGC` 承载一组职责的 inherent 方法；跨模块调用的方法标
// `pub(super)`（arc_heap 子树内可见）。GC 公共 trait 接口在 `interface.rs`（薄委托）。
mod alloc;
mod collect;
mod control;
mod generational;
mod roots;
mod observe;
mod interface;
#[cfg(any(test, debug_assertions))]
mod debug;

impl ArcMagrGC {
    /// 新建默认 GC backend（`Default` 从 `Z42_GC_MODE` 选 mode + 领取 epoch）。
    pub fn new() -> Self { Self::default() }
}

#[cfg(test)]
#[path = "arc_heap_tests/mod.rs"]
mod arc_heap_tests;
