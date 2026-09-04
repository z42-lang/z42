//! `impl MagrGC for ArcMagrGC` —— GC 公共 trait 接口（薄委托层）。
//! 重方法体下沉到 concern 子模块的 inherent 方法（refactor-arc-heap-modularization）。

use std::sync::Arc;
use crate::metadata::{NativeData, ScriptObject, TypeDesc, Value};
use crate::metadata::types::ClosureData;
use crate::gc::heap::MagrGC;
use crate::gc::refs::{GcRef};
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::types::{AllocSamplerFn, CollectStats, FinalizerFn, FrameMark, GcHandleKind, GcObserver, HeapSnapshot, HeapStats, ObserverId, RootHandle, WeakRef, WeakRefInner};

use crate::gc::arc_heap::{ArcMagrGC, HandleEntry, ContextReclaimHook, ExternalRootScanner, CategorizedRootScanner};

impl MagrGC for ArcMagrGC {
    /// **fix-wasm-string-ops**: this heap's process-unique, never-reused epoch (assigned at
    /// construction). See [`MagrGC::heap_epoch`] and [`crate::corelib::str_meta`].
    #[inline]
    fn heap_epoch(&self) -> u64 {
        self.epoch
    }

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
    fn retention_direct_referrers(&self, target: usize) -> Vec<crate::gc::retention::RetainerInfo> {
        self.force_collect();
        self.build_retention_graph().direct_referrers(target)
    }

    /// **L2**: GC roots retaining `target` (category-level), via reverse BFS.
    /// Forces a full GC first for accuracy.
    fn retention_roots(&self, target: usize) -> Vec<crate::gc::retention::RootInfo> {
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
        // unify-object-byte-layout (PR-2): `slots` is now **initial field values by
        // slot index** (not a stored `Box<[Value]>`). Allocate the zeroed byte region
        // + `Null` refs from the composed layout (zero = every primitive/ref default),
        // then encode each provided initial value through `set_field_value`. Callers
        // that pass `Vec::new()` (e.g. `obj_new`, which relies on defaults) skip the
        // loop entirely — zero-init already equals the old per-field defaults.
        let storage = type_desc.object_storage();
        let mut obj = ScriptObject {
            type_desc,
            storage,
            native,
            type_args: Box::new([]),
        };
        for (i, v) in slots.into_iter().enumerate() {
            obj.set_field_value(i, &v);
        }

        self.finish_alloc(obj)
    }

    /// unify Phase 2 R3（装箱统一）：装箱一个基元（整数）成堆 `ScriptObject`+引用身份——`struct_bytes`
    /// 装裸标量（LE 字节，宽度由调用方按 wrapper 定，见 `corelib/convert.rs::box_prim_to_heap`），
    /// **不走 `type_desc.inline_regions()`**（基元 wrapper 如 Int32 是零字段 struct，layout size=0，
    /// 装不下标量）。整数装箱无引用叶子 → `struct_refs`/`slots` 空。返 `Value::Object`，调用方包成
    /// `Value::BoxedStruct`（与 struct 装箱同款引用身份/GC/反射路径）。
    fn alloc_boxed_prim(&self, type_desc: Arc<TypeDesc>, scalar_bytes: Box<[u8]>) -> Value {
        // unify-object-byte-layout (PR-2): the boxed scalar IS the object's whole byte
        // payload (`bytes`); no reference leaves.
        let obj = ScriptObject {
            type_desc,
            storage: crate::metadata::types::ObjStorage::from_bytes(&scalar_bytes),
            native: NativeData::None,
            type_args: Box::new([]),
        };
        self.finish_alloc(obj)
    }

    fn alloc_array(&self, elems: Vec<Value>) -> Value {
        // unify-gc-heap PR-3: the constructor allocates the element block in `self`'s region_var.
        self.alloc_array_obj(crate::metadata::types::ArrayObj::new(self, elems))
    }

    fn alloc_array_typed(&self, element_type: &str, elems: Vec<Value>) -> Value {
        self.alloc_array_obj(crate::metadata::types::ArrayObj::typed(self, element_type, elems))
    }

    /// add-struct-array-codegen (P3b follow-up): `Heap`-trait override so dyn-dispatched
    /// `ctx.heap().alloc_array_obj(..)` region-allocs the pre-built `ArrayObj` (keeping a
    /// `StructBytes` backing), not the trait default that would box it. Routes to the
    /// inherent `ArcMagrGC::alloc_array_obj` (fully-qualified — no recursion/ambiguity).
    fn alloc_array_obj(&self, obj: crate::metadata::types::ArrayObj) -> Value {
        ArcMagrGC::alloc_array_obj(self, obj)
    }

    fn alloc_bytes(&self, bytes: Vec<u8>) -> Value {
        self.alloc_array_obj(crate::metadata::types::ArrayObj::from_bytes(self, bytes))
    }

    fn alloc_closure(&self, data: ClosureData) -> Value {
        self.alloc_closure_in_region(data)
    }

    fn alloc_str(&self, s: &str) -> crate::metadata::vstr::Str {
        self.alloc_str_in_region(s)
    }

    fn alloc_str_concat2(&self, a: &str, b: &str) -> crate::metadata::vstr::Str {
        self.alloc_str_concat2_in_region(a, b)
    }

    fn alloc_var_block(&self, payload: usize, block_type: BlockType) -> VarGcRef {
        // add-gc-tlab (stage 3): lock-free var TLAB when armed; else locked path.
        // (No stats record here — matches the pre-TLAB behavior; the array/backing
        // caller records the whole allocation's size.)
        self.acquire_var_block(payload, block_type).0
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

    fn write_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        ArcMagrGC::write_barrier_field(self, owner, slot, new)
    }

    fn write_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
        ArcMagrGC::write_barrier_array_elem(self, arr, idx, new)
    }

    // ── 4. Object Model ──────────────────────────────────────────────────────

    fn object_size_bytes(&self, value: &Value) -> usize {
        ArcMagrGC::object_size_bytes(self, value)
    }

    fn scan_object_refs(&self, value: &Value, visitor: &mut dyn FnMut(&Value)) {
        // unify-gc-heap PR-5: read-only graph enumeration is now the `for_marking = false`
        // mode of the single-source `Value::visit_gc_children` (no mark side effects; a
        // closure's captured refs are descended directly). Snapshot / retention only.
        value.visit_gc_children(false, visitor);
    }

    // ── 5. Collection control ────────────────────────────────────────────────

    /// **add-concurrent-gc P0 (2026-05-22)**: current GC mode. Read on
    /// the barrier hot path + `run_cycle_collection` entry. `Relaxed`
    /// ordering — mode changes are observed at the next collect / next
    /// write, not synchronized with anything else.
    fn mode(&self) -> crate::gc::GcMode {
        crate::gc::GcMode::from_u8(self.mode.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// **add-concurrent-gc P0 (2026-05-22)**: switch GC mode at runtime.
    /// Takes effect at the next collect; in-progress collects complete
    /// with their original mode. Lock-free `store(Relaxed)` — fast path
    /// on the rare config call.
    fn set_mode(&self, mode: crate::gc::GcMode) {
        self.mode.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    }

    /// **add-custom-allocator P2**: explicit finalize; impl in `ArcMagrGC::finalize_now`.
    fn finalize_now(&self, value: &Value) -> bool {
        ArcMagrGC::finalize_now(self, value)
    }

    /// **add-concurrent-gc P4b**: VmContext-aware collect; impl in `ArcMagrGC::collect_cycles_with_context`.
    fn collect_cycles_with_context(&self, ctx: &crate::vm_context::VmContext) {
        ArcMagrGC::collect_cycles_with_context(self, ctx)
    }

    fn collect_cycles(&self) {
        ArcMagrGC::collect_cycles(self)
    }

    fn force_collect(&self) -> CollectStats {
        ArcMagrGC::force_collect(self)
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
        // add-gc-tlab (stage 2): mirror into the lock-free atomic (u64::MAX = None)
        // so `check_pressure` reads the limit without the inner lock.
        self.max_bytes_atomic.store(
            max.unwrap_or(u64::MAX),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    fn used_bytes(&self) -> u64 {
        self.used_bytes_atomic() // add-gc-tlab (option B): lock-free atomic counter
    }

    fn set_strict_oom(&self, enabled: bool) {
        self.inner.lock().strict_oom = enabled;
        // add-gc-tlab (stage 2): mirror so the alloc fast path can check strict
        // mode lock-free (D6: strict OOM bypasses the TLAB for precise refund).
        self.strict_oom_atomic.store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    /// **add-gc-tlab (stage 2)**: retire the calling thread's TLAB into this
    /// heap's regions (see the trait contract). Delegates to the inherent impl.
    fn retire_thread_tlab(&self) {
        ArcMagrGC::retire_thread_tlab(self)
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
        ArcMagrGC::register_soft_ref(self, value)
    }

    fn soft_ref_get(&self, key: u64) -> Value {
        ArcMagrGC::soft_ref_get(self, key)
    }

    fn unregister_soft_ref(&self, key: u64) {
        ArcMagrGC::unregister_soft_ref(self, key)
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
        let active = sampler.is_some();
        self.inner.lock().alloc_sampler = sampler;
        // add-gc-tlab (stage 2): mirror so the fast path skips the sampler
        // inner.lock() unless sampling is actually on.
        self.sampler_active.store(active, std::sync::atomic::Ordering::Relaxed);
    }

    fn take_snapshot(&self) -> HeapSnapshot {
        ArcMagrGC::take_snapshot(self)
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
        ArcMagrGC::stats(self)
    }
}
