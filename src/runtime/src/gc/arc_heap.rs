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
// fix-wasm-embed-gc-time: Instant/OnceLock only back `now_us()` (GC pause timing),
// which uses a counter on wasm (no std::time there) — so gate the imports off wasm.
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use crate::metadata::{NativeData, ScriptObject, TypeDesc, Value};
use crate::metadata::types::ArrayObj;

use super::heap::MagrGC;
use super::refs::{GcRef, WeakGcRef};
use super::types::{
    AllocKind, AllocSample, AllocSamplerFn, CollectStats, FinalizerFn, FrameMark,
    GcEvent, GcHandleKind, GcKind, GcObserver, HeapSnapshot, HeapStats, ObserverId,
    RootHandle, SnapshotCoverage, WeakRef, WeakRefInner,
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
        Value::Closure(c) => Some(c.env.data_ptr_unlocked() as usize),
        Value::Ref(kind) => match kind.as_ref() {
            crate::metadata::types::RefKind::Array { gc_ref, .. } => {
                Some(gc_ref.data_ptr_unlocked() as usize)
            }
            crate::metadata::types::RefKind::Field { gc_ref, .. } => {
                Some(gc_ref.data_ptr_unlocked() as usize)
            }
            crate::metadata::types::RefKind::Stack { .. } => None,
        },
        _ => None,
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
}

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
            mark_queue: Mutex::new(Vec::new()),
            pause_histogram: Mutex::new(super::types::PauseHistogram::default()),
            #[cfg(test)]
            barrier_observer: Mutex::new(None),
            #[cfg(debug_assertions)]
            debug_stw_no_push: std::sync::atomic::AtomicBool::new(false),
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

impl ArcMagrGC {
    pub fn new() -> Self { Self::default() }

    /// **add-write-barriers (2026-05-21)**: install a test-only observer
    /// that records every `write_barrier_field` / `write_barrier_array_elem`
    /// dispatch on this heap instance. Returns the previously-installed
    /// observer (if any) so tests can chain. Replaces (not stacks) — one
    /// observer per heap. `clear_barrier_observer` removes.
    #[cfg(test)]
    pub fn install_barrier_observer(
        &self,
        obs: std::sync::Arc<BarrierObserver>,
    ) -> Option<std::sync::Arc<BarrierObserver>> {
        std::mem::replace(&mut *self.barrier_observer.lock(), Some(obs))
    }

    /// **add-write-barriers (2026-05-21)**: uninstall the test observer.
    #[cfg(test)]
    pub fn clear_barrier_observer(&self) -> Option<std::sync::Arc<BarrierObserver>> {
        self.barrier_observer.lock().take()
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: test-only entry point to
    /// `snapshot_roots_into_mark_queue`. The production caller will be
    /// `run_cycle_collection_concurrent` (P4) under STW; tests need to
    /// drive the snapshot directly without setting up a real collect.
    #[cfg(test)]
    pub(crate) fn snapshot_roots_into_mark_queue_for_test(&self) -> usize {
        self.snapshot_roots_into_mark_queue()
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: test-only entry to read
    /// the mark queue contents.
    #[cfg(test)]
    pub(crate) fn mark_queue_for_test(&self) -> Vec<Value> {
        self.mark_queue.lock().clone()
    }

    /// **add-gc-debug-invariants P1 (2026-05-22)**: test-only mutable
    /// access to the mark queue for injecting corruption (e.g. leaving
    /// stale entries to verify validate panics).
    #[cfg(test)]
    pub(crate) fn mark_queue_for_test_mut(&self) -> parking_lot::MutexGuard<'_, Vec<Value>> {
        self.mark_queue.lock()
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: test-only entry to the
    /// `mark_if_unmarked` static helper.
    #[cfg(test)]
    pub(crate) fn mark_if_unmarked_for_test(v: &Value) -> bool {
        Self::mark_if_unmarked(v)
    }

    /// **add-generational-gc P1 (2026-05-22)**: test-only accessors
    /// for the region locks (needed by `arc_heap_tests::generational`
    /// to peek at card_dirty state without going through MagrGC trait).
    #[cfg(test)]
    pub(crate) fn region_object_for_test(&self) -> &Mutex<super::region::Region<ScriptObject>> {
        &self.region_object
    }

    #[cfg(test)]
    pub(crate) fn region_array_for_test(&self) -> &Mutex<super::region::Region<ArrayObj>> {
        &self.region_array
    }

    /// **add-generational-gc P3 (2026-05-22)**: test-only entry to the
    /// minor escalation threshold (used by tests; production reads via
    /// minor_escalation_threshold() in the dispatch path).
    #[cfg(test)]
    pub(crate) fn minor_escalation_threshold_for_test() -> f32 {
        Self::minor_escalation_threshold()
    }

    /// **add-gc-debug-invariants P1 (2026-05-22)**: post-collect
    /// invariant check. Validates both regions + heap-wide invariants.
    /// Panics on first violation with a descriptive message. Release
    /// builds compile this method body out entirely via the cfg gate
    /// at the call site.
    ///
    /// Invariants checked:
    /// - `region_object` + `region_array`: see
    ///   [`super::region::Region::validate`]
    /// - `mark_queue` is empty post-collect (concurrent mark must
    ///   drain to empty before sweep; STW + generational never use
    ///   the queue)
    /// - No alive entry has `marked == 1` (sweep clears marks on
    ///   survivors; orphaned mark bit = bug)
    #[cfg(debug_assertions)]
    pub(crate) fn debug_validate_invariants(&self) {
        // 1. Per-region invariants.
        if let Err(v) = self.region_object.lock().validate() {
            panic!("region_object invariant violation: {}", v);
        }
        if let Err(v) = self.region_array.lock().validate() {
            panic!("region_array invariant violation: {}", v);
        }

        // 2. mark_queue must be empty post-collect.
        //
        // diag-mark-queue-stale (2026-05-30): on failure, dump each
        // stale entry's kind + (for heap refs) the GcRef pointer + the
        // marked bit. This is the only way to tell whether the entry
        // was pushed by a still-running mutator (write_barrier_field
        // pushes pre-marked refs) vs. a buggy collector-internal push
        // (would push unmarked, which would also indicate a different
        // class of bug). Without the dump the assertion is opaque —
        // we couldn't diagnose the windows-only flake (concurrent_gc_
        // mode_stress_no_race_no_leak) before this commit.
        let stale: Vec<Value> = self.mark_queue.lock().clone();
        if !stale.is_empty() {
            let summary: Vec<String> = stale.iter().take(8).map(|v| {
                let kind = match v {
                    Value::Object(_) => "Object",
                    Value::Array(_)  => "Array",
                    Value::Str(_)    => "Str",
                    Value::I64(_)    => "I64",
                    Value::F64(_)    => "F64",
                    Value::Bool(_)   => "Bool",
                    Value::Char(_)   => "Char",
                    Value::Null      => "Null",
                    _                => "Other",
                };
                let extra = match v {
                    Value::Object(gc) => format!(" obj_borrow_type={}", gc.type_desc().name),
                    Value::Array(gc)  => format!(" array_len={}", gc.borrow().len()),
                    _                  => String::new(),
                };
                format!("    [kind={kind}{extra}]")
            }).collect();
            let extra = if stale.len() > 8 {
                format!("\n    ... ({} more)", stale.len() - 8)
            } else {
                String::new()
            };
            panic!(
                "mark_queue stale post-collect: {} entries remaining\n{}{}",
                stale.len(), summary.join("\n"), extra
            );
        }

        // 3. No alive entry should carry marked=1 post-sweep.
        //    iterate_alive walks heap-registry-equivalent (regions).
        //
        // diag-stale-mark-bit (2026-05-30): on failure, dump the entry's
        // type name + slot summary so we can tell whether the marked
        // object is the freshly-allocated `Leaf` from the stress
        // worker loop (race: barrier ran post-sweep) vs an `Owner` /
        // older object (race: something re-marked it during STW).
        let region_object = self.region_object.lock();
        let mut stale_obj: Option<(u32, u32, String, usize)> = None;
        region_object.iterate_alive(|h, e| {
            if stale_obj.is_some() { return; }
            if e.is_marked() {
                let obj = e.value.lock();
                let ty = obj.type_desc.name.clone();
                let nslots = obj.slots.len();
                stale_obj = Some((h.chunk_idx as u32, h.entry_idx as u32, ty, nslots));
            }
        });
        drop(region_object);
        if let Some((c, i, ty, n)) = stale_obj {
            panic!(
                "stale mark bit in region_object after sweep: chunk={c}, entry={i}, type={ty}, slots={n}"
            );
        }
        let region_array = self.region_array.lock();
        let mut stale_arr: Option<(u32, u32, usize)> = None;
        region_array.iterate_alive(|h, e| {
            if stale_arr.is_some() { return; }
            if e.is_marked() {
                let arr = e.value.lock();
                stale_arr = Some((h.chunk_idx as u32, h.entry_idx as u32, arr.len()));
            }
        });
        drop(region_array);
        if let Some((c, i, len)) = stale_arr {
            panic!(
                "stale mark bit in region_array after sweep: chunk={c}, entry={i}, array_len={len}"
            );
        }
    }

    #[cfg(test)]
    fn fire_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        if let Some(obs) = self.barrier_observer.lock().as_ref() {
            let owner_addr = match owner {
                Value::Object(rc) => GcRef::as_ptr(rc) as *const () as usize,
                _ => 0,
            };
            obs.push(BarrierEvent::Field {
                owner_addr,
                slot,
                new_is_heap: new.is_heap_ref(),
            });
        }
    }

    #[cfg(test)]
    fn fire_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
        if let Some(obs) = self.barrier_observer.lock().as_ref() {
            let arr_addr = match arr {
                Value::Array(rc) => GcRef::as_ptr(rc) as *const () as usize,
                _ => 0,
            };
            obs.push(BarrierEvent::ArrayElem {
                arr_addr,
                idx,
                new_is_heap: new.is_heap_ref(),
            });
        }
    }

    /// **Phase 3d.1**: 注册一个 external root scanner 闭包。每次 cycle
    /// collection mark 阶段在扫完 pinned roots 后会调用此闭包，把闭包 yield
    /// 出来的 Value 也加入 reachable BFS 队列。
    ///
    /// 典型用途：`VmContext::new` 注册一个扫描自己 static_fields /
    /// pending_exception 的闭包，让那些字段持有的 cyclic 对象在 collect 时
    /// 不被误判为可断。
    ///
    #[cfg(not(target_arch = "wasm32"))]
    fn now_us() -> u64 {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
    }

    /// fix-wasm-embed-gc-time: `wasm32-unknown-unknown` has no `std::time`
    /// (`Instant::now()` panics "time not implemented on this platform"). GC
    /// pause-timing runs on every collection — heavy-allocation workloads (the
    /// in-browser embedded test-host, which runs the whole corpus) triggered a
    /// GC cycle and trapped. Use a monotonic counter on wasm so timing stats
    /// stay non-negative (values are ticks, not µs) instead of panicking.
    #[cfg(target_arch = "wasm32")]
    fn now_us() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TICK: AtomicU64 = AtomicU64::new(0);
        TICK.fetch_add(1, Ordering::Relaxed)
    }

    /// 取 Value 的"类型名"（用于 snapshot 聚合）。
    fn type_name_of(value: &Value) -> Option<String> {
        match value {
            Value::Object(rc) => Some(rc.type_desc().name.clone()),
            Value::BoxedStruct(rc) => Some(rc.type_desc().name.clone()),  // add-boxed-struct-identity (P4b)
            Value::Array(_)   => Some("<Array>".to_string()),
            _ => None,
        }
    }

    /// 把事件分发到所有 observer。先 snapshot observer 列表再分发，避免
    /// observer 在回调中重入 add_observer/remove_observer 引发 borrow 冲突。
    fn fire_event(&self, event: GcEvent) {
        let observers: Vec<_> = self.inner.lock().observers.iter()
            .map(|(_, o)| Arc::clone(o)).collect();
        for o in observers {
            o.on_event(&event);
        }
    }

    /// alloc 通用通路：bump stats + 检查压力 + 触发 sampler。
    ///
    /// **add-custom-allocator P1 (2026-05-22)**: previously pushed a
    /// WeakRef to `heap_registry`; that field is gone now — the region
    /// itself is the authoritative liveness store, iterated directly
    /// by sweep / iterate_live_objects / snapshot helpers.
    /// `kind_fn` is a closure so the `AllocKind` — in particular
    /// `AllocKind::Object`'s **class-name `String` clone** — is materialized
    /// only when an alloc sampler is actually installed (the rare profiling
    /// case). On the hot allocation path with no sampler it is never called,
    /// saving a heap alloc + memcpy per object `new` (measured ~10% on
    /// allocation-heavy JIT loops).
    fn record_alloc(&self, _value: &Value, kind_fn: impl FnOnce() -> AllocKind, size: usize) {
        // 1. 更新 stats（先借再放，避免后续触发事件时 borrow 冲突）
        {
            let mut i = self.inner.lock();
            i.stats.allocations += 1;
            i.stats.used_bytes  = i.stats.used_bytes.saturating_add(size as u64);
        }
        // 2. 压力检查（可能触发 GcEvent）
        self.check_pressure(size as u64);
        // 3. Sampler 调度（仅采样时构造 AllocKind → 省掉热路径的类名 String clone）
        let sampler = self.inner.lock().alloc_sampler.clone();
        if let Some(s) = sampler {
            s(&AllocSample {
                kind: kind_fn(),
                size_bytes: size,
                timestamp_us: Self::now_us(),
            });
        }
    }

    // ── Cycle collection helpers (Phase 3c → P3 mark-sweep) ─────────────────

    // **add-mark-sweep-collector P3 (2026-05-21)**: `mark_reachable_set`
    // was trial-deletion's pointer-key reachable HashSet builder. Replaced
    // by `mark_phase` (sets `marked = 1` directly on each reachable
    // GcAllocation; sweep_phase consumes the bit). Deleted in this commit.

    /// **add-mark-sweep-collector P3 (2026-05-21)**: mark phase of the
    /// mark-sweep collector (now the default).
    ///
    /// BFS from roots (pinned + external scanner) → sets `marked = 1` on
    /// every reachable `GcAllocation`. Idempotent within one cycle: the
    /// `GcRef::mark` CAS guarantees each object enqueues children exactly
    /// once even under root reuse. [`sweep_phase`](Self::sweep_phase)
    /// consumes the bit and resets marks on survivors.
    ///
    /// Returns the count of newly-marked allocations — used by unit tests
    /// to verify BFS visits the expected set.
    fn mark_phase(&self) -> usize {
        // Initial roots: pinned + external scanner output.
        let mut queue: Vec<Value> = self.inner.lock().roots.values().cloned().collect();
        {
            let scanner_borrow = self.external_root_scanner.lock();
            if let Some(scan) = scanner_borrow.as_ref() {
                scan(&mut |v| {
                    queue.push(v.clone());
                });
            }
        }

        let mut newly_marked = 0usize;
        while let Some(v) = queue.pop() {
            // Mark the allocation backing this Value; if already marked
            // (or not a heap allocation at all, e.g. a primitive), skip.
            let just_marked = match &v {
                Value::Object(gc) => GcRef::mark(gc),
                Value::Array(gc)  => GcRef::mark(gc),
                Value::Closure(c) => GcRef::mark(&c.env),
                Value::Ref(kind) => match kind.as_ref() {
                    crate::metadata::types::RefKind::Array { gc_ref, .. } => GcRef::mark(gc_ref),
                    crate::metadata::types::RefKind::Field { gc_ref, .. } => GcRef::mark(gc_ref),
                    crate::metadata::types::RefKind::Stack { .. } => false,
                },
                // add-boxed-struct-identity (P4b, 路 B2): a boxed struct is a shared
                // `ScriptObject` in region_object — mark it like Object (trace_children
                // then scans its `struct_refs` reference leaves).
                Value::BoxedStruct(gc) => GcRef::mark(gc),
                // add-struct-heap-inline (P3b): a struct[] element handle keeps its
                // backing array alive (mark it; trace_children then follows its refs).
                Value::StructRefHeap(e) => GcRef::mark(&e.arr),
                _ => false,
            };
            if !just_marked { continue; }
            newly_marked += 1;

            v.trace_children(&mut |child| {
                queue.push(child.clone());
            });
        }
        newly_marked
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: attempt to mark `v` via
    /// CAS. Returns `true` iff this call transitioned the allocation
    /// from unmarked to marked (i.e. caller is responsible for tracing
    /// children). Returns `false` for primitives + already-marked +
    /// non-heap refs (Stack ref kinds). Single source of truth for
    /// "mark this value" — used by both `mark_phase` (when refactored
    /// in P4) and the concurrent path (P3 barrier, P4 mark loop).
    fn mark_if_unmarked(v: &Value) -> bool {
        match v {
            Value::Object(gc) => GcRef::mark(gc),
            Value::Array(gc)  => GcRef::mark(gc),
            Value::Closure(c) => GcRef::mark(&c.env),
            Value::Ref(kind) => match kind.as_ref() {
                crate::metadata::types::RefKind::Array { gc_ref, .. } => GcRef::mark(gc_ref),
                crate::metadata::types::RefKind::Field { gc_ref, .. } => GcRef::mark(gc_ref),
                crate::metadata::types::RefKind::Stack { .. } => false,
            },
            // add-boxed-struct-identity (P4b, 路 B2): mark the boxed struct's shared ScriptObject.
            Value::BoxedStruct(gc) => GcRef::mark(gc),
            // add-struct-heap-inline (P3b): mark the struct[] element handle's array.
            Value::StructRefHeap(e) => GcRef::mark(&e.arr),
            _ => false,
        }
    }

    /// **add-generational-gc P2 (2026-05-22)**: read the gen_age of
    /// any `Value`. Returns 0 for primitives + stack refs (irrelevant
    /// to generational dispatch — mark/sweep already handles those).
    fn gen_age_of(v: &Value) -> u8 {
        match v {
            Value::Object(gc) => GcRef::gen_age(gc),
            Value::Array(gc)  => GcRef::gen_age(gc),
            Value::Closure(c) => GcRef::gen_age(&c.env),
            Value::Ref(kind) => match kind.as_ref() {
                crate::metadata::types::RefKind::Array { gc_ref, .. } => GcRef::gen_age(gc_ref),
                crate::metadata::types::RefKind::Field { gc_ref, .. } => GcRef::gen_age(gc_ref),
                crate::metadata::types::RefKind::Stack { .. } => 0,
            },
            // add-boxed-struct-identity (P4b): gen-age of the boxed struct's shared object.
            Value::BoxedStruct(gc) => GcRef::gen_age(gc),
            _ => 0,
        }
    }

    /// **add-generational-gc P2 (2026-05-22)**: mark phase for minor GC.
    ///
    /// Roots = pinned roots + external_root_scanner output + entries
    /// in dirty card chunks of both regions. The latter ensures any
    /// old object that has received a young-pointer write since the
    /// last major GC is treated as an additional root.
    ///
    /// BFS marks every reachable entry. When tracing children, only
    /// young children (gen_age < PROMOTION_THRESHOLD) are pushed to
    /// the queue. Old children are skipped — either they have no
    /// young descendants (otherwise they'd be in dirty cards via the
    /// barrier), or their old→young paths are seeded as separate
    /// dirty-card roots. This bounds minor mark work at O(young +
    /// |dirty-card entries|).
    fn mark_phase_minor(&self) -> usize {
        let threshold = super::region::PROMOTION_THRESHOLD;
        let mut queue: Vec<Value> = Vec::new();

        // Pinned roots + external scanner.
        queue.extend(self.inner.lock().roots.values().cloned());
        {
            let scanner = self.external_root_scanner.lock();
            if let Some(scan) = scanner.as_ref() {
                scan(&mut |v| queue.push(v.clone()));
            }
        }

        // Dirty card roots — all entries in dirty chunks of both regions.
        {
            let region = self.region_object.lock();
            region.iterate_dirty_cards(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                // SAFETY: handle came from iterate_dirty_cards; entry
                // is alive + generation matches at iteration time.
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                queue.push(Value::Object(gc));
            });
        }
        {
            let region = self.region_array.lock();
            region.iterate_dirty_cards(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                queue.push(Value::Array(gc));
            });
        }

        let mut marked = 0usize;
        while let Some(v) = queue.pop() {
            let just_marked = Self::mark_if_unmarked(&v);
            if !just_marked { continue; }
            marked += 1;

            v.trace_children(&mut |child| {
                // Only enqueue young children. Old children that need
                // re-rooting are already covered via dirty cards.
                if Self::gen_age_of(child) < threshold {
                    queue.push(child.clone());
                }
            });
        }
        marked
    }

    /// **add-generational-gc P2 (2026-05-22)**: sweep phase for minor GC.
    ///
    /// Walks `young_list` in both regions; for each entry:
    /// - `is_marked == true` → clear mark, increment gen_age (promote
    ///   to next age tier); if reaches threshold, region.promote()
    ///   removes from young_list.
    /// - `is_marked == false` → fire finalizer, tombstone (alive=false,
    ///   generation++, push to free_list AND remove from young_list).
    ///
    /// Old entries are NOT visited — major GC handles them.
    /// card_dirty is NOT cleared by minor (stable old→young refs need
    /// to keep their cards dirty until major scans them).
    fn sweep_phase_young_only(&self) -> u64 {
        let mut freed_bytes: u64 = 0;

        // Object region
        let mut tombstones_object: Vec<(super::region::RegionHandle, Option<FinalizerFn>, u64)> = Vec::new();
        let mut survivors_object: Vec<super::region::RegionHandle> = Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_young(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                    survivors_object.push(h);
                } else {
                    let size = {
                        let obj = entry.value.lock();
                        Self::script_object_size_estimate(&obj)
                    };
                    let fin = entry.finalizer.lock().take();
                    tombstones_object.push((h, fin, size));
                }
            });
        }
        // Promote survivors (may remove some from young_list at threshold).
        for h in survivors_object {
            self.region_object.lock().promote(h);
        }
        // Tombstone dead young entries.
        for (h, fin, size) in tombstones_object {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            {
                let region = self.region_object.lock();
                let entry = region.resolve(h);
                if entry.alive.load(std::sync::atomic::Ordering::Acquire) {
                    let mut obj = entry.value.lock();
                    for slot in obj.slots.iter_mut() {
                        *slot = Value::Null;
                    }
                }
            }
            self.region_object.lock().tombstone(h);
        }

        // Array region (parallel logic)
        let mut tombstones_array: Vec<(super::region::RegionHandle, Option<FinalizerFn>, u64)> = Vec::new();
        let mut survivors_array: Vec<super::region::RegionHandle> = Vec::new();
        {
            let region = self.region_array.lock();
            region.iterate_young(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                    survivors_array.push(h);
                } else {
                    let size = {
                        let arr = entry.value.lock();
                        Self::array_size_estimate(&arr)
                    };
                    let fin = entry.finalizer.lock().take();
                    tombstones_array.push((h, fin, size));
                }
            });
        }
        for h in survivors_array {
            self.region_array.lock().promote(h);
        }
        for (h, fin, size) in tombstones_array {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            {
                let region = self.region_array.lock();
                let entry = region.resolve(h);
                if entry.alive.load(std::sync::atomic::Ordering::Acquire) {
                    entry.value.lock().clear();
                }
            }
            self.region_array.lock().tombstone(h);
        }

        freed_bytes
    }

    /// **add-generational-gc P2 (2026-05-22)**: full minor GC cycle.
    /// Mark phase (young + dirty cards) → sweep phase (young only) →
    /// returns freed_bytes estimate. Card dirty bits are NOT cleared
    /// here — they accumulate across minors and are only cleared by
    /// the next major GC. This preserves correctness for stable
    /// old→young references (whose cards were dirtied at the time of
    /// the write but the target young object hasn't yet been promoted).
    fn run_cycle_collection_minor(&self) -> u64 {
        let _newly_marked = self.mark_phase_minor();
        self.sweep_phase_young_only()
    }

    /// **add-generational-gc P3 (2026-05-22)**: full major GC cycle.
    /// Same as `run_cycle_collection_stw` (mark whole heap from
    /// roots; sweep all entries) PLUS clears `card_dirty` at the end
    /// (cross-gen references are now fully traced; cards can reset
    /// for the next round of minors).
    fn run_cycle_collection_major(&self) -> u64 {
        let freed = self.run_cycle_collection_stw();
        // Major scanned the whole heap → cards no longer track
        // anything we don't already know. Clear so the next minor
        // starts with a fresh dirty set.
        self.region_object.lock().clear_card_dirty();
        self.region_array.lock().clear_card_dirty();
        freed
    }

    /// **add-generational-gc P3 (2026-05-22)**: escalation threshold.
    /// If the fraction of young entries surviving a minor GC exceeds
    /// this, the next collect is escalated to major immediately.
    /// Default 0.75 from [`RuntimeConfig::gc_minor_threshold`]; override
    /// via `Z42_GC_MINOR_THRESHOLD`.
    ///
    /// runtime-config-phase2 (2026-06-03): centralised through
    /// `crate::config::runtime_config()`; previous per-callsite
    /// `OnceLock<f32>` retired.
    #[allow(dead_code)] // wired in collect_cycles_with_context below
    fn minor_escalation_threshold() -> f32 {
        crate::config::runtime_config().gc_minor_threshold
    }

    /// **add-generational-gc P1 (2026-05-22)**: cross-gen detection
    /// helper for the write-barrier override. Marks the owner's chunk
    /// dirty when `owner.gen_age >= PROMOTION_THRESHOLD` (old) AND
    /// `new.gen_age < PROMOTION_THRESHOLD` (young).
    ///
    /// Same routine for both field + array_elem barriers — checks the
    /// owner Value's kind to pick the right region's card bitmap.
    /// Non-heap or stack-kind owners → no-op (no card to mark).
    fn maybe_mark_cross_gen_card(&self, owner: &Value, new: &Value) {
        let new_age = match new {
            Value::Object(gc) => GcRef::gen_age(gc),
            // add-boxed-struct-identity (P4b): a boxed struct is a shared region_object
            // entry — a young box stored into an old owner MUST mark the card, else it is
            // missed by minor GC and freed prematurely.
            Value::BoxedStruct(gc) => GcRef::gen_age(gc),
            Value::Array(gc)  => GcRef::gen_age(gc),
            Value::Closure(c) => GcRef::gen_age(&c.env),
            Value::Ref(kind) => match kind.as_ref() {
                crate::metadata::types::RefKind::Array { gc_ref, .. } => GcRef::gen_age(gc_ref),
                crate::metadata::types::RefKind::Field { gc_ref, .. } => GcRef::gen_age(gc_ref),
                crate::metadata::types::RefKind::Stack { .. } => return,
            },
            _ => return,
        };
        // Only old→young triggers a card. Young→young is in-young
        // scan already; old→old won't reach young.
        if new_age >= super::region::PROMOTION_THRESHOLD {
            return;
        }
        match owner {
            // add-boxed-struct-identity (P4b): a boxed struct owner is a region_object
            // entry too (reflection SetValue writes a ref leaf into its struct_refs).
            Value::Object(gc) | Value::BoxedStruct(gc) => {
                if GcRef::gen_age(gc) < super::region::PROMOTION_THRESHOLD { return; }
                // owner is old; mark its chunk in region_object dirty.
                let entry_ptr = gc.entry_ptr();
                // SAFETY: entry pointer valid for GcRef lifetime.
                let entry = unsafe { entry_ptr.as_ref() };
                let (ci, _) = entry.location;
                if ci != u16::MAX {
                    self.region_object.lock().mark_card_dirty(ci);
                }
            }
            Value::Array(gc) => {
                if GcRef::gen_age(gc) < super::region::PROMOTION_THRESHOLD { return; }
                let entry_ptr = gc.entry_ptr();
                let entry = unsafe { entry_ptr.as_ref() };
                let (ci, _) = entry.location;
                if ci != u16::MAX {
                    self.region_array.lock().mark_card_dirty(ci);
                }
            }
            _ => {} // non-heap owners — no card to mark
        }
    }

    /// **add-concurrent-gc P4a (2026-05-22)**: drain the gray-set
    /// (`mark_queue`) until empty. Trace each popped value's children
    /// and shade newly-discovered heap refs gray (mark + enqueue).
    ///
    /// **Termination invariant**: caller must ensure no new entries
    /// can be pushed concurrently before checking emptiness. In the
    /// concurrent path that means either:
    /// 1. Run during `ConcurrentMarking` phase — barriers + this drain
    ///    race; loop until both empty AND a final STW handshake
    ///    confirms no more writes can occur (P4b orchestrates this).
    /// 2. Run during `Marking` phase (handshake) — mutators parked,
    ///    no new barrier pushes possible, so emptiness is final.
    ///
    /// Returns the count of objects marked during this drain (useful
    /// for tests + diagnostics). 0 on already-empty queue.
    fn drain_mark_queue(&self) -> usize {
        let mut traced = 0usize;
        loop {
            // Take ownership of the current queue contents in one swap.
            // Mutators may push concurrently via barrier (under
            // ConcurrentMarking); we'll see those on the next iteration.
            let local: Vec<Value> = std::mem::take(&mut *self.mark_queue.lock());
            if local.is_empty() {
                break;
            }
            for v in &local {
                traced += 1;
                v.trace_children(&mut |child| {
                    if Self::mark_if_unmarked(child) {
                        self.mark_queue.lock().push(child.clone());
                    }
                });
            }
            // `local` drops here; any heap-ref values it held that are
            // also reachable elsewhere stay alive via those other refs.
        }
        traced
    }

    /// **add-concurrent-gc P4a (2026-05-22)**: end-to-end concurrent
    /// collect minus the safepoint phase transitions (P4b wires those
    /// in). Runs the steps that DON'T require a real VmContext: root
    /// snapshot → drain → sweep. Test-callable on a standalone
    /// `ArcMagrGC::new()` so we can verify algorithmic correctness
    /// (reachable chains preserved, unreachable cycles freed, barrier
    /// integration) before integrating with safepoint protocol.
    ///
    /// **NOT a production path**: production goes through P4b which
    /// adds STW pause coordination + handshake. Calling this without
    /// the surrounding pause is racy under real concurrent mutators —
    /// safe only for single-threaded test contexts that simulate
    /// mutator writes inline.
    #[cfg(test)]
    pub(crate) fn run_cycle_collection_concurrent_inline_for_test(&self) -> u64 {
        // Step 1: STW-equivalent root snapshot (no mutators in test).
        self.snapshot_roots_into_mark_queue();

        // Step 2: Drain queue (simulates "ConcurrentMarking" but
        // single-threaded — no real concurrency).
        let _traced = self.drain_mark_queue();

        // Step 3: Final residual drain (post-handshake equivalent —
        // catches anything pushed by barrier between roots snapshot
        // and now; in single-thread test no concurrent writes happen,
        // so this should be a no-op, but the loop is still here for
        // structural parity with P4b production flow).
        let _residual = self.drain_mark_queue();

        // Step 4: Sweep (STW; identical to STW path's sweep).
        self.sweep_phase()
    }

    /// **add-concurrent-gc P2 (2026-05-22)**: STW-phase root snapshot for
    /// the concurrent mark loop. Walks pinned roots + external root
    /// scanner output, marks each as gray (via `mark_if_unmarked`), and
    /// pushes newly-marked roots into `mark_queue`. The mark thread (P4)
    /// then drains the queue concurrently with mutators.
    ///
    /// Must be called under STW (during `GcPhase::Marking` between
    /// `request_gc_pause` and `set phase ConcurrentMarking`) so the
    /// snapshot is consistent — no mutator can add/remove roots between
    /// `pinned_roots` traversal and external scanner traversal.
    ///
    /// Returns the count of newly-marked root objects (for tests +
    /// diagnostics).
    fn snapshot_roots_into_mark_queue(&self) -> usize {
        let mut queue = self.mark_queue.lock();
        queue.clear();
        let mut count = 0usize;
        // Pinned roots — cloned under inner.lock() to release the lock
        // before any potential observer callbacks.
        let roots: Vec<Value> = self.inner.lock().roots.values().cloned().collect();
        for v in roots {
            if Self::mark_if_unmarked(&v) {
                queue.push(v);
                count += 1;
            }
        }
        // External root scanner (e.g. VmContext static_fields).
        let scanner_borrow = self.external_root_scanner.lock();
        if let Some(scan) = scanner_borrow.as_ref() {
            scan(&mut |v| {
                if Self::mark_if_unmarked(v) {
                    queue.push(v.clone());
                    count += 1;
                }
            });
        }
        count
    }

    /// Sweep phase of the mark-sweep collector.
    ///
    /// **add-mark-sweep-collector P3 (2026-05-21)**: original
    /// implementation walked the Arc-backed `heap_registry` snapshot.
    ///
    /// **add-custom-allocator P1 (2026-05-22)**: rewritten to walk
    /// regions directly:
    /// 1. For each alive entry in `region_object` + `region_array`:
    ///    - `marked == 1` → reset to 0 (next cycle ready), retain
    ///    - `marked == 0` → fire registered finalizer (one-shot take),
    ///      tombstone the entry (alive=false, generation++, push slot
    ///      to free list); break inner refs so any cyclic references
    ///      no longer count toward "iterate_live_objects" reachability
    ///
    /// Returns estimated `freed_bytes` (sum of `object_size_bytes` for
    /// tombstoned entries).
    ///
    /// Finalizer-timing contract (D3): firings happen here only. The
    /// `Std.GC.Finalize(x)` builtin (added by P2) provides a separate
    /// path for prompt resource release outside sweep.
    fn sweep_phase(&self) -> u64 {
        #[cfg(debug_assertions)]
        self.debug_stw_no_push.store(true, std::sync::atomic::Ordering::SeqCst);
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: sweep_phase entered with non-empty mark_queue ({q} items) — push happened between P5 drain and sweep start");
        }
        let mut freed_bytes: u64 = 0;

        // Object region.
        let mut tombstones_object: Vec<(super::region::RegionHandle, Option<FinalizerFn>, u64)> =
            Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_alive(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                } else {
                    // Estimate size before tombstoning (entry still readable).
                    let size = {
                        let obj = entry.value.lock();
                        Self::script_object_size_estimate(&obj)
                    };
                    let fin = entry.finalizer.lock().take();
                    tombstones_object.push((h, fin, size));
                }
            });
        }
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: mark_queue non-empty after object region scan ({q} items)");
        }
        // Fire finalizers + clear inner refs + tombstone.
        for (h, fin, size) in tombstones_object {
            if let Some(f) = fin { f(); }
            #[cfg(debug_assertions)]
            {
                let q = self.mark_queue.lock().len();
                assert_eq!(q, 0, "BUG: mark_queue non-empty after finalizer (h={:?}, {q} items)", h);
            }
            freed_bytes += size;
            // Break inner refs to release any cycles for the region's
            // bookkeeping (iterate_live_objects, future child traversal
            // won't see refs into already-tombstoned entries).
            //
            // SAFETY: handle came from iterate_alive; entry is still
            // accessible (alive=true at this point — we haven't
            // tombstoned yet).
            {
                let region = self.region_object.lock();
                let entry = region.resolve(h);
                if entry.alive.load(std::sync::atomic::Ordering::Acquire) {
                    let mut obj = entry.value.lock();
                    for slot in obj.slots.iter_mut() {
                        *slot = Value::Null;
                    }
                }
            }
            #[cfg(debug_assertions)]
            {
                let q = self.mark_queue.lock().len();
                assert_eq!(q, 0, "BUG: mark_queue non-empty after slot clearing (h={:?}, {q} items)", h);
            }
            self.region_object.lock().tombstone(h);
            #[cfg(debug_assertions)]
            {
                let q = self.mark_queue.lock().len();
                assert_eq!(q, 0, "BUG: mark_queue non-empty after tombstone (h={:?}, {q} items)", h);
            }
        }
        #[cfg(debug_assertions)]
        {
            let q = self.mark_queue.lock().len();
            assert_eq!(q, 0, "BUG: mark_queue non-empty after object tombstone loop ({q} items)");
        }

        // Array region.
        let mut tombstones_array: Vec<(super::region::RegionHandle, Option<FinalizerFn>, u64)> =
            Vec::new();
        {
            let region = self.region_array.lock();
            region.iterate_alive(|h, entry| {
                if entry.is_marked() {
                    entry.clear_mark();
                } else {
                    let size = {
                        let arr = entry.value.lock();
                        Self::array_size_estimate(&arr)
                    };
                    let fin = entry.finalizer.lock().take();
                    tombstones_array.push((h, fin, size));
                }
            });
        }
        for (h, fin, size) in tombstones_array {
            if let Some(f) = fin { f(); }
            freed_bytes += size;
            {
                let region = self.region_array.lock();
                let entry = region.resolve(h);
                if entry.alive.load(std::sync::atomic::Ordering::Acquire) {
                    entry.value.lock().clear();
                }
            }
            self.region_array.lock().tombstone(h);
        }

        #[cfg(debug_assertions)]
        self.debug_stw_no_push.store(false, std::sync::atomic::Ordering::SeqCst);
        freed_bytes
    }

    /// Size estimate helpers for sweep_phase — operate on already-
    /// locked inner data (avoids re-locking via object_size_bytes path).
    fn script_object_size_estimate(obj: &ScriptObject) -> u64 {
        use std::mem::size_of;
        // slots is `Box<[Value]>` — len == actual allocation (no excess
        // capacity to charge separately).
        (size_of::<Value>() + size_of::<ScriptObject>()
            + obj.slots.len() * size_of::<Value>()) as u64
    }

    fn array_size_estimate(arr: &crate::metadata::types::ArrayObj) -> u64 {
        use std::mem::size_of;
        // packed-primitive-arrays: `elem_storage_bytes` is per-backing (byte[] 1B
        // vs Boxed 24B/elem), so packed arrays report their true smaller size.
        (size_of::<Value>() + size_of::<crate::metadata::types::ArrayObj>()
            + arr.elem_storage_bytes()) as u64
    }

    /// **add-mark-sweep-collector P3 (2026-05-21)**: test-only entry
    /// point that exposes the full mark+sweep cycle for unit tests in
    /// `arc_heap_tests::mark_phase`. Production code goes through
    /// `collect_cycles` → `run_cycle_collection`, which calls the same
    /// two phases.
    #[cfg(test)]
    fn collect_cycles_mark_sweep_for_test(&self) -> u64 {
        let _newly_marked = self.mark_phase();
        self.sweep_phase()
    }

    /// **add-mark-sweep-collector P3 (2026-05-21)**: clear all mark bits.
    /// Test-only — production sweep resets marks on survivors inline.
    /// Walks the registry via the existing snapshot upgrade path (so dead
    /// WeakRefs are skipped naturally). Makes mark_phase tests idempotent
    /// across runs.
    #[cfg(test)]
    fn reset_marks_for_test(&self) {
        for v in self.snapshot_live_from_registry() {
            match &v {
                Value::Object(gc) => GcRef::clear_mark(gc),
                Value::Array(gc)  => GcRef::clear_mark(gc),
                _ => {}
            }
        }
    }

    /// Cycle collection — mark-sweep.
    ///
    /// 1. **Mark**：BFS from pinned roots + external scanner, setting
    ///    `marked = 1` on every reachable `GcAllocation`.
    /// 2. **Sweep**：snapshot live objects from registry; reset marks on
    ///    survivors, break internal refs of unmarked allocations so that
    ///    when the snapshot `Vec` drops the Arc strong counts can reach
    ///    zero and chain-drop fires finalizers.
    ///
    /// Returns the estimated `freed_bytes` (sum of `object_size_bytes`
    /// for broken cycle nodes).
    ///
    /// **add-mark-sweep-collector P3 (2026-05-21)**: replaced the
    /// previous trial-deletion (Bacon-Rajan simplified) implementation.
    /// O(N²) → O(reachable). The pure tracing contract: Rust-local
    /// `Value` strong refs are NOT roots — embedders must `pin_root`
    /// anything they want preserved across collect.
    ///
    /// **add-concurrent-gc P0 (2026-05-22)**: dispatches on `self.mode()`.
    /// Both arms currently route to the STW path; P4 fills in the
    /// concurrent arm with `run_cycle_collection_concurrent`.
    fn run_cycle_collection(&self) -> u64 {
        match self.mode() {
            super::GcMode::StwMarkSweep => self.run_cycle_collection_stw(),
            super::GcMode::ConcurrentMarkSweep => {
                // P0 stub: concurrent arm currently routes to STW path —
                // proves dispatch wiring without changing behavior. P4
                // replaces this with `run_cycle_collection_concurrent`.
                self.run_cycle_collection_stw()
            }
            super::GcMode::GenerationalMarkSweep => {
                // add-generational-gc P2: minor GC by default. Major
                // GC requires the VmContext-aware entry
                // (`collect_cycles_with_context`) for pause coord;
                // direct callers of `collect_cycles` (which go through
                // `force_collect`) get a minor cycle here. Major is
                // P3's expansion (auto-collect young pressure trigger
                // + escalation heuristic).
                self.run_cycle_collection_minor()
            }
        }
    }

    /// STW mark-sweep collect — the proven path. Called directly when
    /// `mode() == StwMarkSweep`, or as a fallback by the concurrent path.
    ///
    /// **add-gc-stress-test (2026-05-22)**: defensive cleanup at the
    /// boundaries. The concurrent barrier override (under
    /// `GcMode::ConcurrentMarkSweep`) leaves `marked = 1` on shaded
    /// objects + entries on `mark_queue`. The no-context `force_collect`
    /// path falls back to STW, which assumes a clean slate at start
    /// (all marks 0, queue empty). Without clearing, mark_phase
    /// observes pre-marked entries — CAS fails → `just_marked == false`
    /// → children NOT traced. Sweep then retains those entries
    /// (marked=1) even though they may be unreachable, AND their
    /// children (pointed to via slots) may be unmarked and swept,
    /// leaving stale Values inside slots → next collect's mark BFS
    /// hits entry_ref panic (use-after-finalize). Caught by stress
    /// test + C1 validator.
    fn run_cycle_collection_stw(&self) -> u64 {
        // Defensive reset: ensure clean state for STW mark.
        self.reset_all_marks_in_regions();
        self.mark_queue.lock().clear();
        let _newly_marked = self.mark_phase();
        // **add-gc-softref (2026-05-26)**: revive soft-ref targets that
        // are unmarked but below the pressure threshold.
        self.revive_soft_refs();
        // **add-lazy-context-unload (2026-08-05)**: after mark (marks set),
        // before sweep (which clears them), scan marked objects for retained
        // collectible contexts. Gated on `is_unloading` → zero cost normally.
        let ctx_snapshot = {
            let g = self.context_reclaimer.lock();
            match g.as_ref() {
                Some(r) if r.is_unloading() => Some(r.snapshot()),
                _ => None,
            }
        };
        let live_contexts = ctx_snapshot.as_ref().map(|s| self.scan_marked_contexts(s));
        let freed = self.sweep_phase();
        // Reclaim Unloading contexts with no live references (post-sweep, STW).
        if let Some(live) = live_contexts {
            if let Some(r) = self.context_reclaimer.lock().as_ref() {
                r.reclaim(&live);
            }
        }
        // Prune dead soft-ref entries after sweep.
        self.inner.lock().soft_registry.prune_dead();
        freed
    }

    /// **add-lazy-context-unload (2026-08-05)**: after mark, walk marked
    /// `ScriptObject`s and resolve which collectible contexts they retain
    /// (instance `type_desc` ptr + reflection native handles). Only called when
    /// an unload is in flight. Marks are still set (sweep clears them next).
    fn scan_marked_contexts(
        &self,
        snap: &crate::metadata::context::ContextLiveness,
    ) -> std::collections::HashSet<crate::metadata::context::ContextId> {
        let mut live = std::collections::HashSet::new();
        if snap.is_empty() {
            return live;
        }
        let region = self.region_object.lock();
        region.iterate_alive(|_h, entry| {
            if entry.is_marked() {
                let obj = entry.value.lock();
                let td_ptr = std::sync::Arc::as_ptr(&obj.type_desc) as usize;
                if let Some(cid) = snap.retained_context(td_ptr, &obj.native) {
                    live.insert(cid);
                }
            }
        });
        live
    }

    /// **add-heap-retention-diagnostics (2026-08-06)**: build the reverse
    /// reference graph over the live heap (object + array regions) + categorized
    /// roots. Callers `force_collect()` first so only reachable objects remain.
    fn build_retention_graph(&self) -> super::retention::RetentionGraph {
        use super::retention::{RetainerInfo, RetainerKind};
        let mut g = super::retention::RetentionGraph::new();

        // Object region → each object's heap-ref slots become reverse edges.
        {
            let region = self.region_object.lock();
            region.iterate_alive(|_h, entry| {
                let self_ptr = entry.value.data_ptr() as usize;
                let obj = entry.value.lock();
                let type_name = obj.type_desc.name.clone();
                for slot in obj.slots.iter() {
                    if let Some(child) = value_heap_ptr(slot) {
                        g.add_edge(
                            child,
                            RetainerInfo { kind: RetainerKind::Object, type_name: type_name.clone(), id: self_ptr },
                        );
                    }
                }
            });
        }
        // Array region → each array's heap-ref elements become reverse edges.
        {
            let region = self.region_array.lock();
            region.iterate_alive(|_h, entry| {
                let self_ptr = entry.value.data_ptr() as usize;
                let arr = entry.value.lock();
                let type_name = format!("{}[]", &*arr.element_type);
                for elem in arr.iter_boxed() {
                    if let Some(child) = value_heap_ptr(&elem) {
                        g.add_edge(
                            child,
                            RetainerInfo { kind: RetainerKind::Array, type_name: type_name.clone(), id: self_ptr },
                        );
                    }
                }
            });
        }
        // Pinned roots (GC-internal host pins / frame pins).
        {
            let inner = self.inner.lock();
            for v in inner.roots.values() {
                if let Some(obj) = value_heap_ptr(v) {
                    g.add_root_edge(obj, super::retention::RootKind::Pinned);
                }
            }
        }
        // Categorized VmCore roots (static fields / stack frames / func-ref slots).
        if let Some(scan) = self.categorized_root_scanner.lock().as_ref() {
            scan(&mut |v, kind| {
                if let Some(obj) = value_heap_ptr(v) {
                    g.add_root_edge(obj, kind);
                }
            });
        }
        g
    }

    /// **add-gc-softref (2026-05-26)**: after mark_phase, re-mark alive
    /// soft-ref targets when heap pressure < `Z42_GC_SOFT_THRESHOLD`.
    /// Snapshots the registry entries under the lock, then calls
    /// `revive_if_unmarked` outside the lock (only touches RegionEntry
    /// atomics — no heap lock required).
    fn revive_soft_refs(&self) {
        let (entries, used_bytes, max_bytes) = {
            let inner = self.inner.lock();
            let entries = inner.soft_registry.snapshot_entries();
            let used = inner.stats.used_bytes;
            let max  = inner.stats.max_bytes.unwrap_or(0);
            (entries, used, max)
        };
        // revive_pass on snapshot — no lock held; only atomic field access.
        let _ = super::soft_registry::SoftRegistry::revive_snapshot(&entries, used_bytes, max_bytes);
    }

    /// **add-gc-stress-test (2026-05-22)**: clear `marked` on every
    /// alive entry across both regions. Used by
    /// `run_cycle_collection_stw` to guarantee mark-bit clean slate
    /// when starting a STW cycle. Idempotent.
    fn reset_all_marks_in_regions(&self) {
        self.region_object.lock().iterate_alive(|_h, e| e.clear_mark());
        self.region_array.lock().iterate_alive(|_h, e| e.clear_mark());
    }

    /// Snapshot all alive Values across the heap's regions. Order:
    /// object region first, then array region. Each entry visited
    /// exactly once (no de-dup required — regions are the authoritative
    /// store, every entry there represents one allocation).
    ///
    /// **add-custom-allocator P1 (2026-05-22)**: replaces the
    /// heap_registry-walking version. No more `Weak::upgrade` per
    /// entry; just a linear chunks walk with an alive-bit check.
    fn snapshot_live_from_registry(&self) -> Vec<Value> {
        let mut alive: Vec<Value> = Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_alive(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                // SAFETY: handle came from iterate_alive over a live entry;
                // generation matches the entry's current state.
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                alive.push(Value::Object(gc));
            });
        }
        {
            let region = self.region_array.lock();
            region.iterate_alive(|h, entry| {
                let entry_ptr = std::ptr::NonNull::from(entry);
                let gc = unsafe { GcRef::from_region_entry(entry_ptr, h.generation) };
                alive.push(Value::Array(gc));
            });
        }
        alive
    }

    /// **Phase 3-OOM**: 检查在当前 used_bytes 基础上再分配 `size` 字节是否会
    /// 越过 max_heap_bytes 上限。仅在 strict_oom 模式下使用。
    fn would_oom_after_alloc(&self, size: u64) -> (bool, u64) {
        let i = self.inner.lock();
        if !i.strict_oom { return (false, 0); }
        let Some(limit) = i.stats.max_bytes else { return (false, 0); };
        let after = i.stats.used_bytes.saturating_add(size);
        (after > limit, limit)
    }

    /// **Phase 3d**: 内存压力下自动触发 collect_cycles。
    ///
    /// 条件：
    /// - max_bytes 已设
    /// - used >= 90% limit
    /// - 距上次 auto-collect 增长 >= 10% limit（throttle，避免每次 alloc 都 collect）
    /// - pause_count == 0
    ///
    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: 当 `external_needs_collect`
    /// flag 装上时（VmCore 构造后 wire），仅 `flag.store(true, Release)` —
    /// 实际 collect 延迟到下一次 mutator 走 `check_safepoint(ctx)` 时由该 mutator
    /// 在 safepoint guard 内执行，避免多线程下 scanner 与 mutator regs 写读 race。
    /// 当 flag 未装（GC 单测直接 `ArcMagrGC::new()` 路径）→ fallback 回原
    /// inline collect，保持单线程现有行为零变化。
    fn maybe_auto_collect(&self) {
        let (used, max_opt, last, paused) = {
            let i = self.inner.lock();
            (i.stats.used_bytes, i.stats.max_bytes, i.last_auto_collect_used, i.pause_count > 0)
        };
        if paused { return; }
        let Some(limit) = max_opt else { return };
        let near_threshold = (limit as f64 * 0.9) as u64;
        if used < near_threshold { return; }
        let throttle_delta = (limit as f64 * 0.1) as u64;
        if used.saturating_sub(last) < throttle_delta { return; }
        // Mark this as the "last seen used" pre-collect so we don't re-trip
        // on every subsequent alloc until the collect actually runs.
        self.inner.lock().last_auto_collect_used = used;

        // Defer to safepoint when wired (multi-thread safe path).
        if let Some(flag) = self.external_needs_collect.lock().clone() {
            flag.store(true, std::sync::atomic::Ordering::Release);
            return;
        }
        // Fallback: legacy inline collect — preserves GC unit-test behaviour
        // (those tests construct ArcMagrGC::new() without VmCore wiring).
        self.collect_cycles();
    }

    /// **Phase 3d**: collect 完成后，若 used 已降到 90% 阈值以下，
    /// reset `near_limit_warned` 让下次跨阈值能再发 NearHeapLimit 事件。
    fn maybe_reset_near_limit_warned(&self) {
        let mut i = self.inner.lock();
        let Some(limit) = i.stats.max_bytes else { return };
        let near_threshold = (limit as f64 * 0.9) as u64;
        if i.stats.used_bytes < near_threshold {
            i.near_limit_warned = false;
        }
    }

    fn check_pressure(&self, requested: u64) {
        let (used, max, near_warned) = {
            let i = self.inner.lock();
            (i.stats.used_bytes, i.stats.max_bytes, i.near_limit_warned)
        };
        let Some(limit) = max else { return };
        let near_threshold     = (limit as f64 * 0.9 ) as u64;
        let pressure_threshold = (limit as f64 * 0.75) as u64;

        if !near_warned && used >= near_threshold {
            self.inner.lock().near_limit_warned = true;
            self.fire_event(GcEvent::NearHeapLimit {
                used_bytes: used, limit_bytes: limit,
            });
        } else if used >= pressure_threshold && used < near_threshold {
            self.fire_event(GcEvent::AllocationPressure {
                used_bytes: used, limit_bytes: limit,
            });
        }

        if used > limit {
            self.fire_event(GcEvent::OutOfMemory {
                requested_bytes: requested, limit_bytes: limit,
            });
        }
    }
}

// ── inherent helpers ─────────────────────────────────────────────────────────

impl ArcMagrGC {
    /// add-reflection-array-element-type: shared array allocation over an
    /// `ArrayObj` (element type + elems). Both `alloc_array` (untyped) and
    /// `alloc_array_typed` funnel through here.
    fn alloc_array_obj(&self, obj: crate::metadata::types::ArrayObj) -> Value {
        let elem_count = obj.len();
        let (entry_ptr, generation, handle) = {
            let mut region = self.region_array.lock();
            let handle = region.alloc(obj);
            let entry: std::ptr::NonNull<super::region::RegionEntry<crate::metadata::types::ArrayObj>> =
                std::ptr::NonNull::from(region.resolve(handle));
            (entry, handle.generation, handle)
        };
        let gc = unsafe { GcRef::from_region_entry(entry_ptr, generation) };
        let value = Value::Array(gc);

        let size = self.object_size_bytes(&value);
        let (would_oom, limit) = self.would_oom_after_alloc(size as u64);
        if would_oom {
            self.region_array.lock().tombstone(handle);
            self.fire_event(GcEvent::OutOfMemory {
                requested_bytes: size as u64,
                limit_bytes: limit,
            });
            return Value::Null;
        }
        self.record_alloc(&value, || AllocKind::Array { elem_count }, size);
        self.maybe_auto_collect();
        value
    }
}

// ── trait impl ───────────────────────────────────────────────────────────────

impl MagrGC for ArcMagrGC {
    // ── 2. Roots / scanner ───────────────────────────────────────────────────

    /// **Phase 3d.1** + **add-multithreading-foundation Phase 2.2**：
    /// 注册 external root scanner 闭包。每次 cycle collection mark 阶段在
    /// 扫完 pinned roots 后调用，把闭包 yield 的 Value 也加入 reachable BFS。
    ///
    /// 重复调用覆盖之前的 scanner；传 no-op 闭包等价于卸载。
    fn set_external_root_scanner(&self, scanner: ExternalRootScanner) {
        *self.external_root_scanner.lock() = Some(scanner);
    }

    /// **add-lazy-context-unload (2026-08-05)**: wire the collectible-context
    /// reclaimer. Drives `Unloading` context reclamation on major STW collects.
    fn set_context_reclaimer(&self, hook: ContextReclaimHook) {
        *self.context_reclaimer.lock() = Some(hook);
    }

    /// **add-heap-retention-diagnostics (2026-08-06)**: wire the categorized
    /// root scanner (for `retention_roots` L2 category-level root reporting).
    fn set_categorized_root_scanner(&self, scanner: CategorizedRootScanner) {
        *self.categorized_root_scanner.lock() = Some(scanner);
    }

    /// **L1**: direct heap referrers of `target` (a heap object's data ptr).
    /// Forces a full GC first so only reachable objects are reported.
    fn retention_direct_referrers(&self, target: usize) -> Vec<super::retention::RetainerInfo> {
        self.force_collect();
        self.build_retention_graph().direct_referrers(target)
    }

    /// **L2**: GC roots retaining `target` (category-level), via reverse BFS.
    /// Forces a full GC first for accuracy.
    fn retention_roots(&self, target: usize) -> Vec<super::retention::RootInfo> {
        self.force_collect();
        self.build_retention_graph().retaining_roots(target)
    }

    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: wire the
    /// AtomicBool that `maybe_auto_collect` should set on pressure trip
    /// (deferring the actual collect to the next safepoint).
    fn set_external_needs_collect_flag(&self, flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        *self.external_needs_collect.lock() = Some(flag);
    }

    // ── 1. Allocation ────────────────────────────────────────────────────────

    fn alloc_object(
        &self,
        type_desc: Arc<TypeDesc>,
        slots: Vec<Value>,
        native: NativeData,
    ) -> Value {
        // Keep a cheap `Arc<TypeDesc>` handle (atomic refcount, no String alloc)
        // so `record_alloc`'s closure can clone the class name lazily — only when
        // an alloc sampler is installed, not on every `new`.
        let td_for_record = Arc::clone(&type_desc);
        // review.md E2.P6 (2026-06-02): `slots` is `Box<[Value]>` now.
        // Vec → Box<[T]> drops excess capacity; for the common case where
        // the caller pre-sized to `TypeDesc.fields.len()` this is a zero
        // realloc shrink-to-fit.
        let (struct_bytes, struct_refs) = type_desc.inline_regions();
        let obj = ScriptObject {
            type_desc,
            slots: slots.into_boxed_slice(),
            struct_bytes,
            struct_refs,
            native,
            type_args: Box::new([]),
        };

        // **add-custom-allocator P1 (2026-05-22)**: alloc into region.
        // Region::alloc returns a stable handle; resolve gives us the
        // entry pointer for GcRef construction.
        let (entry_ptr, generation, handle) = {
            let mut region = self.region_object.lock();
            let handle = region.alloc(obj);
            let entry: std::ptr::NonNull<super::region::RegionEntry<ScriptObject>> =
                std::ptr::NonNull::from(region.resolve(handle));
            (entry, handle.generation, handle)
        };
        // SAFETY: handle was just produced by region.alloc; entry ptr
        // is stable for entry lifetime; generation matches.
        let gc = unsafe { GcRef::from_region_entry(entry_ptr, generation) };
        let value = Value::Object(gc);

        let size = self.object_size_bytes(&value);
        // Phase 3-OOM: strict 模式下若 alloc 后会越界，撤销并返 Null
        let (would_oom, limit) = self.would_oom_after_alloc(size as u64);
        if would_oom {
            // Refund: tombstone the entry (no finalizer registered yet,
            // so no fire on tombstone).
            self.region_object.lock().tombstone(handle);
            self.fire_event(GcEvent::OutOfMemory {
                requested_bytes: size as u64,
                limit_bytes: limit,
            });
            return Value::Null;
        }
        self.record_alloc(&value, || AllocKind::Object { class: td_for_record.name.clone() }, size);
        self.maybe_auto_collect();
        value
    }

    fn alloc_array(&self, elems: Vec<Value>) -> Value {
        self.alloc_array_obj(crate::metadata::types::ArrayObj::new(elems))
    }

    fn alloc_array_typed(&self, element_type: &str, elems: Vec<Value>) -> Value {
        self.alloc_array_obj(crate::metadata::types::ArrayObj::typed(element_type, elems))
    }

    /// add-struct-array-codegen (P3b follow-up): `Heap`-trait override so dyn-dispatched
    /// `ctx.heap().alloc_array_obj(..)` region-allocs the pre-built `ArrayObj` (keeping a
    /// `StructBytes` backing), not the trait default that would box it. Routes to the
    /// inherent `ArcMagrGC::alloc_array_obj` (fully-qualified — no recursion/ambiguity).
    fn alloc_array_obj(&self, obj: crate::metadata::types::ArrayObj) -> Value {
        ArcMagrGC::alloc_array_obj(self, obj)
    }

    fn alloc_bytes(&self, bytes: Vec<u8>) -> Value {
        self.alloc_array_obj(crate::metadata::types::ArrayObj::from_bytes(bytes))
    }

    // ── 2. Roots ─────────────────────────────────────────────────────────────

    fn pin_root(&self, value: Value) -> RootHandle {
        let mut i = self.inner.lock();
        let handle = RootHandle(i.next_root_id);
        i.next_root_id += 1;
        i.roots.insert(handle, value);
        i.stats.roots_pinned += 1;
        if let Some(pins) = i.frame_pins.last_mut() {
            pins.push(handle);
        }
        handle
    }

    fn unpin_root(&self, handle: RootHandle) {
        let mut i = self.inner.lock();
        if i.roots.remove(&handle).is_some() {
            i.stats.roots_pinned = i.stats.roots_pinned.saturating_sub(1);
        }
    }

    fn enter_frame(&self) -> FrameMark {
        let mut i = self.inner.lock();
        let depth = i.frame_pins.len() as u32;
        i.frame_pins.push(Vec::new());
        FrameMark(depth)
    }

    fn leave_frame(&self, mark: FrameMark) {
        let mut i = self.inner.lock();
        while i.frame_pins.len() as u32 > mark.0 {
            if let Some(pins) = i.frame_pins.pop() {
                for h in pins {
                    if i.roots.remove(&h).is_some() {
                        i.stats.roots_pinned = i.stats.roots_pinned.saturating_sub(1);
                    }
                }
            }
        }
    }

    fn for_each_root(&self, visitor: &mut dyn FnMut(&Value)) {
        let i = self.inner.lock();
        for v in i.roots.values() {
            visitor(v);
        }
    }

    // ── 3. Write barriers ────────────────────────────────────────────────────
    //
    // **add-write-barriers (2026-05-21)**: ArcMagrGC overrides the trait
    // methods. Production STW-mode is no-op (matches the pre-this-spec
    // baseline); `#[cfg(test)]` always fires the test observer regardless
    // of mode.
    //
    // **add-concurrent-gc P3 (2026-05-22)**: under `ConcurrentMarkSweep`
    // mode, the override implements tricolor incremental update —
    // shade new heap-ref writes gray (mark + enqueue). Mark thread (P4)
    // drains the queue.
    //
    // Caller contract: invoke ONLY when `new.is_heap_ref() == true`
    // (Decision 1 of add-write-barriers). Override `debug_assert!`s the
    // contract under concurrent mode (where the assertion is load-bearing
    // for correctness — a primitive write incorrectly dispatched here
    // would silently no-op since `mark_if_unmarked` returns false on
    // primitives, but the contract violation should be caught).

    #[allow(unused_variables)]
    fn write_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        #[cfg(test)]
        self.fire_barrier_field(owner, slot, new);

        match self.mode() {
            super::GcMode::StwMarkSweep => {} // no-op (production default)
            super::GcMode::ConcurrentMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_field caller must filter primitives via Value::is_heap_ref"
                );
                if Self::mark_if_unmarked(new) {
                    #[cfg(debug_assertions)]
                    debug_assert!(
                        !self.debug_stw_no_push.load(std::sync::atomic::Ordering::SeqCst),
                        "BUG: write_barrier_field pushing to mark_queue while debug_stw_no_push=true (STW sweep is active!) — thread {:?}",
                        std::thread::current().id()
                    );
                    self.mark_queue.lock().push(new.clone());
                }
            }
            super::GcMode::GenerationalMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_field caller must filter primitives via Value::is_heap_ref"
                );
                // **add-generational-gc P1 (2026-05-22)**: cross-gen
                // detection. If owner is old (gen_age >= threshold)
                // AND new is young (gen_age < threshold), the owner's
                // chunk gets card-dirtied so the upcoming minor GC
                // re-roots from that chunk (the young target would
                // otherwise be missed).
                self.maybe_mark_cross_gen_card(owner, new);
            }
        }
    }

    #[allow(unused_variables)]
    fn write_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
        #[cfg(test)]
        self.fire_barrier_array_elem(arr, idx, new);

        match self.mode() {
            super::GcMode::StwMarkSweep => {}
            super::GcMode::ConcurrentMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_array_elem caller must filter primitives via Value::is_heap_ref"
                );
                if Self::mark_if_unmarked(new) {
                    self.mark_queue.lock().push(new.clone());
                }
            }
            super::GcMode::GenerationalMarkSweep => {
                debug_assert!(
                    new.is_heap_ref(),
                    "write_barrier_array_elem caller must filter primitives via Value::is_heap_ref"
                );
                // add-generational-gc P1: same cross-gen check.
                self.maybe_mark_cross_gen_card(arr, new);
            }
        }
    }

    // ── 4. Object Model ──────────────────────────────────────────────────────

    fn object_size_bytes(&self, value: &Value) -> usize {
        use std::mem::size_of;
        match value {
            Value::Null | Value::Bool(_) | Value::Char(_)
            | Value::I64(_) | Value::F64(_) => size_of::<Value>(),
            Value::Str(s) => size_of::<Value>() + s.len(),
            Value::Array(rc) => {
                size_of::<Value>() + size_of::<Vec<Value>>()
                    + rc.borrow().elem_storage_bytes()
            }
            Value::Object(rc) => {
                let obj = rc.borrow();
                // slots is `Box<[Value]>` — len == actual allocation.
                // add-struct-heap-inline (P3b): + inline struct byte region + ref side-table.
                size_of::<Value>() + size_of::<ScriptObject>()
                    + obj.slots.len() * size_of::<Value>()
                    + obj.struct_bytes.len()
                    + obj.struct_refs.len() * size_of::<Value>()
            }
            // Spec C4: PinnedView holds raw ptr+len; the borrowed buffer
            // itself is owned by the source `Value::Str` / `Value::Array`,
            // not by the view. Charge only the discriminant + scalars.
            Value::PinnedView(_) => size_of::<Value>() + size_of::<crate::metadata::PinnedViewData>(),
            // impl-lambda-l2: FuncRef holds the function name; no managed heap
            // allocation beyond the string buffer.
            Value::FuncRef(name) => size_of::<Value>() + name.len(),
            // impl-closure-l3-core: Closure carries a heap-allocated env (Vec<Value>);
            // its size is the env's storage plus the function-name string.
            Value::Closure(c) => {
                size_of::<Value>()
                    + size_of::<crate::metadata::ClosureData>()
                    + size_of::<Vec<Value>>()
                    + c.env.borrow().elem_storage_bytes()
                    + c.fn_name.capacity()
            }
            // impl-closure-l3-escape-stack: StackClosure 的 env 在创建 frame 的
            // env_arena 中，由 frame 拥有；本 Value 自身只携带 idx + fn_name。
            // GC 不为 arena 内存分配 / 释放负责（frame Drop 自动处理）。
            Value::StackClosure(sc) => {
                size_of::<Value>() + size_of::<crate::metadata::StackClosureData>() + sc.fn_name.capacity()
            }
            // Spec impl-ref-out-in-runtime: Ref 仅存索引或 GcRef；底层
            // Vec/Object 已被本身的 Value::Array / Value::Object 计入，
            // Ref 只额外计入自己的 enum tag + RefKind 数据。
            Value::Ref(kind) => match kind.as_ref() {
                crate::metadata::types::RefKind::Stack { .. } => size_of::<Value>(),
                crate::metadata::types::RefKind::Array { .. } => size_of::<Value>(),
                crate::metadata::types::RefKind::Field { field_name, .. } =>
                    size_of::<Value>() + field_name.capacity(),
            },
            // add-primitive-value-boxing: 装箱基元 = enum tag + 堆 BoxedPrim（class Arc + inner）。
            Value::Boxed(b) => size_of::<Value>()
                + size_of::<crate::metadata::types::BoxedPrim>()
                + b.class.len(),
            // add-escape-analysis-stack-alloc: stack objects/arrays live in the
            // per-context arena, not the GC heap — object_size_bytes is a heap-alloc
            // accounting hook and is never called on them; the arm exists only for
            // exhaustiveness. The handle itself is just a (idx, frame_id) pair.
            Value::StackObject { .. } | Value::StackArray { .. } => size_of::<Value>(),
            // add-struct-value-semantics: struct blob lives in the per-context
            // struct arena, not the GC heap — same as stack handles. The handle
            // itself is just an (idx, frame_id) pair.
            Value::StructRef { .. } => size_of::<Value>(),
            // add-boxed-struct-identity (P4b, 路 B2): boxed struct 现是共享 `ScriptObject`——按对象
            // 计其 struct_bytes/struct_refs（与 Object 臂同）。对象本体在 region_object，alloc 时已计一次；
            // 此臂给按 Value 计尺寸的诚实值。
            Value::BoxedStruct(gc) => {
                let obj = gc.borrow();
                size_of::<Value>() + size_of::<ScriptObject>()
                    + obj.slots.len() * size_of::<Value>()
                    + obj.struct_bytes.len()
                    + obj.struct_refs.len() * size_of::<Value>()
            }
            // add-struct-heap-inline (P3b): a struct[] element handle — the backing
            // array is accounted via its own Value::Array; charge only the boxed handle.
            Value::StructRefHeap(_) => size_of::<Value>()
                + size_of::<crate::metadata::types::StructArrayElem>(),
        }
    }

    fn scan_object_refs(&self, value: &Value, visitor: &mut dyn FnMut(&Value)) {
        match value {
            Value::Object(rc) => {
                let obj = rc.borrow();
                for slot in &obj.slots { visitor(slot); }
                // add-struct-heap-inline (P3b): reference leaves of inline struct
                // fields live in the `struct_refs` side-table (D1-a), scanned like
                // any object field. Empty for classes with no inline struct fields.
                for r in &obj.struct_refs { visitor(r); }
            }
            Value::Array(rc) => {
                let arr = rc.borrow();
                for elem in arr.gc_refs() { visitor(elem); }  // add-struct-heap-inline (P3b): incl struct[] refs
            }
            // impl-closure-l3-core: a closure's env owns Value slots that may
            // contain Object/Array refs; scan them so reachable closures keep
            // their captured objects alive.
            Value::Closure(c) => {
                let arr = c.env.borrow();
                for elem in arr.gc_refs() { visitor(elem); }
            }
            // Spec impl-ref-out-in-runtime: Ref::Array / Ref::Field 持 GcRef，
            // GC 必须跟随让 caller 数组 / 对象在调用期间不被回收。
            // Stack kind 不持 GcRef（frame 在调用栈上自然存活）。
            Value::Ref(kind) => match kind.as_ref() {
                crate::metadata::types::RefKind::Stack { .. } => {}
                crate::metadata::types::RefKind::Array { gc_ref, .. } => {
                    let arr = gc_ref.borrow();
                    for elem in arr.gc_refs() { visitor(elem); }
                }
                crate::metadata::types::RefKind::Field { gc_ref, .. } => {
                    let obj = gc_ref.borrow();
                    for slot in &obj.slots { visitor(slot); }
                    for r in &obj.struct_refs { visitor(r); }  // add-struct-heap-inline (P3b)
                }
            },
            // add-boxed-struct-identity (P4b, 路 B2): boxed struct 是共享 `ScriptObject`——扫其
            // struct_refs 引用叶子（slots 空），与 Object 臂同（镜像 trace_children 的 BoxedStruct 分支）。
            Value::BoxedStruct(gc) => { let obj = gc.borrow(); for r in &obj.struct_refs { visitor(r); } }
            // add-struct-heap-inline (P3b): struct[] element handle — follow the
            // backing array's reference leaves (mirrors trace_children).
            Value::StructRefHeap(e) => { let arr = e.arr.borrow(); for r in arr.gc_refs() { visitor(r); } }
            _ => {}
        }
    }

    // ── 5. Collection control ────────────────────────────────────────────────

    /// **add-concurrent-gc P0 (2026-05-22)**: current GC mode. Read on
    /// the barrier hot path + `run_cycle_collection` entry. `Relaxed`
    /// ordering — mode changes are observed at the next collect / next
    /// write, not synchronized with anything else.
    fn mode(&self) -> super::GcMode {
        super::GcMode::from_u8(self.mode.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// **add-concurrent-gc P0 (2026-05-22)**: switch GC mode at runtime.
    /// Takes effect at the next collect; in-progress collects complete
    /// with their original mode. Lock-free `store(Relaxed)` — fast path
    /// on the rare config call.
    fn set_mode(&self, mode: super::GcMode) {
        self.mode.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    }

    /// **add-custom-allocator P2 (2026-05-22)**: explicit finalize.
    /// Wired to `Std.GC.Finalize(x)` z42 builtin.
    ///
    /// Steps (per design D3):
    /// 1. Look up the `RegionEntry` from the Value's GcRef.
    /// 2. Take the finalizer (one-shot via `Mutex<Option>` swap).
    /// 3. If a finalizer was registered: fire it; tombstone the entry
    ///    (alive=false, generation++, push slot to free_list).
    /// 4. Otherwise: still tombstone the entry — caller asked for it
    ///    to be collected.
    ///
    /// Returns `true` iff a finalizer was actually fired by this call.
    fn finalize_now(&self, value: &Value) -> bool {
        match value {
            Value::Object(gc) => {
                let entry_ptr = gc.entry_ptr();
                // SAFETY: GcRef contract guarantees entry pointer is
                // valid for the lifetime of the GcRef. We're under
                // the trait dispatch path; caller's Value parameter
                // keeps the GcRef alive throughout.
                let entry: &super::region::RegionEntry<ScriptObject> = unsafe { entry_ptr.as_ref() };
                let fin = entry.finalizer.lock().take();
                let fired = fin.is_some();
                if let Some(f) = fin { f(); }
                let mut region = self.region_object.lock();
                region.tombstone_via_entry(entry);
                fired
            }
            Value::Array(gc) => {
                let entry_ptr = gc.entry_ptr();
                let entry: &super::region::RegionEntry<ArrayObj> = unsafe { entry_ptr.as_ref() };
                let fin = entry.finalizer.lock().take();
                let fired = fin.is_some();
                if let Some(f) = fin { f(); }
                let mut region = self.region_array.lock();
                region.tombstone_via_entry(entry);
                fired
            }
            _ => false,
        }
    }

    /// **add-concurrent-gc P4b (2026-05-22)**: VmContext-aware collect
    /// dispatch. STW mode follows the proven path (request pause →
    /// collect_cycles); ConcurrentMarkSweep runs the multi-phase
    /// flow (initial STW snapshot → ConcurrentMarking drain → final
    /// STW handshake + sweep).
    fn collect_cycles_with_context(&self, ctx: &crate::vm_context::VmContext) {
        match self.mode() {
            super::GcMode::StwMarkSweep => {
                if let Some(_pause) = super::safepoint::request_gc_pause(ctx) {
                    self.collect_cycles();
                }
            }
            super::GcMode::ConcurrentMarkSweep => {
                let pause = match super::safepoint::request_gc_pause(ctx) {
                    Some(p) => p,
                    None => return, // another collector active; park-as-mutator done
                };
                if self.inner.lock().pause_count > 0 { return; }
                let start = Self::now_us();
                let used_before = self.inner.lock().stats.used_bytes;
                self.fire_event(GcEvent::BeforeCollect {
                    kind: GcKind::CycleCollector, used_bytes: used_before,
                });

                // Phase 1: STW root snapshot (still holding initial pause).
                self.snapshot_roots_into_mark_queue();

                // Phase 2: Yield to ConcurrentMarking — mutators resume.
                pause.yield_to_concurrent_marking();

                // Phase 3: Background mark (this thread = collector; barrier
                // writes from mutators land in mark_queue concurrently).
                self.drain_mark_queue();

                // Phase 4: STW handshake — re-park mutators for final drain.
                pause.request_handshake_pause();

                // Phase 5: Residual drain — any barrier writes between
                // drain-empty-check and handshake-acquire are now safely
                // captured in mark_queue.
                self.drain_mark_queue();
                #[cfg(debug_assertions)]
                {
                    let after_p5 = self.mark_queue.lock().len();
                    assert_eq!(after_p5, 0, "BUG: mark_queue not empty after Phase 5 drain ({after_p5} items)");
                }

                // Phase 6: STW sweep (mutators still parked).
                let freed_bytes = self.sweep_phase();
                #[cfg(debug_assertions)]
                {
                    let post_sweep = self.mark_queue.lock().len();
                    assert_eq!(post_sweep, 0,
                        "BUG: mark_queue non-empty after sweep ({post_sweep} items) — something during sweep pushed to queue");
                }
                {
                    let mut i = self.inner.lock();
                    i.stats.gc_cycles += 1;
                    i.stats.used_bytes = i.stats.used_bytes.saturating_sub(freed_bytes);
                }
                self.maybe_reset_near_limit_warned();
                let pause_us = Self::now_us().saturating_sub(start);
                self.pause_histogram.lock().record(pause_us);
                self.fire_event(GcEvent::AfterCollect {
                    kind: GcKind::CycleCollector, freed_bytes, pause_us,
                });
                #[cfg(debug_assertions)]
                {
                    let post_events = self.mark_queue.lock().len();
                    assert_eq!(post_events, 0,
                        "BUG: mark_queue non-empty after fire_event ({post_events} items) — observer pushed to queue");
                }

                // Validate heap invariants while world is still stopped
                // (before pause Drop wakes workers and write-barriers resume).
                #[cfg(debug_assertions)]
                self.debug_validate_invariants();

                // pause Drop releases the world.
                drop(pause);
            }
            super::GcMode::GenerationalMarkSweep => {
                // add-generational-gc P3 (2026-05-22): minor + escalation.
                // Run a minor first; if survival rate >= threshold,
                // escalate to major in the same STW pause window.
                let _pause = match super::safepoint::request_gc_pause(ctx) {
                    Some(p) => p,
                    None => return,
                };
                if self.inner.lock().pause_count > 0 { return; }

                let start = Self::now_us();
                let used_before = self.inner.lock().stats.used_bytes;
                self.fire_event(GcEvent::BeforeCollect {
                    kind: GcKind::CycleCollector, used_bytes: used_before,
                });

                // Measure young population pre-minor for escalation calc.
                let young_before = {
                    let r_obj = self.region_object.lock();
                    let r_arr = self.region_array.lock();
                    r_obj.young_count() + r_arr.young_count()
                };

                let mut freed_bytes = self.run_cycle_collection_minor();

                // Survival rate: how much of young survived (not
                // tombstoned) AND was promoted out. Easier measured:
                // 1 - tombstoned_fraction; even easier: post young_count
                // / young_before. High survival → escalate.
                let young_after = {
                    let r_obj = self.region_object.lock();
                    let r_arr = self.region_array.lock();
                    r_obj.young_count() + r_arr.young_count()
                };

                if young_before > 0 {
                    // survival_rate = young_after / young_before (the
                    // entries still classed as young after minor —
                    // promoted entries also "survive" but leave
                    // young_list, so this is roughly the
                    // "not-tombstoned-and-not-promoted" rate).
                    let survival = young_after as f32 / young_before as f32;
                    if survival >= Self::minor_escalation_threshold() {
                        // Major in same pause window.
                        freed_bytes += self.run_cycle_collection_major();
                    }
                }

                {
                    let mut i = self.inner.lock();
                    i.stats.gc_cycles += 1;
                    i.stats.used_bytes = i.stats.used_bytes.saturating_sub(freed_bytes);
                }
                self.maybe_reset_near_limit_warned();
                let pause_us = Self::now_us().saturating_sub(start);
                self.pause_histogram.lock().record(pause_us);
                self.fire_event(GcEvent::AfterCollect {
                    kind: GcKind::CycleCollector, freed_bytes, pause_us,
                });
                #[cfg(debug_assertions)]
                self.debug_validate_invariants();
            }
        }
    }

    fn collect_cycles(&self) {
        if self.inner.lock().pause_count > 0 { return; }
        let start = Self::now_us();
        let used_before = self.inner.lock().stats.used_bytes;
        self.fire_event(GcEvent::BeforeCollect {
            kind: GcKind::CycleCollector, used_bytes: used_before,
        });
        let freed_bytes = self.run_cycle_collection();
        {
            let mut i = self.inner.lock();
            i.stats.gc_cycles += 1;
            i.stats.used_bytes = i.stats.used_bytes.saturating_sub(freed_bytes);
        }
        // Phase 3d: 若 used 已降到 90% 阈值以下，重置 near_limit_warned
        self.maybe_reset_near_limit_warned();
        let pause_us = Self::now_us().saturating_sub(start);
        self.pause_histogram.lock().record(pause_us);
        self.fire_event(GcEvent::AfterCollect {
            kind: GcKind::CycleCollector, freed_bytes, pause_us,
        });
        // **add-gc-debug-invariants P1 (2026-05-22)**: post-collect
        // invariant check. Release builds compile this out entirely.
        #[cfg(debug_assertions)]
        self.debug_validate_invariants();
    }

    fn force_collect(&self) -> CollectStats {
        if self.inner.lock().pause_count > 0 {
            return CollectStats::default();
        }
        let start = Self::now_us();
        let used_before = self.inner.lock().stats.used_bytes;
        self.fire_event(GcEvent::BeforeCollect {
            kind: GcKind::Full, used_bytes: used_before,
        });
        let freed_bytes = self.run_cycle_collection();
        {
            let mut i = self.inner.lock();
            i.stats.gc_cycles += 1;
            i.stats.used_bytes = i.stats.used_bytes.saturating_sub(freed_bytes);
        }
        self.maybe_reset_near_limit_warned();
        let pause_us = Self::now_us().saturating_sub(start);
        self.pause_histogram.lock().record(pause_us);
        self.fire_event(GcEvent::AfterCollect {
            kind: GcKind::Full, freed_bytes, pause_us,
        });
        CollectStats {
            freed_bytes, pause_us, kind: Some(GcKind::Full),
        }
    }

    fn pause(&self)  { self.inner.lock().pause_count += 1; }
    fn resume(&self) {
        let mut i = self.inner.lock();
        i.pause_count = i.pause_count.saturating_sub(1);
    }

    // ── 6. Heap config ───────────────────────────────────────────────────────

    fn set_max_heap_bytes(&self, max: Option<u64>) {
        let mut i = self.inner.lock();
        i.stats.max_bytes      = max;
        i.near_limit_warned    = false; // reset 让新阈值能再次触发 NearHeapLimit
    }

    fn used_bytes(&self) -> u64 {
        self.inner.lock().stats.used_bytes
    }

    fn set_strict_oom(&self, enabled: bool) {
        self.inner.lock().strict_oom = enabled;
    }

    // ── 7. Finalization ──────────────────────────────────────────────────────

    /// **Phase 3e**: finalizer 直接挂在 GcAllocation wrapper 上，Drop 时自动
    /// 触发（含 cycle 断环后 alive_vec drop 链 + 普通 Rc Drop）。
    fn register_finalizer(&self, value: &Value, fin: FinalizerFn) {
        match value {
            Value::Object(gc) => GcRef::set_finalizer(gc, fin),
            Value::Array(gc)  => GcRef::set_finalizer(gc, fin),
            _ => {} // 原子值无 finalizer
        }
    }

    fn cancel_finalizer(&self, value: &Value) {
        match value {
            Value::Object(gc) => { let _ = GcRef::cancel_finalizer(gc); }
            Value::Array(gc)  => { let _ = GcRef::cancel_finalizer(gc); }
            _ => {}
        }
    }

    // ── 8. Weak references ───────────────────────────────────────────────────

    fn make_weak(&self, value: &Value) -> Option<WeakRef> {
        match value {
            Value::Object(gc) => Some(WeakRef {
                inner: WeakRefInner::Object(GcRef::downgrade(gc)),
            }),
            Value::Array(gc) => Some(WeakRef {
                inner: WeakRefInner::Array(GcRef::downgrade(gc)),
            }),
            _ => None,
        }
    }

    fn upgrade_weak(&self, weak: &WeakRef) -> Option<Value> {
        match &weak.inner {
            WeakRefInner::Object(w) => w.upgrade().map(Value::Object),
            WeakRefInner::Array (w) => w.upgrade().map(Value::Array),
        }
    }

    // ── 8.6 Soft references ──────────────────────────────────────────────────

    fn register_soft_ref(&self, value: &Value) -> u64 {
        use super::soft_registry::ErasedSoftEntry;
        let (entry, key) = match value {
            Value::Object(gc) => {
                let ptr = gc.entry_ptr();
                let generation = {
                    // SAFETY: entry pointer stable; we only read the generation atomic.
                    unsafe { ptr.as_ref() }.generation.load(std::sync::atomic::Ordering::Acquire)
                };
                unsafe { ptr.as_ref() }.inc_soft_ref_count();
                let key = ptr.as_ptr() as u64;
                (ErasedSoftEntry::from_object(ptr, generation), key)
            }
            Value::Array(gc) => {
                let ptr = gc.entry_ptr();
                let generation = unsafe { ptr.as_ref() }.generation.load(std::sync::atomic::Ordering::Acquire);
                unsafe { ptr.as_ref() }.inc_soft_ref_count();
                let key = ptr.as_ptr() as u64;
                (ErasedSoftEntry::from_array(ptr, generation), key)
            }
            _ => return 0,
        };
        self.inner.lock().soft_registry.insert(entry);
        key
    }

    fn soft_ref_get(&self, key: u64) -> Value {
        // Snapshot under lock, then work outside.
        let entries = self.inner.lock().soft_registry.snapshot_entries();
        let key_usize = key as usize;
        for e in &entries {
            if e.ptr_key() != key_usize { continue; }
            if !e.is_alive() { return Value::Null; }
            // e.is_alive() confirmed: alive=true AND generation == snapshot.
            // Reconstruct GcRef using the snapshot generation (safe against slot reuse).
            return match e.kind {
                super::soft_registry::ErasedKind::Object => {
                    let ptr = key_usize as *mut super::region::RegionEntry<crate::metadata::ScriptObject>;
                    let nn = unsafe { std::ptr::NonNull::new_unchecked(ptr) };
                    Value::Object(unsafe { GcRef::from_region_entry(nn, e.generation_snapshot()) })
                }
                super::soft_registry::ErasedKind::Array => {
                    let ptr = key_usize as *mut super::region::RegionEntry<ArrayObj>;
                    let nn = unsafe { std::ptr::NonNull::new_unchecked(ptr) };
                    Value::Array(unsafe { GcRef::from_region_entry(nn, e.generation_snapshot()) })
                }
            };
        }
        Value::Null
    }

    fn unregister_soft_ref(&self, key: u64) {
        let key_usize = key as usize;
        // Find the entry kind before removing (need it to decrement the right type).
        let kind = {
            let inner = self.inner.lock();
            inner.soft_registry.snapshot_entries()
                .into_iter()
                .find(|e| e.ptr_key() == key_usize)
                .map(|e| e.kind)
        };
        self.inner.lock().soft_registry.remove_one(key_usize);
        // Decrement soft_ref_count on the backing RegionEntry.
        if let Some(kind) = kind {
            match kind {
                super::soft_registry::ErasedKind::Object => {
                    // SAFETY: pointer came from a live RegionEntry; we only
                    // touch the atomic soft_ref_count field.
                    let ptr = key_usize as *mut super::region::RegionEntry<crate::metadata::ScriptObject>;
                    unsafe { (*ptr).dec_soft_ref_count(); }
                }
                super::soft_registry::ErasedKind::Array => {
                    let ptr = key_usize as *mut super::region::RegionEntry<ArrayObj>;
                    unsafe { (*ptr).dec_soft_ref_count(); }
                }
            }
        }
    }

    // ── 8.5 Handle table ────────────────────────────────────────────────────

    fn handle_alloc(&self, target: &Value, kind: GcHandleKind) -> u64 {
        let entry = match (target, kind) {
            (Value::Null, _) => return 0,
            (Value::Object(g), GcHandleKind::Strong) => HandleEntry::StrongObject(g.clone()),
            (Value::Array (g), GcHandleKind::Strong) => HandleEntry::StrongArray (g.clone()),
            (Value::Object(g), GcHandleKind::Weak)   => HandleEntry::WeakObject(GcRef::downgrade(g)),
            (Value::Array (g), GcHandleKind::Weak)   => HandleEntry::WeakArray (GcRef::downgrade(g)),
            // Atomic Strong: just clone the Value into the slot.
            (v, GcHandleKind::Strong) => HandleEntry::StrongAtomic(v.clone()),
            // Atomic Weak: rejected — atomics aren't Rc-backed, can't weak-ref.
            (_, GcHandleKind::Weak) => return 0,
        };
        self.inner.lock().handle_slab.alloc(entry)
    }

    fn handle_target(&self, slot: u64) -> Option<Value> {
        self.inner.lock().handle_slab.get(slot).and_then(|e| e.target())
    }

    fn handle_is_alloc(&self, slot: u64) -> bool {
        self.inner.lock().handle_slab.get(slot).is_some()
    }

    fn handle_kind(&self, slot: u64) -> Option<GcHandleKind> {
        self.inner.lock().handle_slab.get(slot).map(|e| e.kind())
    }

    fn handle_free(&self, slot: u64) {
        self.inner.lock().handle_slab.free(slot);
    }

    // ── 9. Event observers ───────────────────────────────────────────────────

    fn add_observer(&self, observer: Arc<dyn GcObserver>) -> ObserverId {
        let mut i = self.inner.lock();
        let id = ObserverId(i.next_observer_id);
        i.next_observer_id += 1;
        i.observers.push((id, observer));
        i.stats.observers = i.observers.len() as u64;
        id
    }

    fn remove_observer(&self, id: ObserverId) {
        let mut i = self.inner.lock();
        i.observers.retain(|(o_id, _)| *o_id != id);
        i.stats.observers = i.observers.len() as u64;
    }

    // ── 10. Profiler ─────────────────────────────────────────────────────────

    fn set_alloc_sampler(&self, sampler: Option<AllocSamplerFn>) {
        self.inner.lock().alloc_sampler = sampler;
    }

    fn take_snapshot(&self) -> HeapSnapshot {
        // Phase 3b: 直接遍历 heap_registry，覆盖范围升级为 Full（所有 alloc 过且
        // 当前仍 strong-reachable 的对象，不依赖 host pin）。
        let mut snapshot = HeapSnapshot {
            coverage:     SnapshotCoverage::Full,
            timestamp_us: Self::now_us(),
            ..Default::default()
        };
        for v in self.snapshot_live_from_registry() {
            let size = self.object_size_bytes(&v) as u64;
            let Some(type_name) = Self::type_name_of(&v) else { continue };
            let entry = snapshot.objects_by_type.entry(type_name).or_default();
            entry.count += 1;
            entry.bytes += size;
            snapshot.total_objects += 1;
            snapshot.total_bytes   += size;
        }
        snapshot
    }

    fn iterate_live_objects(&self, visitor: &mut dyn FnMut(&Value)) {
        // Phase 3b: registry-driven Full coverage. 同对象只访问一次（registry
        // snapshot 内部去重 by GcRef::as_ptr）。
        for v in self.snapshot_live_from_registry() {
            visitor(&v);
        }
    }

    // ── 11. Stats ────────────────────────────────────────────────────────────

    fn stats(&self) -> HeapStats {
        // Phase 3e: finalizers_pending 即时遍历 heap_registry 重算 —— 因为
        // finalizer 现在挂在 GcAllocation 上，Drop 时自动 take，没有集中
        // 计数器；准确值需扫 registry。snapshot_live_from_registry 顺路 prune
        // 死引用。
        let alive = self.snapshot_live_from_registry();
        let pending = alive.iter().filter(|v| match v {
            Value::Object(gc) => GcRef::has_finalizer(gc),
            Value::Array(gc)  => GcRef::has_finalizer(gc),
            _ => false,
        }).count() as u64;

        let mut s = self.inner.lock().stats.clone();
        s.finalizers_pending = pending;
        s.pause_histogram = self.pause_histogram.lock().clone();
        s
    }
}

#[cfg(test)]
#[path = "arc_heap_tests/mod.rs"]
mod arc_heap_tests;
