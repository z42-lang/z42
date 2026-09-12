//! `ArcMagrGC` 的构造：`Default` impl（`new()` 委托到它）。
//!
//! add-gc-runtime-knobs (2026-09-05) 从 `arc_heap.rs` 搬出 —— 那个文件此前停在
//! 499/500 行的硬上限边缘，任何新字段都会把它顶红。构造器是一块自足的东西
//! （30 个字段的初值 + 为什么 `mode` 必须手写而不是 derive），拆出来给主文件留出余量。
//! 私有字段在这里可见：`arc_heap::construct` 是 `arc_heap` 的子模块。

use super::*;

/// **add-concurrent-gc P0 (2026-05-22)**: manual `Default` impl so the
/// `mode` field is initialized from `GcMode::from_env()` (reads
/// `Z42_GC_MODE`). Other fields fall back to their own `Default`.
impl Default for ArcMagrGC {
    fn default() -> Self {
        let mode = crate::gc::GcMode::from_env();
        let generational = mode == crate::gc::GcMode::GenerationalMarkSweep;
        // add-promotion-age-knob (2026-09-08): read `Z42_GC_PROMOTION_AGE` **once, here**, and
        // hand the value to every part that needs it. The write barrier consults it on every
        // heap reference write, so a `runtime_config()` lookup there would be a global read on
        // the hottest path in the VM — the reason this knob was previously "deliberately not
        // done". A construction-time read costs nothing and keeps the value immutable for the
        // heap's lifetime, which is also what makes the three copies below safe to cache.
        let promotion_age = crate::gc::promotion_age_from_config();
        Self {
            inner: Mutex::new(RcHeapInner::default()),
            external_root_scanner: Mutex::new(None),
            context_reclaimer: Mutex::new(None),
            categorized_root_scanner: Mutex::new(None),
            external_needs_collect: Mutex::new(None),
            mode: std::sync::atomic::AtomicU8::new(mode as u8),
            // fix-young-list-only-when-generational: the young list is minor GC's
            // private index, so only a generational heap pays to maintain it.
            // `set_mode` keeps this in step if the mode changes later.
            region_object: Mutex::new(crate::gc::region::Region::new_for_mode(generational, promotion_age)),
            region_array:  Mutex::new(crate::gc::region::Region::new_for_mode(generational, promotion_age)),
            region_var:    Mutex::new(VarRegion::with_drop_glue_for_mode(var_drop_glue, generational, promotion_age)),
            promotion_policy: Default::default(),
            mark_queue: Mutex::new(Vec::new()),
            alloc_black: std::sync::atomic::AtomicBool::new(false),
            pause_histogram: Mutex::new(crate::gc::types::PauseHistogram::default()),
            #[cfg(test)]
            barrier_observer: Mutex::new(None),
            #[cfg(debug_assertions)]
            debug_stw_no_push: std::sync::atomic::AtomicBool::new(false),
            // fix-wasm-string-ops: claim a fresh, never-reused epoch for this heap.
            epoch: NEXT_HEAP_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            // add-gc-tlab (option B): live counters start at 0 (no allocations yet).
            used_bytes: std::sync::atomic::AtomicU64::new(0),
            allocations: std::sync::atomic::AtomicU64::new(0),
            // add-gc-tlab (stage 2): mirrors of inner config, defaults match
            // `RcHeapInner::default` (strict_oom=false, no limit, no sampler).
            strict_oom_atomic: std::sync::atomic::AtomicBool::new(false),
            max_bytes_atomic: std::sync::atomic::AtomicU64::new(u64::MAX),
            sampler_active: std::sync::atomic::AtomicBool::new(false),
            promoted_bytes_since_major: std::sync::atomic::AtomicU64::new(0),
            pending_major: std::sync::atomic::AtomicBool::new(false),
            promotion_age: std::sync::atomic::AtomicU8::new(promotion_age),
            configured_promotion_age: promotion_age,
            // 0 = "consult the policy on the first allocation", which then arms it properly.
            next_collect_at: std::sync::atomic::AtomicU64::new(0),
            nursery_bytes: std::sync::atomic::AtomicU64::new(
                crate::config::runtime_config()
                    .gc_nursery_bytes
                    .unwrap_or(super::auto_collect::DEFAULT_NURSERY_BYTES)
                    .max(1),
            ),
        }
    }
}