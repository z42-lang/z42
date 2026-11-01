//! `ArcMagrGC` 分配尾部：region 分配 + OOM 兜底 + 压力检查 + size 估算/查询。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

use crate::object::Value;
use crate::object::ClosureData;
use crate::object::header::{BlockType, GcBlockHeader};
use crate::gc::types::{AllocKind, AllocSample, GcEvent};

impl crate::gc::arc_heap::ArcMagrGC {
    /// alloc 通用通路：bump stats + 检查压力 + 触发 sampler。
    ///
    /// **add-custom-allocator P1 **: previously pushed a
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
        crate::gc::safepoint::debug_assert_not_native_parked();
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
        // **arm-gc-by-default **: this is the one point every collect path passes
        // through once its cycle is accounted for, so it is where the next trip point is
        // recomputed. It must stay **lock-free**: three of the four call sites hold
        // `inner.lock()` (they credit the bytes inside the same stats block), and
        // `parking_lot::Mutex` is not reentrant.
        self.rearm_auto_collect();
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
        crate::gc::safepoint::debug_assert_not_native_parked();
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
        // Pressure *events* still need a budget to be a fraction of; auto-collect no longer
        // does (arm-gc-by-default). The gate below is one relaxed load + compare — see
        // `ArcMagrGC::next_collect_at` for why it has to be that cheap.
        if self.max_bytes_atomic.load(Ordering::Relaxed) != u64::MAX {
            self.check_pressure(size as u64);
        }
        if self.used_bytes.load(Ordering::Relaxed) >= self.next_collect_at.load(Ordering::Relaxed) {
            self.maybe_auto_collect();
        }
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
    ) -> Option<crate::object::header::VarGcRef> {
        let (footprint, size_class) = crate::gc::var_region::class_for(payload);
        if size_class == crate::gc::var_region::OVERSIZED_CLASS {
            return None; // oversized → dedicated-chunk locked path
        }
        crate::gc::tlab::with_current_tlab(|tlab| {
            if tlab.heap_epoch != self.epoch {
                if tlab.is_unbound() {
                    tlab.heap_epoch = self.epoch;
                    tlab.holes.forget_dry();
                } else {
                    return None;
                }
            }
            // M12: a dead slot of this size class first (`var_region::holes`) — lock-free out of
            // the thread's stash, refilled in batches under the region lock until the region has
            // none of this class left before the next collection.
            let stash = &mut tlab.holes;
            if stash.has(size_class)
                || (!stash.is_dry(size_class) && self.region_var.lock().take_holes(size_class, stash))
            {
                // SAFETY: owner thread; a slot of this class is ready; the stash's slots were taken
                // from this heap's region (the epoch check above binds the TLAB to it).
                return Some(unsafe { stash.fill(size_class, payload, block_type) });
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
    /// The caller writes the payload into the returned (zeroed) block, then **publishes** it
    /// (`VarGcRef::publish`): a block filled into a reused slot is not alive until then (M12).
    #[inline]
    pub(super) fn acquire_var_block(
        &self,
        payload: usize,
        block_type: BlockType,
    ) -> (crate::object::header::VarGcRef, bool) {
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
        // add-incremental-major-gc M2a: hand this thread's SATB records to its heap (see gc::satb).
        let recorded = crate::gc::satb::take_thread_buffer(self.epoch);
        if !recorded.is_empty() { self.satb_queue.lock().extend(recorded); }
        crate::gc::tlab::with_current_tlab(|tlab| {
            if tlab.heap_epoch != self.epoch {
                // Not bound to this heap (or already unbound) → nothing of ours.
                return;
            }
            let claim = tlab.var.take();
            let holes = Some(&mut tlab.holes).filter(|h| !h.is_empty());
            if claim.is_some() || holes.is_some() {
                let mut region = self.region_var.lock();
                if let Some(mut claim) = claim {
                    region.retire_chunk(&mut claim);
                }
                // M12: unused stashed slots go back on the free list before anything sweeps.
                if let Some(holes) = holes {
                    region.retire_holes(holes);
                }
            }
            tlab.heap_epoch = 0; // unbound
        });
    }

    /// Set the calling thread's TLAB aside in `slot` (native park; see
    /// `MagrGC::park_thread_tlab`): both chunk claims and the M12 hole stash move out of the
    /// thread-local cell, which is left unbound. The SATB buffer is handed over as on retire.
    pub(super) fn park_thread_tlab(&self, slot: &crate::gc::tlab::ParkedTlabSlot) {
        let recorded = crate::gc::satb::take_thread_buffer(self.epoch);
        if !recorded.is_empty() { self.satb_queue.lock().extend(recorded); }
        let parked = crate::gc::tlab::with_current_tlab(|tlab| {
            if tlab.heap_epoch != self.epoch || (tlab.is_unbound() && tlab.holes.is_empty()) { return None; }
            let p = crate::gc::tlab::ParkedTlab {
                var: tlab.var.take(),
                holes: std::mem::take(&mut tlab.holes),
                heap_epoch: tlab.heap_epoch,
            };
            tlab.heap_epoch = 0; // unbound while parked
            Some(p)
        });
        if let Some(p) = parked {
            // A slot still holding a parked TLAB cannot happen (parks do not nest past the
            // outermost); retire defensively rather than lose it.
            if let Some(stale) = slot.put(p) { self.retire_claims(stale) }
        }
    }

    /// Take the set-aside TLAB back (unpark): claims and hole stash, dry marks included — no
    /// collection ran in between (one would have retired the slot), so the free lists did not
    /// grow and the marks still hold. If the TLAB got bound meanwhile, retire instead.
    pub(super) fn unpark_thread_tlab(&self, slot: &crate::gc::tlab::ParkedTlabSlot) {
        let Some(p) = slot.take() else { return };
        if p.heap_epoch != self.epoch { return self.retire_claims(p); }
        let back = crate::gc::tlab::with_current_tlab(|tlab| {
            if !tlab.is_unbound() || !tlab.holes.is_empty() { return Some(p); }
            tlab.var = p.var;
            tlab.holes = p.holes;
            tlab.heap_epoch = p.heap_epoch;
            None
        });
        if let Some(p) = back { self.retire_claims(p) }
    }

    /// Merge a set-aside TLAB into the regions — the retire half of `retire_thread_tlab`:
    /// claims merged, unused stashed slots back on the free list (`retire_holes`), all under
    /// the same region-lock order as there.
    pub(super) fn retire_claims(&self, mut p: crate::gc::tlab::ParkedTlab) {
        let claim = p.var.take();
        if claim.is_some() || !p.holes.is_empty() {
            let mut region = self.region_var.lock();
            if let Some(mut claim) = claim {
                region.retire_chunk(&mut claim);
            }
            if !p.holes.is_empty() {
                region.retire_holes(&mut p.holes);
            }
        }
    }

    /// **Phase 3-OOM**: 检查在当前 used_bytes 基础上再分配 `size` 字节是否会
    /// 越过 max_heap_bytes 上限。仅在 strict_oom 模式下使用。
    pub(super) fn would_oom_after_alloc(&self, size: u64) -> (bool, u64) {
        let (strict, max) = { let i = self.inner.lock(); (i.strict_oom, i.stats.max_bytes) };
        if !strict { return (false, 0); }
        let Some(limit) = max else { return (false, 0); };
        // The cap is judged by the true footprint (`gc::footprint`); `size` is this object's
        // estimate, on top of whatever chunk growth its allocation has already charged.
        let after = self.occupied_bytes().saturating_add(size);
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
    /// **Phase 3d**: collect 完成后，若 used 已降到 near-limit 阈值以下，
    /// reset `near_limit_warned` 让下次跨阈值能再发 NearHeapLimit 事件。
    /// 阈值比率来自 `Z42_GC_NEAR_LIMIT_RATIO`（默认 0.90），与 [`Self::check_pressure`]
    /// 触发 NearHeapLimit 的同一比率——保证「发一次事件」与「重置事件闩」用同一阈值。
    pub(super) fn maybe_reset_near_limit_warned(&self) {
        let near_ratio = crate::config::runtime_config().gc_near_limit_ratio;
        let used = self.occupied_bytes(); // the cap's unit: true footprint
        let mut i = self.inner.lock();
        let Some(limit) = i.stats.max_bytes else { return };
        let near_threshold = (limit as f64 * near_ratio) as u64;
        if used < near_threshold {
            i.near_limit_warned = false;
        }
    }

    pub(super) fn check_pressure(&self, requested: u64) {
        // Judged by the true footprint (`gc::footprint`), the unit the cap is configured in;
        // the events' `used_bytes` carry that reading.
        let used = self.occupied_bytes();
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

    /// unify-gc-heap PR-2: allocate a capturing closure's [`ClosureData`] into the
    /// variable-length GC region (`region_var`) and return a `Value::Closure` handle. The
    /// block is a POD leaf (env `ArrRef` + target `FnId`). Mirrors `alloc_object_block`'s
    /// record + auto-collect tail (no strict-OOM refund path — closure blocks are tiny and the
    /// env array's own alloc already went through the OOM gate). Exposed via the `MagrGC` trait
    /// (`alloc_closure`) so `ctx.heap()` callers reach it through `&dyn MagrGC`.
    pub(super) fn alloc_closure_in_region(&self, data: ClosureData) -> Value {
        // add-gc-tlab (stage 3): lock-free var TLAB when armed; else locked path.
        let (vref, fast) = self.acquire_var_block(std::mem::size_of::<ClosureData>(), BlockType::Closure);
        // SAFETY: fresh block sized exactly for `ClosureData`; write before any typed read.
        unsafe { vref.payload_as_ptr::<ClosureData>().write(data) };
        vref.publish();
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
    pub(super) fn alloc_str_in_region(&self, s: &str) -> crate::object::string::vstr::Str {
        // add-gc-tlab (stage 3): lock-free var TLAB when armed; else locked path.
        let (vref, fast) = self.acquire_var_block(s.len(), BlockType::Str);
        // SAFETY: fresh block sized for exactly `s.len()` bytes; write the UTF-8 bytes
        // into the zeroed payload (derived from the raw block header, D8) before any read.
        unsafe {
            std::ptr::copy_nonoverlapping(s.as_ptr(), vref.payload_as_ptr::<u8>(), s.len());
        }
        // `Length` / `CharAt` / `Substring` then index bytes directly (object::string::str_meta).
        if s.is_ascii() { vref.set_ascii_str(); }
        vref.publish();
        let size = GcBlockHeader::DATA_OFFSET + s.len();
        // `record_alloc`'s `_value` is unused (only the sampler's lazy `kind_fn` matters), so
        // pass `Null` rather than materialize a throwaway `Value::Str`.
        if fast {
            self.record_alloc_fast(|| AllocKind::Object { class: "<string>".to_string() }, size);
        } else {
            self.record_alloc(&Value::Null, || AllocKind::Object { class: "<string>".to_string() }, size);
            self.maybe_auto_collect();
        }
        crate::object::string::vstr::Str::from_var_ref(vref)
    }

    /// fuse-str-concat-alloc: region-alloc a `BlockType::Str` block sized
    /// `a.len() + b.len()` and fill it by copying both segments back-to-back —
    /// the fused counterpart of `alloc_str_in_region(&format!("{a}{b}"))` that
    /// skips the intermediate `String`. Same `record_alloc` / `maybe_auto_collect`
    /// bookkeeping as the single-string path (the concatenation is one block).
    pub(super) fn alloc_str_concat2_in_region(&self, a: &str, b: &str) -> crate::object::string::vstr::Str {
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
        if a.is_ascii() && b.is_ascii() { vref.set_ascii_str(); }
        vref.publish();
        let size = GcBlockHeader::DATA_OFFSET + total;
        if fast {
            self.record_alloc_fast(|| AllocKind::Object { class: "<string>".to_string() }, size);
        } else {
            self.record_alloc(&Value::Null, || AllocKind::Object { class: "<string>".to_string() }, size);
            self.maybe_auto_collect();
        }
        crate::object::string::vstr::Str::from_var_ref(vref)
    }

    /// The array allocation funnel (object model R9 — `MagrGC::alloc_array_with`): one block of
    /// `desc.payload_bytes(len, layout)` bytes through the TLAB / locked paths every block takes
    /// (`acquire_var_block`), the descriptor (and a value-struct array's layout word) installed in
    /// the zeroed block, `init` run on the **unpublished** array, then published and recorded —
    /// the object funnel's shape (`alloc_object_block`). `init` may allocate (a box for an element
    /// that does not fit its cell); the array is unreachable until it returns, and nothing but an
    /// unwired unit-test heap collects inside an allocation. `Value::Null` when a strict-OOM heap
    /// refuses, or the payload is past the block size limit (`u32` bytes).
    pub(super) fn alloc_array_block(
        &self,
        desc: &'static crate::object::ArrayDesc,
        len: usize,
        layout: Option<std::sync::Arc<crate::object::StructTypeLayout>>,
        init: &mut dyn FnMut(&crate::object::ArrayObj),
    ) -> Value {
        let Some(payload) = desc.payload_bytes(len, layout.as_deref()) else {
            let requested = (len as u64).saturating_mul(desc.width() as u64);
            self.fire_event(GcEvent::OutOfMemory { requested_bytes: requested, limit_bytes: u32::MAX as u64 });
            return Value::Null;
        };
        let (vref, fast) = self.acquire_var_block(payload, desc.block_type());
        // SAFETY: a fresh, zeroed block of exactly `payload` bytes nothing else can reach.
        let arr = unsafe { crate::object::ArrayObj::install(vref.header_raw(), payload, desc, layout) };
        init(arr);
        // Built: from here a heap walk may see it (M12 — a reused slot is born not alive).
        vref.publish();
        let value = Value::Array(crate::object::ArrRef::from_var(vref));
        let size = GcBlockHeader::DATA_OFFSET + payload;
        let kind = || AllocKind::Array { elem_count: len };
        if fast {
            self.record_alloc_fast(kind, size);
            return value;
        }
        // Phase 3-OOM: a strict heap refunds an allocation that would cross its limit.
        let (would_oom, limit) = self.would_oom_after_alloc(size as u64);
        if would_oom {
            // No finalizer / side entry can exist yet; tombstoning runs the drop glue (layout).
            self.region_var.lock().tombstone(vref);
            self.fire_event(GcEvent::OutOfMemory { requested_bytes: size as u64, limit_bytes: limit });
            return Value::Null;
        }
        self.record_alloc(&value, kind, size);
        self.maybe_auto_collect();
        value
    }

    pub(super) fn object_size_bytes(&self, value: &Value) -> usize {
        use std::mem::size_of;
        match value {
            Value::Null | Value::Bool(_) | Value::Char(_)
            | Value::I64(_) | Value::F64(_) => size_of::<Value>(),
            Value::Str(s) => size_of::<Value>() + s.len(),
            // Object model R6 / R8 / R9: an object's or array's whole block — header and payload —
            // the figure its allocation charged and its sweep credits (`alloc_charge_bytes`).
            Value::Array(rc) => GcBlockHeader::DATA_OFFSET + rc.var_ref().payload_size(),
            Value::Object(rc) | Value::BoxedStruct(rc) => {
                GcBlockHeader::DATA_OFFSET + rc.var_ref().payload_size()
            }
            // impl-lambda-l2 / P1-2 PR 8: FuncRef holds a `FnId` — no heap allocation.
            Value::FuncRef(_) => size_of::<Value>(),
            // The closure's own block (its env array is a block of its own).
            Value::Closure(c) => GcBlockHeader::DATA_OFFSET + c.payload_size(),
            // make-value-copy: `Ref` / `StructRefHeap` are now
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
            | Value::Ref { .. }
            | Value::PinnedView { .. } | Value::StructRefHeap { .. } => size_of::<Value>(),
        }
    }
}
