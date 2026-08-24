//! `MagrGC` —— z42 VM 的 GC 抽象接口（嵌入式宿主友好版）。
//!
//! 命名取自《银河系漫游指南》中的 **Magrathea** —— 那颗专门建造定制行星的传奇
//! 世界。
//!
//! # 设计
//!
//! trait 形状对齐 [MMTk](https://www.mmtk.io/) 的 `VMBinding` porting contract
//! —— OpenJDK / V8 / Julia / Ruby / RustPython 的事实标准 GC 抽象。把
//! MMTk 拆分的 sub-trait（`ObjectModel` / `Scanning` / `Collection` /
//! `ReferenceGlue` / ...）合到一个 trait 里按"能力组"分段，让 z42 体量下
//! 接口更易读，未来如需拆 sub-trait 切割面清晰。
//!
//! # 能力组
//!
//! 1. **Allocation** —— 堆分配入口
//! 2. **Roots** —— host-side 显式 pin/unpin + frame scope + GC-side scan
//! 3. **Write barriers** —— 字段 / 数组元素写屏障（默认 no-op；generational
//!    / 自定义堆 / MMTk 集成等后续迭代会重载，见 gc.md "GC 后续迭代规划" A3 / D1）
//! 4. **Object Model** —— 对象尺寸 / 引用扫描 helper（用于 trace / snapshot）
//! 5. **Collection control** —— collect / cycles / force / pause / resume
//! 6. **Heap config** —— max_bytes / used_bytes
//! 7. **Finalization** —— register / cancel finalizer
//! 8. **Weak references** —— make / upgrade weak
//! 9. **Event observers** —— add / remove observer + GcEvent
//! 10. **Profiler** —— alloc sampler / heap snapshot / iterate live
//! 11. **Stats** —— HeapStats 快照
//!
//! # Phase 路线
//!
//! 见 [`docs/design/runtime/gc.md`](../../../../docs/design/runtime/gc.md)
//! "GC 子系统" 段。

use std::sync::Arc;

use crate::metadata::{NativeData, TypeDesc, Value};

pub use super::types::{
    AllocKind, AllocSample, AllocSamplerFn, CollectStats, FinalizerFn, FrameMark,
    GcEvent, GcHandleKind, GcKind, GcObserver, HeapSnapshot, HeapStats, ObjectStats,
    ObserverId, RootHandle, SnapshotCoverage, WeakRef,
};

/// MagrGC —— z42 VM 的 GC 抽象接口（host-friendly）。
///
/// # 实现契约
///
/// - `alloc_*` 返回的 `Value` 与对应 variant 一致（Object / Array）
/// - 多次 `alloc_*` 返回的堆对象互相独立（`Rc::ptr_eq` 为 false）
/// - `pin_root` 返回的 `RootHandle` 在该 heap 内唯一，且 `unpin_root(h)` 后该
///   handle 不再有效
/// - `for_each_root` 必须遍历**所有当前活跃**的 pinned root（含 frame 内）
/// - `enter_frame` 与 `leave_frame` 必须严格配对（栈式）
/// - 实现自负 `&self` 接口背后的内部可变性
pub trait MagrGC: std::fmt::Debug + Send + Sync {
    // ── 1. Allocation ────────────────────────────────────────────────────────

    /// 分配一个 `ScriptObject` 并以 `Value::Object` 返回。
    /// 调用方负责通过 `type_args` 字段（默认 `vec![]`）后续 set 运行时泛型类型
    /// 参数（D-8b-3 Phase 2）；本接口保持向后兼容，不强制每个调用点都传 type_args。
    fn alloc_object(
        &self,
        type_desc: Arc<TypeDesc>,
        slots: Vec<Value>,
        native: NativeData,
    ) -> Value;

    /// unify Phase 2 R3（装箱统一）：装箱一个基元（整数）成堆 `ScriptObject` + 引用身份。
    /// 裸标量 LE 字节存 `struct_bytes`（宽度由调用方按 wrapper 标量宽度定尺，与 struct 装箱完全同构），
    /// **不**走 `type_desc.inline_regions()` 定尺——wrapper 是 phantom struct（零字段/size 0），其
    /// emitted struct_layout 不动（零格式 bump）。默认实现回落 `alloc_object`（测试 mock 不装箱基元，无碍）。
    fn alloc_boxed_prim(&self, type_desc: Arc<TypeDesc>, _struct_bytes: Box<[u8]>) -> Value {
        self.alloc_object(type_desc, Vec::new(), NativeData::None)
    }

    /// 分配一个 `Vec<Value>` 数组并以 `Value::Array` 返回（元素类型未知）。
    fn alloc_array(&self, elems: Vec<Value>) -> Value;

    /// add-reflection-array-element-type: 分配携带元素类型 FQ 名的数组（来自
    /// `ArrayNew` / `ArrayNewLit`），使 `arr.GetType().GetElementType()` 不擦除。
    /// 默认丢弃元素类型退化为 `alloc_array`（测试 mock 不反射，无碍）。
    fn alloc_array_typed(&self, _element_type: &str, elems: Vec<Value>) -> Value {
        self.alloc_array(elems)
    }

    /// add-struct-array-codegen (P3b follow-up): allocate a **pre-built** `ArrayObj`
    /// (e.g. a value-struct `Point[]` with `StructBytes` backing that carries packed
    /// bytes + a reference side-table). Default funnels its boxed elements through
    /// `alloc_array` (mock heaps don't preserve packed backings; harmless for tests).
    /// `ArcHeap` overrides to region-alloc the `ArrayObj` in place, keeping the backing.
    fn alloc_array_obj(&self, obj: crate::metadata::types::ArrayObj) -> Value {
        self.alloc_array(obj.to_boxed_vec())
    }

    /// packed-primitive-arrays Step 3: allocate a packed `byte[]` directly from
    /// an owned `Vec<u8>` (FFI return path — no per-byte boxing). Default boxes
    /// for mock heaps; `ArcHeap` overrides to build a `Bytes` backing in place.
    fn alloc_bytes(&self, bytes: Vec<u8>) -> Value {
        self.alloc_array_typed("byte", bytes.into_iter().map(|b| Value::I64(b as i64)).collect())
    }

    /// fix-wasm-string-ops: a process-unique, monotonically-increasing identifier for this
    /// heap's address space. Never reused across heaps (even if a new heap's backing memory is
    /// malloc-recycled at an old heap's address). Address-keyed caches with a lifetime longer
    /// than one heap (`corelib::str_meta`) tag their entries with this epoch and invalidate on
    /// change, so a fresh block (generation 0) at a recycled address can't false-hit a stale
    /// gen-0 entry from a torn-down heap. Mock/test heaps return `0` (they never churn heaps).
    fn heap_epoch(&self) -> u64 {
        0
    }

    /// unify-gc-heap PR-3: allocate a raw variable-length GC block of `payload` bytes with the
    /// given block type, returning its handle. Used to place array element storage (and closure
    /// data) in the single GC heap. Only `ArcMagrGC` implements a real variable-length region;
    /// the default panics (mock heaps never allocate GC blocks — they degrade array/closure
    /// paths to boxed `Vec`s).
    fn alloc_var_block(
        &self,
        payload: usize,
        block_type: crate::gc::var_region::BlockType,
    ) -> crate::gc::var_region::VarGcRef {
        let _ = (payload, block_type);
        unreachable!("alloc_var_block requires a variable-length GC region (ArcMagrGC)")
    }

    /// unify-gc-heap PR-2: allocate a capturing closure's [`ClosureData`](crate::metadata::types::ClosureData)
    /// into the GC variable-length region and return a `Value::Closure` handle. Default boxes it
    /// for mock heaps (no variable-length region); `ArcMagrGC` overrides to region-alloc in place.
    fn alloc_closure(&self, data: crate::metadata::types::ClosureData) -> Value {
        // Mock/default heaps have no variable-length region — this default is unused in the real
        // VM (ArcMagrGC overrides). Constructing a closure without a region is impossible, so the
        // default drops the data and yields Null (mock heaps never build closures).
        let _ = data;
        Value::Null
    }

    /// unify-gc-heap PR-4: allocate an immutable UTF-8 string into the GC
    /// variable-length region (`BlockType::Str`) and return a thin [`Str`] handle.
    /// The primary allocation entry for `Value::Str` (reached via the ambient heap
    /// from `Str::new`/`.into()`). The default falls back to a standalone leaked
    /// block for mock heaps with no variable-length region (test doubles never churn
    /// enough for the leak to matter); `ArcMagrGC` overrides to region-alloc in place.
    fn alloc_str(&self, s: &str) -> crate::metadata::vstr::Str {
        crate::metadata::vstr::Str::new_leaked(s)
    }

    /// fuse-str-concat-alloc: allocate a fresh GC string that is the
    /// concatenation `a ++ b`, sizing the `BlockType::Str` block to
    /// `a.len() + b.len()` and filling it by copying both segments directly.
    /// This fuses what `alloc_str(&format!("{a}{b}"))` did in two heap
    /// allocations (the intermediate `String` + the GC block) into one — the
    /// hot path for the `StrConcat` IR op (string `+`) and `Std.String.Concat`.
    /// The default builds the `String` and falls back to a leaked block (mock
    /// heaps); `ArcMagrGC` overrides to region-alloc the fused block in place.
    fn alloc_str_concat2(&self, a: &str, b: &str) -> crate::metadata::vstr::Str {
        let mut s = String::with_capacity(a.len() + b.len());
        s.push_str(a);
        s.push_str(b);
        crate::metadata::vstr::Str::new_leaked(&s)
    }

    // ── 2. Roots ─────────────────────────────────────────────────────────────

    /// 注册一个 **external root scanner** 闭包 —— GC mark 阶段在扫完 pinned
    /// root 后调用此闭包，让宿主（典型情况是 `VmContext` / `VmCore`）暴露自己
    /// 持有的、不通过 GC handle 持有的 `Value`（如 `static_fields` 槽位、
    /// pending exception、frame regs 等）。
    ///
    /// **add-multithreading-foundation Phase 2.2 (2026-05-19)**：本方法从
    /// `ArcMagrGC` 专属升级到 trait 接口，因为 `heap` 字段在本阶段移入 VmCore
    /// 后，scanner 必须能通过 `Box<dyn MagrGC>` 安装（VmCore 构造完毕拿到
    /// `Weak<VmCore>` 再 install）。
    ///
    /// 同一 backend 上重复调用**覆盖**之前的 scanner（仅一个 active 闭包）。
    /// 默认实现 no-op（适合不参与 cycle / external-root 的 backend；当前
    /// 仅 `ArcMagrGC` 重载）。
    fn set_external_root_scanner(&self, _scanner: super::arc_heap::ExternalRootScanner) {}

    /// **add-lazy-context-unload (2026-08-05)**: wire the collectible-context
    /// reclaimer hook (drives `AssemblyLoadContext` unload reclamation on major
    /// GC). Default no-op; only `ArcMagrGC` overrides.
    fn set_context_reclaimer(&self, _hook: super::arc_heap::ContextReclaimHook) {}

    /// **add-heap-retention-diagnostics (2026-08-06)**: wire the categorized
    /// root scanner (for retention-query L2). Default no-op; `ArcMagrGC` overrides.
    fn set_categorized_root_scanner(&self, _scanner: super::arc_heap::CategorizedRootScanner) {}

    /// **add-heap-retention-diagnostics**: L1 direct referrers of a heap object
    /// (by data ptr). Default empty (backends without a reverse walk).
    fn retention_direct_referrers(&self, _target: usize) -> Vec<super::retention::RetainerInfo> {
        Vec::new()
    }

    /// **add-heap-retention-diagnostics**: L2 retaining roots of a heap object.
    /// Default empty.
    fn retention_roots(&self, _target: usize) -> Vec<super::retention::RootInfo> {
        Vec::new()
    }

    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: register the external
    /// "needs collect" flag that turns an allocator pressure-trip into a
    /// *deferred*, safepoint-coordinated collect rather than an inline one.
    ///
    /// # Auto-collect protocol (three states)
    ///
    /// The allocator's [`maybe_auto_collect`](crate::gc::arc_heap) decides a
    /// collect is warranted when heap-used crosses **`gc_near_limit_ratio`**
    /// (default 0.90) of the max-bytes limit *and* has grown by at least
    /// **`gc_throttle_ratio`** (default 0.10) of the limit since the last
    /// auto-collect (both `Z42_GC_*` knobs — see `config.rs`). What happens
    /// next depends on whether this flag is wired:
    ///
    /// 1. **Register** — `VmCore::new` calls this once after construction to
    ///    hand the heap an `Arc<AtomicBool>` shared with every `VmContext`.
    ///    A backend that ignores allocation thresholds (mock heaps) or needs
    ///    no cross-thread coordination keeps the default no-op and stays in
    ///    state 3 permanently.
    /// 2. **Defer** (flag wired, the production path) — `maybe_auto_collect`
    ///    only does `flag.store(true, Release)` and returns; it does **not**
    ///    collect on the allocating thread. The flag is drained by the next
    ///    [`check_safepoint`](crate::gc::safepoint::check_safepoint) on any
    ///    mutator: the slow path claims the round via `swap(false, AcqRel)`
    ///    (first claimer wins; others skip) and runs a stop-the-world collect
    ///    under [`request_gc_pause`](crate::gc::safepoint::request_gc_pause),
    ///    so the scanner never races a mutator's live registers.
    /// 3. **Fallback** (flag unwired) — `maybe_auto_collect` collects inline
    ///    via `collect_cycles()`. This preserves single-threaded behaviour for
    ///    GC unit tests that construct `ArcMagrGC::new()` without a VmCore.
    ///
    /// **Who checks / when**: the flag is *set* on the allocating thread at
    /// alloc time; it is *checked and cleared* by mutator threads at their
    /// throttled safepoint polls (function entry / back-edge / Call return).
    /// A set flag never blocks the allocator — collection latency is bounded
    /// by the safepoint throttle, not by allocation.
    fn set_external_needs_collect_flag(&self, _flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {}

    /// 把一个 value 加入 root set，host 持有返回的 `RootHandle` 期间该值
    /// 不会被 GC 回收。等价于 V8 `Persistent<T>` / .NET `GCHandle.Alloc(Normal)`。
    fn pin_root(&self, value: Value) -> RootHandle;

    /// 释放 `pin_root` 返回的 handle。释放后的 handle 不应再次使用。
    fn unpin_root(&self, handle: RootHandle);

    /// 进入一个 root scope frame。同 frame 内所有 `pin_root` 在
    /// `leave_frame(mark)` 时自动 unpin（无需逐个调 `unpin_root`）。
    fn enter_frame(&self) -> FrameMark;

    /// 离开 frame，丢弃自 `enter_frame` 起 pin 的所有 root。
    fn leave_frame(&self, mark: FrameMark);

    /// 遍历当前所有活跃 root（`pin_root` + frame pins）。GC 实现 trace 时使用。
    fn for_each_root(&self, visitor: &mut dyn FnMut(&Value));

    // ── 3. Write barriers ────────────────────────────────────────────────────

    /// 字段写屏障：当 `owner` 对象的第 `slot` 字段被赋为 `new` 时通知 GC。
    ///
    /// **Caller 契约 (add-write-barriers, 2026-05-21)**: callers (interp /
    /// JIT FieldSet path) MUST invoke this exactly when `new.is_heap_ref()
    /// == true`. Primitive writes (`I64 / F64 / Bool / Char / Str / Null /
    /// FuncRef / PinnedView / StackClosure / Ref::Stack`) skip the call —
    /// they neither create cross-region nor cross-generation references.
    /// Caller also drops any held inner-`Mutex` lock before calling
    /// (`borrow_mut` on `owner.slots`), so a future override that needs
    /// to re-borrow `owner` (e.g. card-marking that inspects all slots)
    /// does not deadlock.
    ///
    /// **Override 契约**: implementations may `debug_assert!(new.is_heap_ref())`
    /// to detect contract violations. Phase 1 STW mark-sweep default is
    /// no-op. Future `add-generational-gc` / `add-concurrent-gc` will
    /// override with card-marking / SATB logic.
    #[allow(unused_variables)]
    fn write_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {}

    /// 数组元素写屏障；契约同 `write_barrier_field`（call-site filter +
    /// post-write + lock released before call）。
    #[allow(unused_variables)]
    fn write_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {}

    // ── 4. Object Model ──────────────────────────────────────────────────────

    /// 估计对象的浅尺寸（不递归 nested values）。Phase 1 实现给出 enum tag +
    /// 容器自身的合理估计；Phase 3 trace 时会被精确化。
    fn object_size_bytes(&self, value: &Value) -> usize;

    /// 访问 `value` 中所有引用类型的内嵌 Value。
    /// - `Value::Object` → 每个 slot
    /// - `Value::Array`  → 每个元素
    /// - 原子值（I64/F64/Bool/Char/Str/Null）→ 不调 visitor
    fn scan_object_refs(&self, value: &Value, visitor: &mut dyn FnMut(&Value));

    // ── 5. Collection control ────────────────────────────────────────────────

    /// **add-concurrent-gc P0 (2026-05-22)**: 读取当前 GC 模式
    /// (STW mark-sweep / concurrent mark-sweep)。
    ///
    /// 默认实现返回 `GcMode::StwMarkSweep` —— 非 Arc backing 的 heap impl
    /// 不必支持模式切换，trait 默认让它们自动落到 STW（兼容）。
    fn mode(&self) -> super::GcMode { super::GcMode::StwMarkSweep }

    /// **add-concurrent-gc P0 (2026-05-22)**: 设置 GC 模式。
    ///
    /// 默认实现 panic —— 只有支持多模式的 backing（当前仅 `ArcMagrGC`）
    /// 真实重载。模式切换在 collect 进行中不生效（下次 collect 才采用
    /// 新模式），由实现保证。
    fn set_mode(&self, _mode: super::GcMode) {
        panic!("set_mode not supported by this MagrGC impl");
    }

    /// **add-custom-allocator P2 (2026-05-22)**: explicit finalize.
    /// User-facing entry point via `Std.GC.Finalize(x)` z42 builtin.
    /// Fires `value`'s registered finalizer immediately (one-shot)
    /// and tombstones the slot so future strong references panic on
    /// borrow + future weak references upgrade to None.
    ///
    /// Returns `true` if a finalizer was invoked (object had one
    /// registered and was alive at time of call), `false` otherwise
    /// (no-finalizer / not heap-ref / already tombstoned).
    ///
    /// Default impl returns false — non-ArcMagrGC backends without
    /// region/tombstone semantics don't support prompt finalization.
    fn finalize_now(&self, _value: &crate::metadata::Value) -> bool {
        false
    }

    /// 触发完整 GC（stop-the-world tracing）。Phase 1 默认 no-op。
    fn collect(&self) {}

    /// 触发环引用检测与回收。Phase 1 默认 no-op（仅递增 stats counter
    /// 与发 GcEvent）。
    fn collect_cycles(&self) {}

    /// **add-concurrent-gc P4b (2026-05-22)**: VmContext-aware collect
    /// entry point. Production callers (safepoint slow-path + the
    /// `Std.GC.Collect()` builtin) call this so the heap can choose its
    /// own pause-coordination strategy:
    ///
    /// - `GcMode::StwMarkSweep` impls take `request_gc_pause` themselves
    ///   and call `collect_cycles()` STW (current default).
    /// - `GcMode::ConcurrentMarkSweep` impls take the initial pause,
    ///   snapshot roots, transition to `ConcurrentMarking`, drain the
    ///   gray queue concurrently with mutators, then call
    ///   `request_handshake_pause` for final drain + STW sweep.
    ///
    /// Default implementation falls back to the STW path: caller
    /// acquires its own pause (via `request_gc_pause`) and calls
    /// `collect_cycles()`. ArcMagrGC overrides to dispatch on `mode()`.
    fn collect_cycles_with_context(&self, ctx: &crate::vm_context::VmContext) {
        // Default: STW path. ArcMagrGC overrides.
        if let Some(_pause) = super::safepoint::request_gc_pause(ctx) {
            self.collect_cycles();
        }
    }

    /// 强制立即回收，返回本次 GC 的统计。返回 `kind: None` 表示被
    /// `pause()` 跳过。
    fn force_collect(&self) -> CollectStats;

    /// 暂停 GC（关键区使用，如 host 在采样热路径时不希望 GC 介入）。
    /// 暂停期间 `force_collect` / `collect_cycles` 跳过实际工作但仍返回
    /// 一致结果。
    fn pause(&self);

    /// 恢复 GC 工作。`pause` / `resume` 嵌套调用，需配对。
    fn resume(&self);

    // ── 6. Heap config ───────────────────────────────────────────────────────

    /// 设置堆字节上限（`None` = 不限制）。超过 75% 触发 `AllocationPressure`，
    /// 超过 90% 触发 `NearHeapLimit`，越界触发 `OutOfMemory`。
    /// 默认情况下（strict_oom=false）alloc 仍然成功，仅通知；启用
    /// `set_strict_oom(true)` 后 alloc 越界返回 `Value::Null` 不实际占用 heap。
    fn set_max_heap_bytes(&self, max: Option<u64>);

    /// 已用字节数（同 `stats().used_bytes`）。
    fn used_bytes(&self) -> u64;

    /// 启用 / 关闭 **strict OOM 模式**。Phase 3-OOM (2026-04-29)。
    /// 默认 false（行为兼容历史：alloc 越界仅 fire 事件不拒绝）。
    /// 启用后：`alloc_*` 越过 `max_heap_bytes` 时返回 `Value::Null`、不入 registry、
    /// 不 bump used_bytes，仍 fire `OutOfMemory` 事件让 observer 感知。
    /// 调用方（script）见到 Null 通常会在后续访问产生 NullReferenceException；
    /// host 可通过 OOM observer 提前感知并主动管理（kill VM / 重置 heap 等）。
    fn set_strict_oom(&self, _enabled: bool) {}

    // ── 7. Finalization ──────────────────────────────────────────────────────

    /// 注册一个 finalizer，当 `value` 不可达时触发。
    ///
    /// **Phase 1 RC 模式限制**：注册被记录（`stats().finalizers_pending` 加 1），
    /// 但 callback **不会被自动调用** —— `Rc<RefCell<T>>` Drop 不可拦截。
    /// Phase 3 mark-sweep 调度真实触发。
    fn register_finalizer(&self, value: &Value, finalizer: FinalizerFn);

    /// 取消之前注册的 finalizer。
    fn cancel_finalizer(&self, value: &Value);

    // ── 8. Weak references ───────────────────────────────────────────────────

    /// 创建对 `value` 的弱引用。原子值（I64/Str/Null/...）返回 `None`。
    fn make_weak(&self, value: &Value) -> Option<WeakRef>;

    // ── 8.6 Soft references（add-gc-softref，2026-05-26）────────────────────
    //
    // Soft references keep their target alive when heap pressure is below
    // `Z42_GC_SOFT_THRESHOLD` (default 0.80). Above the threshold the
    // target is treated as unreachable and may be swept.
    //
    // The registry key returned by `register_soft_ref` is an opaque `u64`
    // that the `Std.SoftHandle` script object stores in its `_key: long`
    // field. `soft_ref_get(key)` re-upgrades the target; `unregister_soft_ref`
    // must be called when the `SoftHandle` object is finalized.

    /// Register a soft reference to `value`. Returns an opaque key (`>= 1`)
    /// on success. Returns 0 for non-heap values (atomics / Null).
    ///
    /// Default: returns 0 (unsupported by non-ArcMagrGC backends).
    fn register_soft_ref(&self, _value: &Value) -> u64 { 0 }

    /// Try to upgrade the soft reference identified by `key`. Returns
    /// `Value::Null` if the target was collected or `key` is unknown.
    ///
    /// Default: always returns `Value::Null`.
    fn soft_ref_get(&self, _key: u64) -> Value { Value::Null }

    /// Release the soft-ref slot identified by `key`. Idempotent.
    ///
    /// Default: no-op.
    fn unregister_soft_ref(&self, _key: u64) {}

    /// 尝试从弱引用恢复强引用。若目标已被回收（无强引用持有）返回 `None`。
    fn upgrade_weak(&self, weak: &WeakRef) -> Option<Value>;

    // ── 8.5 Handle table（reorganize-gc-stdlib，2026-05-07）─────────────────
    //
    // Slab + free-list backed handle table that powers `Std.GCHandle` (struct,
    // single `_slot: long`). Slot 0 is reserved as the "unallocated" sentinel —
    // any caller seeing `_slot == 0` knows the handle was never bound to a real
    // entry (e.g. `AllocWeak` on an atomic value).
    //
    // Strong slots `Rc::clone` the underlying ScriptObject / Vec<Value>, which
    // anchors the target across GC collection. Weak slots store a `Weak<...>`
    // and return `None` from `handle_target` once the target has been dropped.
    // Both kinds require explicit `handle_free` to release the slot — copying a
    // GCHandle struct duplicates the slot ID, so freeing one alias frees the
    // backing for all aliases (matches C# `GCHandle` struct semantics).

    /// Allocate a handle table slot for `target` with the given `kind`.
    /// Returns the slot id (always `>= 1` on success, `0` for "could not
    /// allocate"). The current rejection conditions:
    /// - `kind == Weak` and `target` is an atomic value (no Rc backing) → 0
    /// - `target == Value::Null` → 0
    fn handle_alloc(&self, target: &Value, kind: GcHandleKind) -> u64;

    /// Read the current target of `slot`. Returns `None` when the slot has been
    /// freed, or — for Weak slots — when the target has been collected.
    fn handle_target(&self, slot: u64) -> Option<Value>;

    /// `true` until `handle_free(slot)` is called. **Note**: for Weak slots
    /// `is_alloc` stays `true` even after the target is collected (slot is
    /// still owned by its handle). Use `handle_target` to detect collection.
    fn handle_is_alloc(&self, slot: u64) -> bool;

    /// `Some(kind)` while the slot is allocated; `None` after `handle_free`.
    fn handle_kind(&self, slot: u64) -> Option<GcHandleKind>;

    /// Release `slot`. Idempotent: freeing a slot that has already been freed
    /// (or never allocated, e.g. slot 0) is a no-op.
    fn handle_free(&self, slot: u64);

    // ── 9. Event observers ───────────────────────────────────────────────────

    /// 注册一个 GC 事件观察者。
    fn add_observer(&self, observer: Arc<dyn GcObserver>) -> ObserverId;

    /// 移除一个观察者。
    fn remove_observer(&self, id: ObserverId);

    // ── 10. Profiler ─────────────────────────────────────────────────────────

    /// 安装分配采样器（每次 `alloc_*` 触发回调），传 `None` 卸载。
    fn set_alloc_sampler(&self, sampler: Option<AllocSamplerFn>);

    /// 拍下堆快照（按 type_desc.name 聚合）。
    ///
    /// **Phase 1 RC 模式**：snapshot 仅覆盖**从 pinned roots 可达**的对象，
    /// `coverage = SnapshotCoverage::ReachableFromPinnedRoots`。Phase 3 trace
    /// 实现后自动升级 `Full`。
    fn take_snapshot(&self) -> HeapSnapshot;

    /// 遍历当前所有存活对象（同 snapshot 的覆盖范围限制）。
    fn iterate_live_objects(&self, visitor: &mut dyn FnMut(&Value));

    // ── 11. Stats ────────────────────────────────────────────────────────────

    fn stats(&self) -> HeapStats;
}

#[cfg(test)]
#[path = "heap_tests.rs"]
mod heap_tests;
