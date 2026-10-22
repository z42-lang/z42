//! `ArcMagrGC` —— 默认 GC backend（接口完整 + Trial-deletion 环回收器）。
//!
//! 通过 GC 句柄抽象走 `Rc<GcAllocation<T>>` backing（GcAllocation
//! 含 `inner: RefCell<T>` + `finalizer: RefCell<Option<FinalizerFn>>` + 自定义
//! `Drop`，Phase 3e 起 Drop 时自动触发 finalizer），同时实现 MMTk porting
//! contract 形状的全部 host-side 嵌入接口（roots / observers / profiler /
//! weak refs / finalizers / heap config / strict OOM / ...）。
//!
//! **Phase 3a/3b/3c/3d/3d.1/3f/3e/3f-2/3-OOM 后已知限制**：（无）
//!
//! GC 子系统主功能至此完整。所有原始限制已解决：
//! - 接口（trait + 句柄 + heap registry）✅
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
//! - Phase 3e（add-drop-time-finalizer）：GC 句柄 backing 升级为
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

use super::obj_ref::{ObjRef, WeakObjRef};
use super::arr_ref::{ArrRef, WeakArrRef};
use super::var_region::{BlockType, VarGcRef, VarRegion};
use super::types::{
    AllocSamplerFn, GcHandleKind, GcObserver, HeapStats, ObserverId, RootHandle,
};

// ── Handle table（reorganize-gc-stdlib）─────────────────────────

/// One slot in [`HandleSlab`]. Strong slots store cloneable references that
/// anchor their target across collection; weak slots store a `Weak<...>` that
/// silently nulls out when the target drops elsewhere.
///
/// `Strong(Atomic)` covers `AllocStrong` for atomic values (`I64` / `Str` / ...)
/// — not Rc-backed, so we just hold the cloned `Value`. `AllocWeak` rejects
/// atomic values at the `handle_alloc` layer (returns slot 0).
#[derive(Clone)]
enum HandleEntry {
    StrongObject(ObjRef),
    StrongArray(ArrRef),
    /// Atomic Value clone (I64 / F64 / Str / Bool / Char / FuncRef / ...).
    /// Strong-only — AllocWeak on atomics rejects at the alloc layer.
    StrongAtomic(Value),
    /// Object model R6 / R9: upgraded by the heap (`ArcMagrGC::upgrade_weak_object` /
    /// `upgrade_weak_array`), which reads the chunk epoch the reference recorded.
    WeakObject(WeakObjRef),
    WeakArray(WeakArrRef),
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

    /// Read the slot's current target; weak slots return None once collected. `heap` upgrades
    /// a weak object reference (it needs the variable-length region — call without `inner`
    /// held).
    fn target(&self, heap: &ArcMagrGC) -> Option<Value> {
        match self {
            HandleEntry::StrongObject(g) => Some(Value::Object(*g)),
            HandleEntry::StrongArray(g)  => Some(Value::Array(*g)),
            HandleEntry::StrongAtomic(v) => Some(v.clone()),
            HandleEntry::WeakObject(w)   => heap.upgrade_weak_object(w).map(Value::Object),
            HandleEntry::WeakArray(w)    => heap.upgrade_weak_array(w).map(Value::Array),
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

    /// Every **strong** slot's target, as mark roots.
    ///
    /// **fix-strong-handles-are-not-roots **: this is what makes
    /// `GCHandle.AllocStrong` mean anything. `handle_slab` was reachable only from the four
    /// `handle_*` methods in `arc_heap/interface.rs`; **no mark phase ever scanned it**, so a
    /// strong handle did not anchor its target and the only observable difference from a weak
    /// one was whether `downgrade` was possible. Weak slots are deliberately absent — a weak
    /// handle must *not* keep its target alive.
    fn strong_targets(&self) -> impl Iterator<Item = Value> + '_ {
        self.entries.iter().flatten().filter_map(|e| match e {
            HandleEntry::StrongObject(g) => Some(Value::Object(*g)),
            HandleEntry::StrongArray(g)  => Some(Value::Array(*g)),
            HandleEntry::StrongAtomic(v) => Some(v.clone()),
            HandleEntry::WeakObject(_) | HandleEntry::WeakArray(_) => None,
        })
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
    /// The authoritative liveness store is `ArcMagrGC.region_var` (every object, array, string
    /// and closure is one of its blocks); sweep and iteration walk it directly.
    /// **Phase 3d**: 上次 auto-collect 触发时的 `used_bytes`，用于 throttle
    /// 自动 collect —— 仅当当前 used 距上次增长 >= 10% limit 才再次自动触发。
    last_auto_collect_used: u64,
    /// add-gc-runtime-knobs : `stats.reclaimed_bytes` as of the last
    /// auto-collect trip. The delta since tells `maybe_auto_collect` whether the
    /// previous collection actually bought room — see `auto_collect.rs`.
    last_auto_collect_reclaimed: u64,
    /// Multiplier on the auto-collect growth gate. Doubles on each unproductive
    /// collection (capped), resets to 1 on a productive one, so an over-budget
    /// live set stops re-collecting forever.
    auto_collect_backoff: u32,
    /// The occupied footprint (`gc::footprint`) at the last auto-collect trip, and whether that
    /// trip was the soft cap's (near-limit). A collection the cap asked for is judged by whether
    /// it held the footprint, not by the live bytes it reclaimed — see `auto_collect.rs`.
    last_auto_collect_occupied: u64,
    last_trip_for_cap: bool,
    /// **Phase 3-OOM**: strict OOM 模式开关。true 时 alloc 越界返回 Value::Null
    /// 不入 registry / 不 bump used_bytes（撤销分配）；false（默认）兼容历史
    /// 行为：alloc 仍成功，只 fire 事件。
    strict_oom: bool,
    /// **reorganize-gc-stdlib **: GCHandle slab。Slot 0 reserved
    /// 作"未分配" sentinel；其他 slot 由 `Std.GCHandle._slot: long` 引用。
    handle_slab: HandleSlab,
    // **Phase 3e**: finalizers 不再集中存 HashMap；改存到每个 GcAllocation 的
    // finalizer Cell 上。Drop 时自动 take + fire（含 cycle 断环后 alive_vec
    // drop 链）。register_finalizer / cancel_finalizer 走 ArrRef 方法。
    // finalizers_pending 由 stats() 即时遍历 registry 重算。

    /// **add-gc-softref **: registry of all active soft
    /// references. Populated by `register_soft_ref`; entries are removed
    /// on `unregister_soft_ref`. The revive pass (between mark + sweep)
    /// iterates this to re-mark alive targets when heap pressure is below
    /// the soft threshold.
    soft_registry: super::soft_registry::SoftRegistry,
    /// Object model R6 / R9: finalizers registered on objects and arrays, keyed by block-header
    /// address. The block carries `SIDE_BIT`, so its death reaches the heap through the region's
    /// flagged-dead list (`ArcMagrGC::process_flagged_dead`), which fires and removes the entry.
    obj_finalizers: HashMap<usize, super::types::FinalizerFn>,
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
            .field("auto_collect_backoff", &self.auto_collect_backoff)
            .finish()
    }
}

// ── ArcMagrGC ─────────────────────────────────────────────────────────────────

/// External root scanner type. 宿主（典型情况是 `VmCore` / `VmContext`）
/// 通过 `set_external_root_scanner` 注册的闭包，在 mark 阶段被调用以暴露
/// 自己持有的 Value（如 static_fields / pending_exception / interp 栈帧 regs），
/// 让 cycle collector 不会把这些可达对象误判为 unreachable。
///
/// **add-multithreading-foundation Phase 3 **：要求 `Send + Sync`
/// —— 闭包内部捕获 Arc<VmCore> Weak 等 Send-safe handle；GC 后续可能在
/// 独立 worker 线程上跑收集。
pub type ExternalRootScanner = Box<dyn Fn(&mut dyn FnMut(&Value)) + Send + Sync>;

/// **add-lazy-context-unload **: lets the GC drive collectible
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

/// **add-heap-retention-diagnostics **: like `ExternalRootScanner`
/// but yields each root **with its category** (`RootKind`). Used only by the
/// on-demand retention query (`retention_*`) to report retaining roots at the
/// category level — NOT on the mark hot path (mark uses the anonymous scanner).
pub type CategorizedRootScanner =
    Box<dyn Fn(&mut dyn FnMut(&Value, super::retention::RootKind)) + Send + Sync>;

/// **add-heap-retention-diagnostics **: the heap identity (data ptr
/// as usize) of a heap-ref `Value`, or `None` for primitives / stack refs. Used
/// to key the reverse reference graph. Mirrors `mark_if_unmarked`'s variant set.
fn value_heap_ptr(v: &Value) -> Option<usize> {
    match v {
        // Object model R6: the header address, taken from the handle (thin, whole-block provenance).
        Value::Object(gc) => Some(ObjRef::as_ptr(gc) as usize),
        // add-boxed-struct-identity (P4b): boxed struct is a heap object (an object block).
        Value::BoxedStruct(gc) => Some(ObjRef::as_ptr(gc) as usize),
        // Object model R9: an array is a block too — the header address.
        Value::Array(gc) => Some(ArrRef::as_ptr(gc) as usize),
        // unify-gc-heap PR-2: the closure is itself a heap object (region_var block) — its own
        // block address is its identity.
        Value::Closure(c) => Some(c.addr()),
        // unify-gc-heap PR-4: strings are region_var blocks now — the block header address is
        // their heap identity. (`FuncRef` is a `FnId` since P1-2 PR 8 — no heap identity.)
        Value::Str(s) => Some(s.as_ptr() as usize),
        // make-value-copy: a `Ref` handle has no stable heap identity of its own (its
        // payload lives in the transient arena); like `StructRef`/`StackObject` → None.
        _ => None,
    }
}

/// The payload finalizer injected into the variable-length region (which stays a pure byte
/// allocator with no `metadata::types` dependency), dispatched by block type, and the mask of the
/// block types it runs for (`drop_type_bit`) — the region checks the mask before the indirect call,
/// so a reclaimed POD block costs one branch:
///
/// - `Object` — the header owns an `Arc<TypeDesc>` and, for a native-backed object, the extras box
///   (object model R6 / R8);
/// - `ArrayStruct` — a value-struct array owns its element layout (`Arc<StructTypeLayout>`, the
///   payload's first word; object model R9);
/// - `ArrayValue` — `Value`s, which own nothing while `Value: Copy` (`needs_drop` keeps the arm
///   out of the mask; it comes back on its own the day a droppable variant joins `Value`);
/// - every other block (strings, closures, packed / REF / MIXED arrays) is POD.
fn var_payload_drop_glue() -> Option<(crate::gc::var_region::PayloadDropGlue, u8)> {
    use crate::gc::var_region::drop_type_bit;
    let mut types = drop_type_bit(BlockType::Object) | drop_type_bit(BlockType::ArrayStruct);
    if std::mem::needs_drop::<Value>() {
        types |= drop_type_bit(BlockType::ArrayValue);
    }
    Some((var_drop_glue as crate::gc::var_region::PayloadDropGlue, types))
}

/// # Safety
/// Called once per block reclaim with this block's header (whole-block provenance) and its
/// initialized `size`-byte payload (upheld by `VarRegion`).
unsafe fn var_drop_glue(bt: BlockType, header: std::ptr::NonNull<crate::gc::var_region::GcBlockHeader>, size: usize) -> u64 {
    match bt {
        BlockType::Object => {
            // A block whose type word was never written (an allocation refunded before
            // construction) owns nothing; an installed `Arc` is never null.
            // SAFETY: `header` is this block's header (whole-block provenance, from the region).
            if unsafe { header.as_ref() }.kind_word().is_null() {
                return 0;
            }
            // SAFETY: `header` / `size` are this block's (whole-block provenance, from the region).
            let obj = unsafe { ScriptObject::from_block(header.cast(), size) };
            // SAFETY: a constructed object, dropped exactly once (the region calls the glue once
            // per reclaim); the extras box size is read before it is freed. The kind word is
            // cleared behind it, so a later occupant of the slot finds it null.
            unsafe {
                let freed = ScriptObject::destroy(obj);
                header.as_ref().clear_kind_word();
                freed
            }
        }
        BlockType::ArrayStruct => {
            // SAFETY: as above; a STRUCT array's layout word is null until installed.
            unsafe {
                if header.as_ref().kind_word().is_null() {
                    return 0;
                }
                ArrayObj::destroy_struct(ArrayObj::from_block(header.cast(), size));
                header.as_ref().clear_kind_word();
            }
            0
        }
        BlockType::ArrayValue => {
            let n = size / std::mem::size_of::<Value>();
            // SAFETY: the payload follows the header in the same allocation.
            let base = unsafe { crate::gc::var_region::payload_ptr_of(header) } as *mut Value;
            for i in 0..n {
                // SAFETY: `base[i]` is one of `n` initialized `Value`s in the block payload.
                unsafe { std::ptr::drop_in_place(base.add(i)) }
            }
            0
        }
        BlockType::Str | BlockType::ArrayPrim | BlockType::Closure
        | BlockType::ArrayRef | BlockType::ArrayCell => 0,
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
    /// Selectable GC algorithm.
    /// Encoded as `u8` (`GcMode::from_u8` for round-trip). Read on the
    /// barrier-override hot path and at the entrance of
    /// `run_cycle_collection`. `Relaxed` ordering is sufficient — mode
    /// changes don't synchronize with collect; in-progress collect
    /// completes with its original mode, next collect picks up new
    /// mode (per spec scenario "Mode switch is observable but cannot
    /// interrupt a running collect"). Initialized from
    /// `GcMode::from_env()` so `Z42_GC_MODE` selects the mode at
    /// process start.
    mode: std::sync::atomic::AtomicU8,
    /// The heap: one variable-length GC block region holding every managed value — objects
    /// (`Value::Object` / `Value::BoxedStruct`, object model R6), arrays (`Value::Array`, R9),
    /// strings and closures, each one block (16 B header + payload). Constructed with
    /// `var_drop_glue` (object and value-struct-array blocks release what their header owns; the
    /// rest are POD).
    region_var: Mutex<VarRegion>,
    /// **Object model R6 / R9**: the remembered set — old objects and arrays that may hold a young
    /// reference, each with `REMEMBERED_BIT` set on its block (the bit makes a push happen once
    /// per block). The region has no card table; this list is what the minor re-roots the old
    /// generation from (`seed_from_remembered`). Pushed by the write barrier (mutators, only when
    /// the bit was clear) and by promotion; trimmed by every minor and when a remembered block
    /// dies.
    remembered: Mutex<Vec<VarGcRef>>,
    /// **adaptive-promotion **: whether a survivor one tier short of promotion
    /// should be promoted now rather than spend another collection being re-marked. Decided
    /// per minor from the survival rate of the tier it would skip — see
    /// [`promotion_policy`](super::arc_heap::promotion_policy) for the measurements.
    promotion_policy: promotion_policy::PromotionPolicy,
    /// Gray-object queue of a major mark. Populated by the root snapshot
    /// (`snapshot_roots_into_mark_queue`), by the SATB hand-over in
    /// `close_major_marking`, and by tracing when children turn out to be
    /// newly reachable; drained by `drain_mark_queue` / the incremental mark
    /// slices. Only the collector touches it, always inside a pause, so it is
    /// empty outside a cycle.
    mark_queue: Mutex<Vec<Value>>,
    /// Marking-period allocate-black. True for exactly the span of an
    /// incremental major cycle in which mutators can run between slices; every
    /// allocation made while it is set is born with the cycle's mark. See
    /// [`super::arc_heap::alloc_black`] for why this exists and what it costs.
    alloc_black: std::sync::atomic::AtomicBool,
    /// **add-incremental-major-gc M1 **: this heap's current major mark epoch
    /// (1..=127; starts at 1, so the first major opens 2). Per heap, not per process: another heap advancing a
    /// shared counter could wrap it onto this heap's last epoch and make stale marks read as
    /// current. See [`crate::gc::refs::MarkKind`].
    mark_epoch: std::sync::atomic::AtomicU8,
    /// **add-incremental-major-gc M2a**: old values recorded by the SATB barrier (`gc::satb`) and
    /// handed over by mutator threads when they retire their TLAB. Drained into the mark before a
    /// cycle's marking is declared complete, and treated as extra roots by a minor.
    satb_queue: parking_lot::Mutex<Vec<Value>>,
    /// **add-gc-pause-histogram **: aggregate pause-time
    /// histogram. Recorded into at the end of every `collect_cycles` /
    /// `collect_cycles_with_context` / `force_collect` path, right
    /// before the `AfterCollect` event fires. Surfaced via
    /// `stats().pause_histogram` and the `Std.GC.PauseHistogram()` /
    /// `Std.GC.PauseStatsRaw()` z42 builtins. Single histogram per
    /// heap (per-mode split is a deferred perf spec).
    pause_histogram: Mutex<super::types::PauseHistogram>,
    /// **add-gc-safepoint-auto-threshold **: external flag the
    /// `maybe_auto_collect` path sets (instead of running collect inline)
    /// when allocation pressure trips the threshold. Drained by the next
    /// `check_safepoint(ctx)` which runs a stop-the-world collect.
    ///
    /// `None` when not wired (e.g. GC unit tests that construct
    /// `ArcMagrGC::new()` standalone without a VmCore) — `maybe_auto_collect`
    /// then falls back to the legacy inline `collect_cycles()` call,
    /// preserving the pre- single-threaded behaviour.
    external_needs_collect: Mutex<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// Sends every mutator's next safepoint check to the slow path; run whenever
    /// `external_needs_collect` is raised (`request_collect_at_safepoint`). `None` when unwired.
    safepoint_poke: Mutex<Option<std::sync::Arc<dyn Fn() + Send + Sync>>>,
    /// **add-write-barriers **: test-only sink for barrier
    /// dispatch events. Production builds (no `cfg(test)`) compile this
    /// field out entirely, so the override on `write_barrier_field` /
    /// `write_barrier_array_elem` collapses to a true no-op.
    #[cfg(test)]
    barrier_observer: Mutex<Option<std::sync::Arc<BarrierObserver>>>,
    /// **fix-wasm-string-ops**: process-unique, monotonically-increasing epoch for this heap's
    /// address space (see [`MagrGC::heap_epoch`]). Assigned once at construction from
    /// [`NEXT_HEAP_EPOCH`] and never reused, so an address-keyed cache outliving a torn-down
    /// heap (`corelib::str_meta`) can detect a heap switch and drop entries that would otherwise
    /// false-hit a recycled address (the wasm32 cross-VM string-corruption bug).
    epoch: u64,
    /// **add-gc-tlab (option B)**: live allocation counters moved OUT of the
    /// `inner`-locked `HeapStats` onto lock-free atomics, so the per-alloc hot path
    /// (`record_alloc`) no longer takes the `inner` Mutex (removes 2 of the 4–5 inner locks
    /// per `new`). `stats()` reads these to fill the returned `HeapStats` snapshot; collect
    /// paths `sub_used_bytes` the reclaimed bytes. `Relaxed` suffices — these are monotone
    /// counters / heuristic pressure thresholds, not synchronization for other heap state.
    used_bytes: std::sync::atomic::AtomicU64,
    /// What the heap really holds (`gc::footprint`), shared with the three regions — the
    /// figure `HeapStats::committed_bytes` reports and the soft cap is judged against.
    footprint: std::sync::Arc<crate::gc::footprint::Footprint>,
    allocations: std::sync::atomic::AtomicU64,
    /// **add-gc-tlab (stage 2)**: lock-free mirrors of three
    /// `inner`-locked config fields, read on the TLAB allocation fast path so it
    /// takes **no** `inner` lock per object (the whole point of the TLAB):
    /// - `strict_oom_atomic`: mirrors `inner.strict_oom`. When `true` the fast
    ///   path is bypassed entirely (D6 — strict OOM needs per-object precise
    ///   refund, so it stays on the locked path).
    /// - `max_bytes_atomic`: mirrors `inner.stats.max_bytes` (`u64::MAX` =
    ///   `None`). Lets `check_pressure` short-circuit lock-free when no heap
    ///   limit is set (the common case).
    /// - `sampler_active`: `true` iff `inner.alloc_sampler.is_some()`. Lets the
    ///   fast path skip the sampler `inner.lock()` unless sampling is on.
    ///
    /// All `Relaxed` — heuristics / rarely-flipped config, not synchronization.
    strict_oom_atomic: std::sync::atomic::AtomicBool,
    max_bytes_atomic: std::sync::atomic::AtomicU64,
    sampler_active: std::sync::atomic::AtomicBool,
    /// **add-bounded-nursery **: bytes that have been **promoted into the old
    /// generation** since the last major collection. This is the only signal in the heap that
    /// tracks old-generation growth, and therefore the only sound trigger for a major.
    ///
    /// `used_bytes` cannot do that job: it counts **live** bytes, and a minor keeps it low by
    /// reclaiming young garbage — so a budget gate reading it is satisfied forever while dead
    /// *old* objects (which only a major sweeps) pile up unseen. Measured on
    /// `z42c.semantics --release --no-incremental` at a 128M budget before this change:
    /// `used` ≈ 90 MB, 18 minors, **0 majors**, peak RSS 1031 MB — against 607 MB for plain
    /// STW. Old-gen garbage is bounded by what flowed into it, which is exactly this counter.
    ///
    /// Maintained in the minor sweep (where the promoted set is already being computed) and
    /// reset by [`Self::run_cycle_collection_major`] — **zero cost on the allocation path**.
    promoted_bytes_since_major: std::sync::atomic::AtomicU64,
    /// **add-bounded-nursery **: set by `maybe_auto_collect` when the promoted-byte
    /// gate trips, consumed (and cleared) by the generational arm of
    /// `collect_cycles_with_context`. The auto-collect policy decides *which kind* of
    /// collection it wants; the deferred safepoint path only knows "collect".
    pending_major: std::sync::atomic::AtomicBool,
    /// **add-promotion-age-knob **: minor GCs an entry must survive before it is
    /// promoted. Seeded from `Z42_GC_PROMOTION_AGE` at construction (see `construct.rs`).
    ///
    /// **adaptive-promotion **: no longer fixed — the minor sweep lowers it when
    /// the tier it would drop is measured to reclaim nothing, and restores it otherwise (see
    /// [`promotion_policy`](arc_heap::promotion_policy)). Still a field rather than a config
    /// lookup, so the write barrier's cross-gen check stays one relaxed load; and it only
    /// ever changes inside a STW sweep, so no mutator can observe it mid-write.
    promotion_age: std::sync::atomic::AtomicU8,
    /// The age [`promotion_age`](Self::promotion_age) was configured with, kept because the
    /// adaptive policy needs a fixed line to reason against — it lowers *by one tier from
    /// the configured age*, never by one tier from wherever it drifted to last.
    configured_promotion_age: u8,
    /// **arm-gc-by-default **: the `used_bytes` reading at which the auto-collect
    /// policy wants to be consulted again — Mono SGen's `major_collection_trigger_size`.
    ///
    /// This is what makes arming-by-default affordable. `maybe_auto_collect` takes the heap's
    /// `inner` mutex to read its watermarks, so calling it on every allocation would put a
    /// lock acquire on the hottest path in the VM; before this, the only thing keeping that
    /// off the path was "no budget set ⇒ never collect". Now every allocation instead does
    /// **one relaxed load and a compare** against this, and the slow path runs at most once
    /// per growth gate.
    ///
    /// Maintained by `maybe_auto_collect` on every exit (so a declined trip cannot re-enter
    /// on the next allocation) and by `rearm_auto_collect` after every collection.
    /// `0` — the initial value — means "consult on the first allocation", which arms it.
    next_collect_at: std::sync::atomic::AtomicU64,
    /// **arm-gc-by-default **: bytes that may be allocated before a **minor**
    /// trips, and the unit the major allowance is floored in. `Z42_GC_NURSERY_BYTES` when
    /// set, else [`auto_collect::DEFAULT_NURSERY_BYTES`] — read once at construction, like
    /// `promotion_age`, so the policy never does a `runtime_config()` lookup.
    ///
    /// An `AtomicU64` rather than a plain field only so tests can size it: the policy is all
    /// about ratios between this and the live set, and a test that has to allocate 16 MB to
    /// see one collection is not a test of the policy.
    nursery_bytes: std::sync::atomic::AtomicU64,
    /// **add-incremental-major-gc M2b**: the incremental major's phase, cursor and the policy's
    /// request flags — see `arc_heap/incremental.rs`.
    incremental: incremental::IncrementalState,
    /// **add-pause-budget-nursery**: sizes the nursery from the measured cost of a minor, so the
    /// minor pause has a bound the way the major's slices do. Disabled (and left at the constant
    /// nursery) when `Z42_GC_PAUSE_TARGET_MS=0` or `Z42_GC_NURSERY_BYTES` is set explicitly.
    pause_budget: pause_budget::PauseBudget,
    /// **M9**: judges each minor by its yield and schedules tenures when minors stop paying —
    /// see `arc_heap/young_policy.rs`.
    young_policy: young_policy::YoungPolicy,
    /// **add-pause-budget-nursery**: the configured nursery, kept fixed while `nursery_bytes`
    /// adapts — the old generation's allowance is denominated in this one. See
    /// `auto_collect::allowance_unit`.
    allowance_unit_bytes: std::sync::atomic::AtomicU64,
    /// The allocation-size reading that caps the default gates in entries — see
    /// `arc_heap/entry_unit.rs`. Off when the nursery is configured explicitly.
    entry_unit: entry_unit::EntryUnit,
}

impl ArcMagrGC {
    /// Resize the nursery (tests only). See [`ArcMagrGC::nursery_bytes`].
    #[cfg(test)]
    pub(crate) fn set_nursery_bytes_for_test(&self, n: u64) {
        self.nursery_bytes.store(n.max(1), std::sync::atomic::Ordering::Relaxed);
        // add-pause-budget-nursery: a test sizing the nursery is sizing the whole policy, so the
        // old generation's allowance unit moves with it (production splits the two — see
        // `auto_collect::allowance_unit`).
        self.allowance_unit_bytes.store(n.max(1), std::sync::atomic::Ordering::Relaxed);
        // A stated byte nursery is a manual setting: no entry cap on top of it.
        self.entry_unit.disable();
        self.rearm_auto_collect();
    }

    /// Bytes promoted into the old generation since the last major (tests).
    #[cfg(test)]
    pub(crate) fn promoted_bytes_for_test(&self) -> u64 {
        self.promoted_bytes_since_major.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// **fix-wasm-string-ops**: process-global monotonic source for [`ArcMagrGC::epoch`]. Starts at
/// `1` so `0` stays the "no ambient heap" sentinel ([`crate::gc::ambient::current_heap_epoch`]).
static NEXT_HEAP_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);


/// **add-write-barriers **: discriminant for a single
/// barrier dispatch event captured by [`BarrierObserver`]. Used by
/// `arc_heap_tests::write_barriers` to prove that interp / JIT call
/// sites invoke the barrier with the expected arguments.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BarrierEvent {
    Field { owner_addr: usize, slot: usize, new_is_heap: bool },
    ArrayElem { arr_addr: usize, idx: usize, new_is_heap: bool },
}

/// **add-write-barriers **: test-only barrier event sink.
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
mod alloc_black;
mod alloc_object;
mod barrier;
mod remembered;
mod side_tables;
mod card_verify;
mod construct;
mod auto_collect;
mod collect;
mod control;
mod generational;
mod incremental;
mod mark_ring;
mod pause_budget;
mod entry_unit;
mod promotion_policy;
mod young_policy;
mod roots;
mod observe;
mod interface;
#[cfg(any(test, debug_assertions))]
mod debug;
mod footprint;

impl ArcMagrGC {
    /// 新建默认 GC backend（`Default` 从 `Z42_GC_MODE` 选 mode + 领取 epoch）。
    pub fn new() -> Self { Self::default() }
}

#[cfg(test)]
#[path = "arc_heap_tests/mod.rs"]
mod arc_heap_tests;
