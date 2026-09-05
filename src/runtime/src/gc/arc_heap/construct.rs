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
        Self {
            inner: Mutex::new(RcHeapInner::default()),
            external_root_scanner: Mutex::new(None),
            context_reclaimer: Mutex::new(None),
            categorized_root_scanner: Mutex::new(None),
            external_needs_collect: Mutex::new(None),
            mode: std::sync::atomic::AtomicU8::new(crate::gc::GcMode::from_env() as u8),
            region_object: Mutex::new(crate::gc::region::Region::new()),
            region_array:  Mutex::new(crate::gc::region::Region::new()),
            region_var:    Mutex::new(VarRegion::with_drop_glue(var_drop_glue)),
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
        }
    }
}