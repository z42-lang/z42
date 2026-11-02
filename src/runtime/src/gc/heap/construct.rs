//! Construct a heap from immutable policy inputs supplied by its caller.

use super::*;

impl Default for Heap {
    fn default() -> Self { Self::new(crate::gc::GcConfig::default()) }
}

impl Heap {
    pub(super) fn construct(config: crate::gc::GcConfig) -> Self {
        let mode = config.mode;
        let generational = mode == crate::gc::GcMode::GenerationalMarkSweep;
        let promotion_age = config.promotion_age();
        let footprint = std::sync::Arc::new(crate::gc::footprint::Footprint::default());
        let mut region_var = VarRegion::with_drop_glue_for_mode(var_payload_drop_glue(), generational, promotion_age);
        region_var.attach_footprint(footprint.clone());
        Self {
            config,
            inner: Mutex::new(RcHeapInner::default()),
            external_root_scanner: Mutex::new(None),
            context_reclaimer: Mutex::new(None),
            categorized_root_scanner: Mutex::new(None),
            external_needs_collect: Mutex::new(None),
            safepoint_poke: Mutex::new(None),
            mode: std::sync::atomic::AtomicU8::new(mode as u8),
            // fix-young-list-only-when-generational: the young set is minor GC's private
            // index, so only a generational heap pays to maintain it (`set_mode` keeps it in step).
            region_var:    Mutex::new(region_var),
            remembered:    Mutex::new(Vec::new()),
            promotion_policy: Default::default(),
            mark_queue: Mutex::new(Vec::new()),
            alloc_black: std::sync::atomic::AtomicBool::new(false),
            // 1, not 0: 0 means "never major-marked", and marks placed before the first cycle
            // must not collide with the first cycle's epoch — `begin_major_mark` moves on to 2.
            // See `Heap::major_mark`.
            mark_epoch: std::sync::atomic::AtomicU8::new(1),
            satb_queue: parking_lot::Mutex::new(Vec::new()),
            pause_histogram: Mutex::new(crate::gc::types::PauseHistogram::new(config.pause_window)),
            #[cfg(test)]
            barrier_observer: Mutex::new(None),
            // fix-wasm-string-ops: claim a fresh, never-reused epoch for this heap.
            epoch: NEXT_HEAP_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            // add-gc-tlab (option B): live counters start at 0 (no allocations yet).
            used_bytes: std::sync::atomic::AtomicU64::new(0),
            footprint,
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
                config.nursery_bytes
                    .unwrap_or(super::auto_collect::DEFAULT_NURSERY_BYTES)
                    .max(1),
            ),
            // add-pause-budget-nursery: the allowance unit is the configured nursery and stays put
            // while `nursery_bytes` adapts to the pause budget (see `auto_collect::allowance_unit`).
            allowance_unit_bytes: std::sync::atomic::AtomicU64::new(
                config.nursery_bytes
                    .unwrap_or(super::auto_collect::DEFAULT_NURSERY_BYTES)
                    .max(1),
            ),
            // The entry cap only applies to the default gates: a configured nursery is taken as is.
            entry_unit: entry_unit::EntryUnit::new(config.nursery_bytes.is_none()),
            incremental: Default::default(),
            // M9: without a clock (wasm32) `now_us` counts ticks, so pauses are not comparable as time.
            young_policy: young_policy::YoungPolicy::new(crate::pal::time::HAS_CLOCK),
            // add-pause-budget-nursery: an explicit nursery is a manual override — it turns the
            // adaptation off rather than seeding it, so "I set the nursery and it did not stick"
            // cannot happen. Without a clock (wasm32; `now_us` counts ticks) the cost model would be
            // meaningless.
            pause_budget: pause_budget::PauseBudget::new(
                if !crate::pal::time::HAS_CLOCK || config.nursery_bytes.is_some() {
                    0
                } else {
                    (config.pause_target_ms * 1000.0) as u64
                },
            ),
        }
    }
}