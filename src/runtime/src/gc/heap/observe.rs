//! `Heap` 观测：barrier observer(test) + 事件分发 + pause 计时 + stats。
//! 从 `heap.rs` 拆出（refactor-arc-heap-modularization）。

#[cfg(test)]
use super::{BarrierEvent, BarrierObserver};
use std::sync::Arc;
use crate::object::Value;
#[cfg(test)]
use crate::object::ArrRef;
use crate::gc::types::{GcEvent, HeapStats};

impl crate::gc::Heap {
    /// Pause-timing clock. fix-wasm-embed-gc-time: GC pause timing runs on every
    /// collection, and wasm32 has no clock — `pal::time` counts ticks there instead.
    pub(super) fn now_us() -> u64 {
        crate::pal::time::monotonic_us()
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

    /// **add-write-barriers **: install a test-only observer
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

    /// **add-write-barriers **: uninstall the test observer.
    #[cfg(test)]
    pub fn clear_barrier_observer(&self) -> Option<std::sync::Arc<BarrierObserver>> {
        self.barrier_observer.lock().take()
    }

    #[cfg(test)]
    pub(super) fn fire_barrier_field(&self, owner: &Value, slot: usize, new: &Value) {
        if let Some(obs) = self.barrier_observer.lock().as_ref() {
            let owner_addr = match owner {
                Value::Object(rc) => crate::object::ObjRef::as_ptr(rc) as *const () as usize,
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
                Value::Array(rc) => ArrRef::as_ptr(rc) as *const () as usize,
                _ => 0,
            };
            obs.push(BarrierEvent::ArrayElem {
                arr_addr,
                idx,
                new_is_heap: new.is_heap_ref(),
            });
        }
    }


    /// Alive objects and arrays that still carry a finalizer (side table) — the same set
    /// `snapshot_live_from_registry` would hand out, counted in place. A block an incremental
    /// sweep has already judged dead is not counted (as `admit_resurrected` would refuse it).
    fn count_alive_with_finalizer(&self) -> u64 {
        self.retire_thread_tlab();
        self.count_object_finalizers()
    }

    pub fn stats(&self) -> HeapStats {
        // finalizers_pending：侧表里的 finalizer 只有一部分属于还活着的块，准确值要扫一遍 region ——
        // 但只数数，不把活对象收进 Vec（那样观测本身就要多付 16–32 B/对象的峰值内存）。
        let pending = self.count_alive_with_finalizer();

        let mut s = self.inner.lock().stats.clone();
        // add-gc-tlab (option B): live counters live on the atomics now, not inner.stats.
        s.used_bytes = self.used_bytes_atomic();
        s.committed_bytes = self.committed_bytes();
        s.allocations = self.allocations.load(std::sync::atomic::Ordering::Relaxed);
        s.finalizers_pending = pending;
        s.pause_histogram = self.pause_histogram.lock().clone();
        s
    }
}
