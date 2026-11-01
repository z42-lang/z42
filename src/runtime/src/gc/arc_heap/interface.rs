//! `impl MagrGC for ArcMagrGC` —— GC 公共 trait 接口（薄委托层）。
//! 重方法体下沉到 concern 子模块的 inherent 方法（refactor-arc-heap-modularization）。

use std::sync::Arc;
use crate::object::{NativeData, TypeDesc, Value};
use crate::object::OwnedObject;
use crate::object::ClosureData;
use crate::gc::heap::MagrGC;
use crate::gc::types::{AllocSamplerFn, CollectStats, FinalizerFn, FrameMark, GcHandleKind, GcObserver, HeapSnapshot, HeapStats, ObserverId, RootHandle, WeakRef, WeakRefInner};

use crate::gc::arc_heap::{ArcMagrGC, HandleEntry, ContextReclaimHook, ExternalRootScanner, CategorizedRootScanner};

impl MagrGC for ArcMagrGC {
    /// **fix-wasm-string-ops**: this heap's process-unique, never-reused epoch (assigned at
    /// construction). See [`MagrGC::heap_epoch`] and [`crate::object::string::str_meta`].
    #[inline]
    fn heap_epoch(&self) -> u64 {
        self.epoch
    }

    fn var_chunk_stamp(&self, b: crate::object::header::VarGcRef) -> Option<(u32, u32)> {
        let (chunk, epoch) = self.weak_stamp(b);
        (chunk != u32::MAX).then_some((chunk, epoch))
    }

    fn var_chunk_unchanged(&self, chunk: u32, epoch: u32) -> bool {
        self.region_var.lock().chunk_epoch(chunk) == Some(epoch)
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

    /// **add-lazy-context-unload **: wire the collectible-context
    /// reclaimer. Drives `Unloading` context reclamation on major STW collects.
    fn set_context_reclaimer(&self, hook: ContextReclaimHook) {
        *self.context_reclaimer.lock() = Some(hook);
    }

    /// **add-heap-retention-diagnostics **: wire the categorized
    /// root scanner (for `retention_roots` L2 category-level root reporting).
    fn set_categorized_root_scanner(&self, scanner: CategorizedRootScanner) {
        *self.categorized_root_scanner.lock() = Some(scanner);
    }

    /// **L1**: direct heap referrers of `target` (a heap object's data ptr).
    /// Forces a full GC first so only reachable objects are reported.
    fn retention_direct_referrers(&self, target: usize) -> Vec<crate::gc::retention::RetainerInfo> {
        self.collect_full();
        self.build_retention_graph().direct_referrers(target)
    }

    /// **L2**: GC roots retaining `target` (category-level), via reverse BFS.
    /// Forces a full GC first for accuracy.
    fn retention_roots(&self, target: usize) -> Vec<crate::gc::retention::RootInfo> {
        self.collect_full();
        self.build_retention_graph().retaining_roots(target)
    }

    /// **add-gc-safepoint-auto-threshold **: wire the
    /// AtomicBool that `maybe_auto_collect` should set on pressure trip
    /// (deferring the actual collect to the next safepoint).
    fn set_external_needs_collect_flag(&self, flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        *self.external_needs_collect.lock() = Some(flag);
    }

    fn set_safepoint_poke(&self, poke: std::sync::Arc<dyn Fn() + Send + Sync>) {
        *self.safepoint_poke.lock() = Some(poke);
    }

    // ── 1. Allocation ────────────────────────────────────────────────────────

    fn alloc_object(
        &self,
        type_desc: Arc<TypeDesc>,
        slots: Vec<Value>,
        native: NativeData,
    ) -> Value {
        // unify-object-byte-layout (PR-2): `slots` is **initial field values by slot index**.
        // Zeroed fields are every field's default, so with no initial values (and no native
        // backing) the object is built straight in its block. Otherwise it is built outside the
        // heap first: encoding a value can allocate (a boxed primitive in an `object` field), and
        // no allocation may happen between taking an object's block and publishing it.
        if slots.is_empty() && matches!(native, NativeData::None) {
            return self.alloc_instance_in_region(type_desc, crate::object::TypeArgs::empty());
        }
        let obj = OwnedObject::with_native(type_desc, native);
        for (i, v) in slots.into_iter().enumerate() {
            obj.set_field_value(i, &v);
        }
        self.alloc_owned_in_region(obj)
    }

    fn alloc_script_object(&self, obj: OwnedObject) -> Value {
        self.alloc_owned_in_region(obj)
    }

    fn alloc_instance(&self, type_desc: Arc<TypeDesc>, type_args: crate::object::TypeArgs) -> Value {
        self.alloc_instance_in_region(type_desc, type_args)
    }

    /// unify Phase 2 R3（装箱统一）：装箱一个基元（整数）成堆对象 + 引用身份——载荷装裸标量（LE 字节，
    /// 宽度由调用方按 wrapper 定，见 `corelib/convert.rs::box_prim_to_heap`），**不走
    /// `type_desc.inline_regions()`**（基元 wrapper 如 Int32 是零字段 struct，layout size=0，装不下标量）。
    /// 返 `Value::Object`，调用方包成 `Value::BoxedStruct`（与 struct 装箱同款引用身份/GC/反射路径）。
    fn alloc_boxed_prim(&self, type_desc: Arc<TypeDesc>, scalar_bytes: Box<[u8]>) -> Value {
        self.alloc_boxed_bytes_in_region(type_desc, &scalar_bytes)
    }

    fn alloc_array_with(
        &self,
        desc: &'static crate::object::ArrayDesc,
        len: usize,
        layout: Option<Arc<crate::object::StructTypeLayout>>,
        init: &mut dyn FnMut(&crate::object::ArrayObj),
    ) -> Value {
        self.alloc_array_block(desc, len, layout, init)
    }

    #[inline(always)]
    fn as_dyn(&self) -> &dyn MagrGC {
        self
    }

    fn alloc_closure(&self, data: ClosureData) -> Value {
        self.alloc_closure_in_region(data)
    }

    fn alloc_str(&self, s: &str) -> crate::object::string::vstr::Str {
        self.alloc_str_in_region(s)
    }

    fn alloc_str_concat2(&self, a: &str, b: &str) -> crate::object::string::vstr::Str {
        self.alloc_str_concat2_in_region(a, b)
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
    // **add-write-barriers **: ArcMagrGC overrides the trait
    // methods. Production STW-mode is no-op (matches the pre-this-spec
    // baseline); `#[cfg(test)]` always fires the test observer regardless
    // of mode.
    //
    // Under `GenerationalMarkSweep` the override records old→young writes
    // in the remembered set (`barrier.rs`).
    //
    // Caller contract: invoke ONLY when `new.is_heap_ref() == true`
    // (Decision 1 of add-write-barriers). The generational arm
    // `debug_assert!`s the contract.

    fn write_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        ArcMagrGC::write_barrier_field(self, owner, slot, new)
    }

    fn write_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
        ArcMagrGC::write_barrier_array_elem(self, arr, idx, new)
    }

    /// `write_barrier_field` acts only in generational mode, for an owner at least
    /// `promotion_age` old that is not yet remembered (`barrier.rs`).
    #[cfg(feature = "jit")]
    fn jit_barrier_bytes(&self) -> Option<(*const std::sync::atomic::AtomicU8, *const std::sync::atomic::AtomicU8)> {
        Some((&self.mode, &self.promotion_age))
    }

    #[cfg(test)]
    fn is_remembered_for_test(&self, owner: &Value) -> bool {
        match owner {
            Value::Object(o) | Value::BoxedStruct(o) => o.var_ref().is_remembered(),
            Value::Array(a) => a.var_ref().is_remembered(),
            _ => false,
        }
    }

    fn verify_card_invariant(&self) -> Result<(), String> {
        ArcMagrGC::verify_card_invariant(self)
    }

    // ── 4. Object Model ──────────────────────────────────────────────────────

    fn object_size_bytes(&self, value: &Value) -> usize {
        ArcMagrGC::object_size_bytes(self, value)
    }

    fn scan_object_refs(&self, value: &Value, visitor: &mut dyn FnMut(&Value)) {
        // unify-gc-heap PR-5: read-only graph enumeration is now the `for_marking = false`
        // mode of the single-source `Value::visit_gc_children` (no mark side effects; a
        // closure's captured refs are descended directly). Snapshot / retention only.
        value.visit_gc_children(None, visitor);
    }

    // ── 5. Collection control ────────────────────────────────────────────────

    /// Current GC mode. Read on
    /// the barrier hot path + `run_cycle_collection` entry. `Relaxed`
    /// ordering — mode changes are observed at the next collect / next
    /// write, not synchronized with anything else.
    fn mode(&self) -> crate::gc::GcMode {
        crate::gc::GcMode::from_u8(self.mode.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Switch GC mode at runtime.
    /// Takes effect at the next collect; in-progress collects complete
    /// with their original mode. Lock-free `store(Relaxed)` — fast path
    /// on the rare config call.
    fn set_mode(&self, mode: crate::gc::GcMode) {
        self.mode.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
        // fix-young-list-only-when-generational: the regions' young set is minor
        // GC's index, and minor GC runs only in this mode — so the regions'
        // maintenance flag is part of the mode, set here in the same breath as
        // the store. Switching *to* generational rebuilds the set from the
        // live entries, so a heap that has already allocated collects correctly.
        let generational = mode == crate::gc::GcMode::GenerationalMarkSweep;
        // The young set's bitmaps are only allocated while generational.
        self.region_var.lock().set_generational(generational);
        // arm-gc-by-default: the two modes size their growth gate differently (a nursery vs
        // a full allowance), so the cached trip point has to be recomputed.
        self.rearm_auto_collect();
    }

    /// **add-custom-allocator P2**: explicit finalize; impl in `ArcMagrGC::finalize_now`.
    fn finalize_now(&self, value: &Value) -> bool {
        ArcMagrGC::finalize_now(self, value)
    }

    /// VmContext-aware collect; impl in `ArcMagrGC::collect_cycles_with_context`.
    fn collect_cycles_with_context(&self, ctx: &crate::vm_context::VmContext) {
        ArcMagrGC::collect_cycles_with_context(self, ctx)
    }

    fn collect_cycles(&self) {
        ArcMagrGC::collect_cycles(self)
    }

    fn force_collect(&self) -> CollectStats {
        ArcMagrGC::force_collect(self)
    }

    fn collect_full(&self) -> CollectStats {
        ArcMagrGC::collect_full(self)
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
        drop(i);
        // arm-gc-by-default: the soft cap squeezes the allowance, so the cached trip point is
        // stale as soon as it changes. (`rearm_auto_collect` is lock-free but takes no chances
        // with a guard still alive — hence the explicit `drop` above.)
        self.rearm_auto_collect();
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

    fn park_thread_tlab(&self, slot: &crate::gc::tlab::ParkedTlabSlot) {
        ArcMagrGC::park_thread_tlab(self, slot)
    }

    fn unpark_thread_tlab(&self, slot: &crate::gc::tlab::ParkedTlabSlot) {
        ArcMagrGC::unpark_thread_tlab(self, slot)
    }

    fn retire_parked_tlab(&self, slot: &crate::gc::tlab::ParkedTlabSlot) {
        if let Some(p) = slot.take() { ArcMagrGC::retire_claims(self, p) }
    }

    // ── 7. Finalization ──────────────────────────────────────────────────────

    /// Finalizers live in a side table keyed by block address (object model R6 / R9); the sweep
    /// fires them through the flagged-dead list.
    fn register_finalizer(&self, value: &Value, fin: FinalizerFn) {
        if let Some(b) = Self::owner_block(value) {
            self.set_block_finalizer(b, fin);
        } // 原子值无 finalizer
    }

    fn cancel_finalizer(&self, value: &Value) {
        if let Some(b) = Self::owner_block(value) {
            let _ = self.take_block_finalizer(b);
        }
    }

    // ── 8. Weak references ───────────────────────────────────────────────────

    fn make_weak(&self, value: &Value) -> Option<WeakRef> {
        match value {
            Value::Object(gc) => Some(WeakRef {
                inner: WeakRefInner::Object(self.downgrade_object(gc)),
            }),
            Value::Array(gc) => Some(WeakRef {
                inner: WeakRefInner::Array(self.downgrade_array(gc)),
            }),
            _ => None,
        }
    }

    fn upgrade_weak(&self, weak: &WeakRef) -> Option<Value> {
        let v = match &weak.inner {
            WeakRefInner::Object(w) => self.upgrade_weak_object(w).map(Value::Object),
            WeakRefInner::Array (w) => self.upgrade_weak_array(w).map(Value::Array),
        };
        // add-incremental-major-gc M2a/M2b: shade while marking; refuse a doomed target while sweeping.
        v.filter(|v| self.admit_resurrected(v))
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
            (Value::Object(g), GcHandleKind::Strong) => HandleEntry::StrongObject(*g),
            (Value::Array (g), GcHandleKind::Strong) => HandleEntry::StrongArray (*g),
            (Value::Object(g), GcHandleKind::Weak)   => HandleEntry::WeakObject(self.downgrade_object(g)),
            (Value::Array (g), GcHandleKind::Weak)   => HandleEntry::WeakArray (self.downgrade_array(g)),
            // Atomic Strong: just clone the Value into the slot.
            (v, GcHandleKind::Strong) => HandleEntry::StrongAtomic(v.clone()),
            // Atomic Weak: rejected — atomics aren't Rc-backed, can't weak-ref.
            (_, GcHandleKind::Weak) => return 0,
        };
        self.inner.lock().handle_slab.alloc(entry)
    }

    fn handle_target(&self, slot: u64) -> Option<Value> {
        // The entry is copied out first: upgrading a weak object reference takes the region lock,
        // which must not nest inside `inner`.
        let entry = self.inner.lock().handle_slab.get(slot).cloned();
        let v = entry.and_then(|e| e.target(self));
        // add-incremental-major-gc M2a/M2b: weak handles — see `ArcMagrGC::admit_resurrected`.
        v.filter(|v| self.admit_resurrected(v))
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
        // Every alive object / array block, once each (the region walk visits each block once).
        for v in self.snapshot_live_from_registry() {
            visitor(&v);
        }
    }

    // ── 11. Stats ────────────────────────────────────────────────────────────

    fn stats(&self) -> HeapStats {
        ArcMagrGC::stats(self)
    }
}
