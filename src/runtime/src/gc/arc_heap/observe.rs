//! `ArcMagrGC` 观测：barrier observer(test) + 事件分发 + pause 计时 + snapshot/stats。
//! 从 `arc_heap.rs` 拆出（refactor-arc-heap-modularization）。

#[cfg(test)]
use super::{BarrierEvent, BarrierObserver};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use crate::metadata::{Value};
use crate::gc::refs::{GcRef};
use crate::gc::types::{GcEvent, HeapSnapshot, HeapStats, SnapshotCoverage};

impl crate::gc::arc_heap::ArcMagrGC {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn now_us() -> u64 {
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
    pub(super) fn now_us() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static TICK: AtomicU64 = AtomicU64::new(0);
        TICK.fetch_add(1, Ordering::Relaxed)
    }

    /// 取 Value 的"类型名"（用于 snapshot 聚合）。
    pub(super) fn type_name_of(value: &Value) -> Option<String> {
        match value {
            Value::Object(rc) => Some(rc.type_desc().name.clone()),
            Value::BoxedStruct(rc) => Some(rc.type_desc().name.clone()),  // add-boxed-struct-identity (P4b)
            Value::Array(_)   => Some("<Array>".to_string()),
            _ => None,
        }
    }

    /// 把事件分发到所有 observer。先 snapshot observer 列表再分发，避免
    /// observer 在回调中重入 add_observer/remove_observer 引发 borrow 冲突。
    pub(super) fn fire_event(&self, event: GcEvent) {
        let observers: Vec<_> = self.inner.lock().observers.iter()
            .map(|(_, o)| Arc::clone(o)).collect();
        for o in observers {
            o.on_event(&event);
        }
    }

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

    #[cfg(test)]
    pub(super) fn fire_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
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
    pub(super) fn fire_barrier_array_elem(&self, arr: &Value, idx: usize, new: &Value) {
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

    pub(super) fn take_snapshot(&self) -> HeapSnapshot {
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

    pub(super) fn stats(&self) -> HeapStats {
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
        // add-gc-tlab (option B): live counters live on the atomics now, not inner.stats.
        s.used_bytes = self.used_bytes_atomic();
        s.allocations = self.allocations.load(std::sync::atomic::Ordering::Relaxed);
        s.finalizers_pending = pending;
        s.pause_histogram = self.pause_histogram.lock().clone();
        s
    }
}
