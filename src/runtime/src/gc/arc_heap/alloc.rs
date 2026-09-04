//! `ArcMagrGC` 分配尾部：region 分配 + OOM 兜底 + 压力检查 + size 估算/查询。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use std::sync::Arc;
use crate::metadata::{ScriptObject, Value};
use crate::metadata::types::ClosureData;
use crate::gc::refs::{GcRef};
use crate::gc::var_region::{BlockType, GcBlockHeader};
use crate::gc::types::{AllocKind, AllocSample, GcEvent};

impl crate::gc::arc_heap::ArcMagrGC {
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
    pub(super) fn record_alloc(&self, _value: &Value, kind_fn: impl FnOnce() -> AllocKind, size: usize) {
        use std::sync::atomic::Ordering;
        // 1. 更新 stats —— **add-gc-tlab (option B)**: lock-free atomic counters (no inner lock).
        //    `Relaxed` is sufficient: these are monotone heuristic counters, not synchronization.
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.used_bytes.fetch_add(size as u64, Ordering::Relaxed);
        // 2. 压力检查（可能触发 GcEvent）—— reads the atomic used_bytes.
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

    /// **add-gc-tlab (option B)**: read the lock-free live `used_bytes` counter.
    #[inline]
    pub(super) fn used_bytes_atomic(&self) -> u64 {
        self.used_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// **add-gc-tlab (option B)**: saturating subtract on the atomic `used_bytes`, used by the
    /// collect paths to account reclaimed bytes. A CAS loop keeps the saturation correct even
    /// though collect runs under STW (no concurrent alloc) — cheap and future-proof.
    pub(super) fn sub_used_bytes(&self, n: u64) {
        use std::sync::atomic::Ordering;
        let mut cur = self.used_bytes.load(Ordering::Relaxed);
        loop {
            let new = cur.saturating_sub(n);
            match self.used_bytes.compare_exchange_weak(cur, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }

    /// **add-gc-tlab (stage 2/3)**: the lock-free counterpart of [`record_alloc`]
    /// for the TLAB fast path. Bumps the atomic stat counters and (only when a
    /// sampler is installed, gated by `sampler_active`) fires the alloc sampler.
    ///
    /// **Pressure / auto-collect (fix cross_thread_smoke regression)**: when the
    /// heap is **unbounded** (`max_bytes` unset — the common production case, and
    /// the `ArcMagrGC::new()` scaling benchmark) this stays fully lock-free — no
    /// pressure work at all. When a heap **limit is set**, it does the accurate
    /// **per-object** pressure check + deferred-collect arming (same as the locked
    /// `record_alloc` path). A retire-granularity check alone would miss the
    /// threshold — small allocs may never fill a chunk nor GC-park, so a bounded
    /// heap would never auto-collect (regressed `cross_thread_smoke`'s
    /// `auto_collect_triggers_via_safepoint_no_race`). `max_bytes_atomic` gates
    /// this lock-free, so the zero-lock fast path is preserved for unbounded heaps.
    ///
    /// MUST be called **outside** the TLAB borrow (`with_current_tlab`): the
    /// bounded path may run `maybe_auto_collect`, whose (unwired) inline-collect
    /// branch retires the thread TLAB → would re-enter the borrow. Armed heaps are
    /// always wired (defer, no inline collect), but calling from outside keeps it
    /// sound regardless.
    pub(super) fn record_alloc_fast(&self, kind_fn: impl FnOnce() -> AllocKind, size: usize) {
        use std::sync::atomic::Ordering;
        self.allocations.fetch_add(1, Ordering::Relaxed);
        self.used_bytes.fetch_add(size as u64, Ordering::Relaxed);
        if self.sampler_active.load(Ordering::Relaxed) {
            let sampler = self.inner.lock().alloc_sampler.clone();
            if let Some(s) = sampler {
                s(&AllocSample {
                    kind: kind_fn(),
                    size_bytes: size,
                    timestamp_us: Self::now_us(),
                });
            }
        }
        if self.max_bytes_atomic.load(Ordering::Relaxed) != u64::MAX {
            self.check_pressure(size as u64);
            self.maybe_auto_collect();
        }
    }

    /// **add-gc-tlab (stage 2)**: object-region fast path. Bump-fills the
    /// calling thread's TLAB object claim lock-free; borrows a fresh chunk
    /// (retiring the full one) when needed. Returns `Err(obj)` — handing the
    /// object back for the ambient locked path — only when this thread's TLAB
    /// still holds claims bound to a *different* heap (multi-heap test edge).
    pub(super) fn tlab_alloc_object(
        &self,
        obj: ScriptObject,
        td: &Arc<crate::metadata::TypeDesc>,
    ) -> Result<Value, ScriptObject> {
        // Fill inside the TLAB borrow; record OUTSIDE it (record_alloc_fast may run
        // maybe_auto_collect when the heap is bounded — must not re-enter the borrow).
        let value = match crate::gc::tlab::with_current_tlab(|tlab| {
            // Bind an unbound TLAB to this heap; decline if it holds foreign claims.
            if tlab.heap_epoch != self.epoch {
                if tlab.is_unbound() {
                    tlab.heap_epoch = self.epoch;
                } else {
                    return Err(obj);
                }
            }
            // Ensure the object claim has a free slot.
            if tlab.obj.as_ref().map_or(true, |c| !c.has_room()) {
                if let Some(full) = tlab.obj.take() {
                    self.region_object.lock().retire_chunk(&full);
                }
                tlab.obj = Some(self.region_object.lock().borrow_chunk());
            }
            // SAFETY: fill only fails on a full claim; we just ensured room.
            let (entry_ptr, generation) = tlab.obj.as_mut().unwrap().fill(obj).unwrap();
            // SAFETY: entry_ptr is a fresh, stable slot; generation matches.
            let gc = unsafe { GcRef::from_region_entry(entry_ptr, generation) };
            Ok(Value::Object(gc))
        }) {
            Ok(v) => v,
            Err(o) => return Err(o),
        };
        let size = self.object_size_bytes(&value);
        self.record_alloc_fast(|| AllocKind::Object { class: td.name.clone() }, size);
        Ok(value)
    }

    /// **add-gc-tlab (stage 2)**: array-region fast path (mirror of
    /// [`tlab_alloc_object`] for `region_array`).
    pub(super) fn tlab_alloc_array(
        &self,
        obj: crate::metadata::types::ArrayObj,
    ) -> Result<Value, crate::metadata::types::ArrayObj> {
        let elem_count = obj.len();
        // Fill inside the TLAB borrow; record OUTSIDE it (see `record_alloc_fast`).
        let value = match crate::gc::tlab::with_current_tlab(|tlab| {
            if tlab.heap_epoch != self.epoch {
                if tlab.is_unbound() {
                    tlab.heap_epoch = self.epoch;
                } else {
                    return Err(obj);
                }
            }
            if tlab.arr.as_ref().map_or(true, |c| !c.has_room()) {
                if let Some(full) = tlab.arr.take() {
                    self.region_array.lock().retire_chunk(&full);
                }
                tlab.arr = Some(self.region_array.lock().borrow_chunk());
            }
            let (entry_ptr, generation) = tlab.arr.as_mut().unwrap().fill(obj).unwrap();
            let gc = unsafe { GcRef::from_region_entry(entry_ptr, generation) };
            Ok(Value::Array(gc))
        }) {
            Ok(v) => v,
            Err(o) => return Err(o),
        };
        let size = self.object_size_bytes(&value);
        self.record_alloc_fast(|| AllocKind::Array { elem_count }, size);
        Ok(value)
    }

    /// **add-gc-tlab (stage 3)**: variable-length region fast path (strings /
    /// closures / var blocks). Bump-fills the calling thread's TLAB var claim
    /// lock-free; borrows a fresh chunk (retiring the full one) when needed.
    /// Returns `None` for an **oversized** block (→ locked `alloc` path, which
    /// carves a dedicated chunk) or when this thread's TLAB holds foreign-heap
    /// claims. The caller writes the payload into the returned block.
    pub(super) fn tlab_alloc_var(
        &self,
        payload: usize,
        block_type: BlockType,
    ) -> Option<crate::gc::var_region::VarGcRef> {
        let (footprint, size_class) = crate::gc::var_region::class_for(payload);
        if size_class == crate::gc::var_region::OVERSIZED_CLASS {
            return None; // oversized → dedicated-chunk locked path
        }
        crate::gc::tlab::with_current_tlab(|tlab| {
            if tlab.heap_epoch != self.epoch {
                if tlab.is_unbound() {
                    tlab.heap_epoch = self.epoch;
                } else {
                    return None;
                }
            }
            if tlab.var.as_ref().map_or(true, |c| !c.has_room(footprint)) {
                if let Some(mut full) = tlab.var.take() {
                    self.region_var.lock().retire_chunk(&mut full);
                }
                tlab.var = Some(self.region_var.lock().borrow_chunk());
            }
            // A fresh chunk (cap == CHUNK_BYTES) fits any non-oversized footprint,
            // so fill only returns None on a genuinely full reused claim — which
            // the has_room check above already excluded.
            tlab.var.as_mut().unwrap().fill(payload, footprint, size_class, block_type)
        })
    }

    /// **add-gc-tlab (stage 3)**: acquire a fresh var block of `payload` bytes —
    /// via the lock-free TLAB fast path when armed + non-strict + non-oversized,
    /// else the locked `VarRegion::alloc`. Returns the block handle plus whether
    /// the fast path was taken (so the caller records via the lock-free
    /// `record_alloc_fast` vs the locked `record_alloc` + `maybe_auto_collect`).
    /// The caller writes the payload into the returned (zeroed) block.
    #[inline]
    pub(super) fn acquire_var_block(
        &self,
        payload: usize,
        block_type: BlockType,
    ) -> (crate::gc::var_region::VarGcRef, bool) {
        if crate::gc::tlab::is_armed()
            && !self.strict_oom_atomic.load(std::sync::atomic::Ordering::Relaxed)
        {
            if let Some(vref) = self.tlab_alloc_var(payload, block_type) {
                return (self.shade_var_newborn(vref), true); // 3.2 allocate-black
            }
        }
        (self.shade_var_newborn(self.region_var.lock().alloc(payload, block_type)), false)
    }

    /// **add-gc-tlab (stage 2/3)**: retire the calling thread's TLAB — merge all
    /// borrowed chunks' filled contents back into their regions and drop the
    /// claims, leaving the TLAB unbound. Idempotent (no-op when already
    /// unbound). Called at safepoint park, before a collector marks, and at
    /// `VmContext::drop`. See the `MagrGC::retire_thread_tlab` contract.
    pub(super) fn retire_thread_tlab(&self) {
        crate::gc::tlab::with_current_tlab(|tlab| {
            if tlab.heap_epoch != self.epoch {
                // Not bound to this heap (or already unbound) → nothing of ours.
                return;
            }
            if let Some(claim) = tlab.obj.take() {
                self.region_object.lock().retire_chunk(&claim);
            }
            if let Some(claim) = tlab.arr.take() {
                self.region_array.lock().retire_chunk(&claim);
            }
            if let Some(mut claim) = tlab.var.take() {
                self.region_var.lock().retire_chunk(&mut claim);
            }
            tlab.heap_epoch = 0; // unbound
        });
    }

    /// Size estimate helpers for sweep_phase — operate on already-
    /// locked inner data (avoids re-locking via object_size_bytes path).
    pub(super) fn script_object_size_estimate(obj: &ScriptObject) -> u64 {
        use std::mem::size_of;
        // slots is `Box<[Value]>` — len == actual allocation (no excess
        // capacity to charge separately).
        (size_of::<Value>() + size_of::<ScriptObject>()
            + obj.bytes().len() + obj.refs().len() * size_of::<Value>()) as u64
    }

    pub(super) fn array_size_estimate(arr: &crate::metadata::types::ArrayObj) -> u64 {
        use std::mem::size_of;
        // packed-primitive-arrays: `elem_storage_bytes` is per-backing (byte[] 1B
        // vs Boxed 24B/elem), so packed arrays report their true smaller size.
        (size_of::<Value>() + size_of::<crate::metadata::types::ArrayObj>()
            + arr.elem_storage_bytes()) as u64
    }

    /// **Phase 3-OOM**: 检查在当前 used_bytes 基础上再分配 `size` 字节是否会
    /// 越过 max_heap_bytes 上限。仅在 strict_oom 模式下使用。
    pub(super) fn would_oom_after_alloc(&self, size: u64) -> (bool, u64) {
        let (strict, max) = { let i = self.inner.lock(); (i.strict_oom, i.stats.max_bytes) };
        if !strict { return (false, 0); }
        let Some(limit) = max else { return (false, 0); };
        // add-gc-tlab (option B): used_bytes now lives on the atomic, not inner.stats.
        let after = self.used_bytes_atomic().saturating_add(size);
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
    pub(super) fn maybe_auto_collect(&self) {
        let (max_opt, last, paused) = {
            let i = self.inner.lock();
            (i.stats.max_bytes, i.last_auto_collect_used, i.pause_count > 0)
        };
        let used = self.used_bytes_atomic();
        if paused { return; }
        let Some(limit) = max_opt else { return };
        let cfg = crate::config::runtime_config();
        let near_threshold = (limit as f64 * cfg.gc_near_limit_ratio) as u64;
        if used < near_threshold { return; }
        let throttle_delta = (limit as f64 * cfg.gc_throttle_ratio) as u64;
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

    /// **Phase 3d**: collect 完成后，若 used 已降到 near-limit 阈值以下，
    /// reset `near_limit_warned` 让下次跨阈值能再发 NearHeapLimit 事件。
    /// 阈值比率来自 `Z42_GC_NEAR_LIMIT_RATIO`（默认 0.90），与 [`Self::check_pressure`]
    /// 触发 NearHeapLimit 的同一比率——保证「发一次事件」与「重置事件闩」用同一阈值。
    pub(super) fn maybe_reset_near_limit_warned(&self) {
        let near_ratio = crate::config::runtime_config().gc_near_limit_ratio;
        let used = self.used_bytes_atomic(); // add-gc-tlab (option B)
        let mut i = self.inner.lock();
        let Some(limit) = i.stats.max_bytes else { return };
        let near_threshold = (limit as f64 * near_ratio) as u64;
        if used < near_threshold {
            i.near_limit_warned = false;
        }
    }

    pub(super) fn check_pressure(&self, requested: u64) {
        let used = self.used_bytes_atomic(); // add-gc-tlab (option B)
        let (max, near_warned) = {
            let i = self.inner.lock();
            (i.stats.max_bytes, i.near_limit_warned)
        };
        let Some(limit) = max else { return };
        let cfg = crate::config::runtime_config();
        let near_threshold     = (limit as f64 * cfg.gc_near_limit_ratio) as u64;
        let pressure_threshold = (limit as f64 * cfg.gc_pressure_ratio) as u64;

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

    /// 共享对象分配尾部：把已建好的 `ScriptObject` alloc 进 region + OOM 兜底 + record。
    /// `alloc_object`（struct_bytes 由 `type_desc.inline_regions()` 定尺）与 `alloc_boxed_prim`
    /// （struct_bytes 由调用方按 wrapper 标量宽度显式定尺，unify Phase 2 R3）复用本尾部。
    pub(super) fn finish_alloc(&self, obj: ScriptObject) -> Value {
        let td_for_record = Arc::clone(&obj.type_desc);
        // **add-gc-tlab (stage 2)**: chunk-exclusive TLAB fast path (lock-free
        // bump-fill) — only on a TLAB-armed thread (has a live VmContext) and
        // not in strict-OOM mode (D6 needs the per-object refund path). `Err`
        // hands the object back for the ambient path.
        let obj = if crate::gc::tlab::is_armed()
            && !self.strict_oom_atomic.load(std::sync::atomic::Ordering::Relaxed)
        {
            match self.tlab_alloc_object(obj, &td_for_record) {
                Ok(value) => return self.shade_newborn(value), // 3.2 allocate-black
                Err(o) => o,
            }
        } else {
            obj
        };
        // **add-custom-allocator P1 (2026-05-22)**: alloc into region (ambient
        // locked path — strict-OOM or a foreign-heap TLAB).
        // Region::alloc returns a stable handle; resolve gives us the
        // entry pointer for GcRef construction.
        let (entry_ptr, generation, handle) = {
            let mut region = self.region_object.lock();
            let handle = region.alloc(obj);
            let entry: std::ptr::NonNull<crate::gc::region::RegionEntry<ScriptObject>> =
                std::ptr::NonNull::from(region.resolve(handle));
            (entry, handle.generation, handle)
        };
        // SAFETY: handle was just produced by region.alloc; entry ptr
        // is stable for entry lifetime; generation matches.
        let gc = unsafe { GcRef::from_region_entry(entry_ptr, generation) };
        let value = self.shade_newborn(Value::Object(gc)); // 3.2 allocate-black

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

    /// unify-gc-heap PR-2: allocate a capturing closure's [`ClosureData`] into the
    /// variable-length GC region (`region_var`) and return a `Value::Closure` handle. The
    /// block's drop-glue drops `fn_name` when the closure is swept. Mirrors `finish_alloc`'s
    /// record + auto-collect tail (no strict-OOM refund path — closure blocks are tiny and the
    /// env array's own alloc already went through the OOM gate). Exposed via the `MagrGC` trait
    /// (`alloc_closure`) so `ctx.heap()` callers reach it through `&dyn MagrGC`.
    pub(super) fn alloc_closure_in_region(&self, data: ClosureData) -> Value {
        // add-gc-tlab (stage 3): lock-free var TLAB when armed; else locked path.
        let (vref, fast) = self.acquire_var_block(std::mem::size_of::<ClosureData>(), BlockType::Closure);
        // SAFETY: fresh block sized exactly for `ClosureData`; write before any typed read.
        unsafe { vref.payload_as_ptr::<ClosureData>().write(data) };
        let value = Value::Closure(vref);
        let size = GcBlockHeader::DATA_OFFSET + std::mem::size_of::<ClosureData>();
        if fast {
            self.record_alloc_fast(|| AllocKind::Object { class: "<closure>".to_string() }, size);
        } else {
            self.record_alloc(&value, || AllocKind::Object { class: "<closure>".to_string() }, size);
            self.maybe_auto_collect();
        }
        value
    }

    /// unify-gc-heap PR-4: allocate an immutable UTF-8 string into the variable-length
    /// GC region (`region_var`, `BlockType::Str`) and return a thin [`Str`] handle
    /// (`{GcBlockHeader, inline UTF-8 bytes}`, one alloc). Mirrors `alloc_closure_in_region`'s
    /// record + auto-collect tail. No drop-glue / OOM-refund: a `Str` block is a POD leaf
    /// (`var_drop_glue` already treats `BlockType::Str` as nothing-to-drop). Exposed via the
    /// `MagrGC` trait (`alloc_str`) so ambient-heap callers (`Str::new`) reach it.
    pub(super) fn alloc_str_in_region(&self, s: &str) -> crate::metadata::vstr::Str {
        // add-gc-tlab (stage 3): lock-free var TLAB when armed; else locked path.
        let (vref, fast) = self.acquire_var_block(s.len(), BlockType::Str);
        // SAFETY: fresh block sized for exactly `s.len()` bytes; write the UTF-8 bytes
        // into the zeroed payload (derived from the raw block header, D8) before any read.
        unsafe {
            std::ptr::copy_nonoverlapping(s.as_ptr(), vref.payload_as_ptr::<u8>(), s.len());
        }
        let size = GcBlockHeader::DATA_OFFSET + s.len();
        // `record_alloc`'s `_value` is unused (only the sampler's lazy `kind_fn` matters), so
        // pass `Null` rather than materialize a throwaway `Value::Str`.
        if fast {
            self.record_alloc_fast(|| AllocKind::Object { class: "<string>".to_string() }, size);
        } else {
            self.record_alloc(&Value::Null, || AllocKind::Object { class: "<string>".to_string() }, size);
            self.maybe_auto_collect();
        }
        crate::metadata::vstr::Str::from_var_ref(vref)
    }

    /// fuse-str-concat-alloc: region-alloc a `BlockType::Str` block sized
    /// `a.len() + b.len()` and fill it by copying both segments back-to-back —
    /// the fused counterpart of `alloc_str_in_region(&format!("{a}{b}"))` that
    /// skips the intermediate `String`. Same `record_alloc` / `maybe_auto_collect`
    /// bookkeeping as the single-string path (the concatenation is one block).
    pub(super) fn alloc_str_concat2_in_region(&self, a: &str, b: &str) -> crate::metadata::vstr::Str {
        let total = a.len() + b.len();
        // add-gc-tlab (stage 3): lock-free var TLAB when armed; else locked path.
        let (vref, fast) = self.acquire_var_block(total, BlockType::Str);
        // SAFETY: fresh block sized for exactly `total` bytes; write `a` then `b`
        // into the zeroed payload (raw block header, D8) before any read. Both
        // segments are valid UTF-8, so their concatenation is valid UTF-8.
        unsafe {
            let dst = vref.payload_as_ptr::<u8>();
            std::ptr::copy_nonoverlapping(a.as_ptr(), dst, a.len());
            std::ptr::copy_nonoverlapping(b.as_ptr(), dst.add(a.len()), b.len());
        }
        let size = GcBlockHeader::DATA_OFFSET + total;
        if fast {
            self.record_alloc_fast(|| AllocKind::Object { class: "<string>".to_string() }, size);
        } else {
            self.record_alloc(&Value::Null, || AllocKind::Object { class: "<string>".to_string() }, size);
            self.maybe_auto_collect();
        }
        crate::metadata::vstr::Str::from_var_ref(vref)
    }

    /// add-reflection-array-element-type: shared array allocation over an
    /// `ArrayObj` (element type + elems). Both `alloc_array` (untyped) and
    /// `alloc_array_typed` funnel through here.
    pub(super) fn alloc_array_obj(&self, obj: crate::metadata::types::ArrayObj) -> Value {
        let elem_count = obj.len();
        // **add-gc-tlab (stage 2)**: TLAB fast path — armed thread + non-strict (D6).
        let obj = if crate::gc::tlab::is_armed()
            && !self.strict_oom_atomic.load(std::sync::atomic::Ordering::Relaxed)
        {
            match self.tlab_alloc_array(obj) {
                Ok(value) => return self.shade_newborn(value), // 3.2 allocate-black
                Err(o) => o,
            }
        } else {
            obj
        };
        let (entry_ptr, generation, handle) = {
            let mut region = self.region_array.lock();
            let handle = region.alloc(obj);
            let entry: std::ptr::NonNull<crate::gc::region::RegionEntry<crate::metadata::types::ArrayObj>> =
                std::ptr::NonNull::from(region.resolve(handle));
            (entry, handle.generation, handle)
        };
        let gc = unsafe { GcRef::from_region_entry(entry_ptr, generation) };
        let value = self.shade_newborn(Value::Array(gc)); // 3.2 allocate-black

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

    pub(super) fn object_size_bytes(&self, value: &Value) -> usize {
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
                    + obj.bytes().len()
                    + obj.refs().len() * size_of::<Value>()
            }
            // impl-lambda-l2: FuncRef holds the function name; no managed heap
            // allocation beyond the string buffer.
            Value::FuncRef(name) => size_of::<Value>() + name.len(),
            // impl-closure-l3-core: Closure carries a heap-allocated env (Vec<Value>);
            // its size is the env's storage plus the function-name string.
            Value::Closure(c) => {
                let data = crate::metadata::types::closure_data_of(c);
                size_of::<Value>()
                    + size_of::<crate::metadata::ClosureData>()
                    + size_of::<Vec<Value>>()
                    + data.env.borrow().elem_storage_bytes()
                    + data.fn_name.len()   // unify-gc-heap PR-5: fn_name is a GC `Str` (bytes in its block)
            }
            // make-value-copy: `StackClosure` / `Ref` / `StructRefHeap` are now
            // transient-arena handles (payload owned by the per-context arena, freed by
            // frame-exit truncation, not the GC heap) — the handle in a Value is just an
            // (idx, frame_id) pair, exactly like the stack / struct-arena handles below.
            //
            // add-escape-analysis-stack-alloc: stack objects/arrays live in the
            // per-context arena, not the GC heap — object_size_bytes is a heap-alloc
            // accounting hook and is never called on them; the arm exists only for
            // exhaustiveness. The handle itself is just a (idx, frame_id) pair.
            // add-struct-value-semantics: struct blob lives in the per-context struct arena.
            Value::StackObject { .. } | Value::StackArray { .. }
            | Value::StructRef { .. }
            | Value::StackClosure { .. } | Value::Ref { .. }
            | Value::PinnedView { .. } | Value::StructRefHeap { .. } => size_of::<Value>(),
            // add-boxed-struct-identity (P4b, 路 B2): boxed struct 现是共享 `ScriptObject`——按对象
            // 计其 struct_bytes/struct_refs（与 Object 臂同）。对象本体在 region_object，alloc 时已计一次；
            // 此臂给按 Value 计尺寸的诚实值。
            Value::BoxedStruct(gc) => {
                let obj = gc.borrow();
                size_of::<Value>() + size_of::<ScriptObject>()
                    + obj.bytes().len()
                    + obj.refs().len() * size_of::<Value>()
            }
        }
    }
}
