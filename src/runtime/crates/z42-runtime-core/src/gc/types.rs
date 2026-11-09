//! Supporting types for the [`Heap`](super::heap::Heap).
//!
//! Houses every data type that the GC interface contracts on (handles, events,
//! observer trait, allocation events, …) so that `heap.rs` can stay
//! focused on heap state and collection.
//!
//! Threading bounds: every callback type (`GcObserver`, `AllocSamplerFn`,
//! `FinalizerFn`) carries `Send + Sync`. The host is expected to wire these
//! into multi-threaded telemetry / metrics pipelines, so the bound is required
//! even though `VmContext` itself is currently single-threaded.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::object::header::arr_ref::WeakArrRef;

// ── Handles ──────────────────────────────────────────────────────────────────

/// Opaque handle returned by [`Heap::pin_root`](super::heap::Heap::pin_root);
/// pass to [`Heap::unpin_root`](super::heap::Heap::unpin_root) to release.
///
/// Modeled after V8's `Persistent<T>::Reset()` and .NET's `GCHandle.Free()` —
/// host has explicit ownership semantics rather than RAII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RootHandle(pub u64);

/// Frame scope marker: the depth of the pin-stack at the moment of
/// [`Heap::enter_frame`](super::heap::Heap::enter_frame). Pass to
/// [`Heap::leave_frame`](super::heap::Heap::leave_frame) to drop every
/// pin added inside that frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMark(pub u32);

/// Observer identity returned by
/// [`Heap::add_observer`](super::heap::Heap::add_observer); used with
/// [`Heap::remove_observer`](super::heap::Heap::remove_observer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(pub u64);

// ── GC events / observer ─────────────────────────────────────────────────────

/// What one collection pause actually did — carried in [`GcEvent`] and [`CollectStats`].
///
/// One kind per pause, named after the work that ran, never after the entry point that asked for
/// it: `GC.Collect()` / `force_collect` under the generational collector usually run a **minor**,
/// and say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcKind {
    /// Generational minor collection: the young generation only.
    Minor,
    /// One bounded slice of an incremental major cycle (`Z42_GC_SLICE_MS`) — including the slice
    /// that opens the cycle and the one that completes it.
    Slice,
    /// A whole-heap collection completed in this pause: STW mark-sweep, a one-shot
    /// generational major, or an open incremental cycle finished synchronously.
    Major,
}

/// Lifecycle / pressure events emitted by the heap to registered observers.
#[derive(Debug, Clone)]
pub enum GcEvent {
    /// Fired immediately before a collection starts.
    BeforeCollect      { kind: GcKind, used_bytes: u64 },
    /// Fired after a collection completes.
    AfterCollect       { kind: GcKind, freed_bytes: u64, pause_us: u64 },
    /// Allocation happening with heap usage at >75% of `max_bytes`.
    AllocationPressure { used_bytes: u64, limit_bytes: u64 },
    /// Heap usage crossed >90% of `max_bytes` (deduped per Heap instance).
    NearHeapLimit      { used_bytes: u64, limit_bytes: u64 },
    /// Allocation would exceed `max_bytes`; Phase 1 still allows the allocation
    /// (RC mode does not enforce). Phase 3 tracing GC may reject.
    OutOfMemory        { requested_bytes: u64, limit_bytes: u64 },
}

/// Embedding-side event subscriber.
///
/// `Send + Sync` so observers can sit in a host metrics / telemetry pipeline
/// that crosses threads (e.g. forward events to an async runtime).
pub trait GcObserver: std::fmt::Debug + Send + Sync {
    fn on_event(&self, event: &GcEvent);
}

// ── Allocation sampling ──────────────────────────────────────────────────────

/// Allocation kind for [`AllocSample`].
#[derive(Debug, Clone)]
pub enum AllocKind {
    Object { class: String },
    Array  { elem_count: usize },
}

/// One allocation sample delivered to an installed
/// [`AllocSamplerFn`].
#[derive(Debug, Clone)]
pub struct AllocSample {
    pub kind:         AllocKind,
    pub size_bytes:   usize,
    /// Microseconds since `Heap::EPOCH` (process start, monotonic).
    pub timestamp_us: u64,
}

/// Allocation sampler callback installed via
/// [`Heap::set_alloc_sampler`](super::heap::Heap::set_alloc_sampler).
pub type AllocSamplerFn = Arc<dyn Fn(&AllocSample) + Send + Sync>;

// ── Finalization ─────────────────────────────────────────────────────────────

/// Finalizer callback registered via
/// [`Heap::register_finalizer`](super::heap::Heap::register_finalizer).
///
/// **Phase 1 RC mode**: registration is recorded but the callback is never
/// invoked (RC has no Drop hook for arbitrary objects). Phase 3 mark-sweep
/// schedules finalizers post-collection.
pub type FinalizerFn = Arc<dyn Fn() + Send + Sync>;

// ── Collection result ────────────────────────────────────────────────────────

/// Result of [`Heap::force_collect`](super::heap::Heap::force_collect).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CollectStats {
    pub freed_bytes: u64,
    pub pause_us:    u64,
    /// `None` when collection was skipped (e.g. heap is paused).
    pub kind:        Option<GcKind>,
}

// ── Weak references ──────────────────────────────────────────────────────────

/// Weak reference to a heap value.
///
/// Constructed via [`Heap::make_weak`](super::heap::Heap::make_weak);
/// upgraded via [`Heap::upgrade_weak`](super::heap::Heap::upgrade_weak).
///
/// Atomic value variants (`I64` / `F64` / `Bool` / `Char` / `Str` / `Null`)
/// cannot be weak-referenced — `make_weak` returns `None` for them.
#[derive(Debug, Clone)]
pub struct WeakRef {
    pub inner: WeakRefInner,
}

#[derive(Debug, Clone)]
pub enum WeakRefInner {
    /// Object model R6 / R9: a weak reference records its chunk's reuse epoch.
    Object(crate::object::WeakObjRef),
    Array (WeakArrRef),
}

// ── GC handle table ──────────────────────────────────────────────────────────

/// Discriminator on slots in [`Heap::handle_alloc`](super::heap::Heap::handle_alloc).
///
/// - **`Strong`**: slot stores a reference that anchors the target across GC
///   collection — equivalent to C# `GCHandleType.Normal`. In Phase 1 RC mode
///   this is a `Rc::clone` of the wrapped value (atomic values are stored by
///   value clone). The slot keeps the target alive until the slot is freed.
/// - **`Weak`**: slot stores a non-anchoring reference; if all strong refs to
///   the target drop, [`Heap::handle_target`](super::heap::Heap::handle_target)
///   on the slot returns `None`. Equivalent to C# `GCHandleType.Weak`.
///
/// Phase 3 tracing GC will add `Pinned` (anchor + forbid relocation) and
/// `WeakTrackResurrection` (weak + finalizer-aware) variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcHandleKind {
    Weak,
    Strong,
}

// ── Pause histogram ──────────────────────────────────────────────────────────

/// Default capacity of a rolling pause-time window. The facade can pass a
/// `Z42_GC_PAUSE_WINDOW` override through `GcConfig`; explicit capacities
/// are bounded by `PauseHistogram::new`.
pub const PAUSE_WINDOW_DEFAULT_CAP: usize = 1024;

/// Hard ceiling on `Z42_GC_PAUSE_WINDOW` override. 65536 × 8 bytes =
/// 512 KB per heap — generous but prevents a hostile env from
/// allocating GB-sized deques. Reflected in
/// the facade pause-window parser.
pub const PAUSE_WINDOW_MAX_CAP: usize = 65536;

/// Half-open bucket edges (µs). `BUCKET_EDGES[i]` is the lower bound of
/// bucket `i+1`; bucket `i` covers `[BUCKET_EDGES[i-1], BUCKET_EDGES[i])`.
/// Bucket 0 is `[0, BUCKET_EDGES[0])`; bucket 7 is `[BUCKET_EDGES[6], ∞)`.
pub const PAUSE_BUCKET_EDGES: [u64; 7] = [
    10,          // < 10 µs
    100,         // [10, 100) µs
    1_000,       // [100µs, 1ms)
    10_000,      // [1, 10) ms
    100_000,     // [10, 100) ms
    1_000_000,   // [100ms, 1s)
    10_000_000,  // [1, 10) s
                 // bucket 7: ≥ 10 s
];

/// Aggregate pause-time distribution recorded across every `collect_cycles`
/// invocation. 8 logarithmic buckets cover µs–10s+ pause ranges; min / max /
/// total / count provide basic summary stats.
///
/// `Default` uses `min_us = u64::MAX` as a sentinel for "no collect recorded
/// yet"; callers should check `count == 0` before reading `min_us`.
///
/// **add-gc-pause-window **: also maintains
/// `recent_pauses: VecDeque<u64>`, a rolling FIFO window of the
/// last `window_cap` pause samples (oldest first). `Copy` is no
/// longer derivable due to the deque — callers `clone()` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseHistogram {
    pub buckets:       [u64; 8],
    pub min_us:        u64,
    pub max_us:        u64,
    pub total_us:      u64,
    pub count:         u64,
    /// Rolling window of the last `window_cap` pause-time samples
    /// in microseconds, oldest first. Capacity is fixed at heap
    /// construction.
    pub recent_pauses: VecDeque<u64>,
    /// Fixed capacity of `recent_pauses`. Stored so callers /
    /// snapshots can self-describe (`Std.GC.PauseWindowCapacity`).
    pub window_cap:    usize,
}

impl Default for PauseHistogram {
    fn default() -> Self { Self::new(PAUSE_WINDOW_DEFAULT_CAP) }
}

impl PauseHistogram {
    /// A bounded rolling window. Zero and oversized explicit inputs are clamped
    /// just like values parsed by the facade.
    pub fn new(cap: usize) -> Self {
        let cap = cap.clamp(1, PAUSE_WINDOW_MAX_CAP);
        Self {
            buckets: [0; 8], min_us: u64::MAX, max_us: 0, total_us: 0,
            count: 0, recent_pauses: VecDeque::with_capacity(cap), window_cap: cap,
        }
    }

    /// Maps a pause measurement (µs) into its bucket index (0..=7).
    ///
    /// Half-open intervals `[lower, upper)`: boundary values land in the
    /// higher bucket (e.g. `bucket_index(10) == 1`, not `0`).
    pub fn bucket_index(pause_us: u64) -> usize {
        for (i, &edge) in PAUSE_BUCKET_EDGES.iter().enumerate() {
            if pause_us < edge { return i; }
        }
        7
    }

    /// Records one pause sample. Saturating arithmetic on every field so a
    /// pathological process running for years cannot overflow into garbage.
    ///
    /// **add-gc-pause-window **: also appends to the rolling
    /// window, evicting the oldest sample if the window is at capacity.
    pub fn record(&mut self, pause_us: u64) {
        let idx = Self::bucket_index(pause_us);
        self.buckets[idx] = self.buckets[idx].saturating_add(1);
        if self.count == 0 || pause_us < self.min_us {
            self.min_us = pause_us;
        }
        if pause_us > self.max_us {
            self.max_us = pause_us;
        }
        self.total_us = self.total_us.saturating_add(pause_us);
        self.count    = self.count.saturating_add(1);

        // Rolling window: FIFO, drop oldest at saturation.
        if self.recent_pauses.len() == self.window_cap {
            self.recent_pauses.pop_front();
        }
        self.recent_pauses.push_back(pause_us);
    }
}

// ── Stats ────────────────────────────────────────────────────────────────────

/// Heap-wide statistics snapshot returned by
/// [`Heap::stats`](super::heap::Heap::stats).
///
/// **add-gc-pause-window **: dropped `Copy` (transitive on
/// `PauseHistogram`, which now carries a `VecDeque`). Callers that
/// previously consumed by value continue to work — `stats()` returns
/// by value via `Clone`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeapStats {
    /// Total allocations since heap creation.
    pub allocations:        u64,
    /// Number of `collect_cycles` / `force_collect` invocations (all-modes
    /// total, unchanged semantics).
    pub gc_cycles:          u64,
    /// Generational **minor** (young-gen) collections — every pause that ran
    /// `run_cycle_collection_minor`, including the minor a generational
    /// `force_collect` / `collect_cycles` runs. `minor + major` need NOT equal
    /// `gc_cycles`: a slice that does not complete its cycle counts as neither,
    /// and a generational `force_collect` that finishes an open cycle counts as both.
    pub minor_collections:  u64,
    /// **Major** (whole-heap) collections *completed*: STW
    /// mark-sweep, a one-shot generational major, an incremental cycle's last
    /// slice or synchronous finish.
    pub major_collections:  u64,
    /// Cumulative bytes reclaimed across all collection cycles (saturating).
    pub reclaimed_bytes:    u64,
    /// Approximate live bytes. **RC mode caveat**: monotonically increases
    /// (`Rc<T>` drop is not observable); Phase 3 tracing GC will be precise.
    pub used_bytes:         u64,
    /// Bytes the heap's regions hold from the allocator: fixed-region chunk storage plus
    /// variable-region chunks. Unlike `used_bytes` (a per-object estimate that the auto-collect
    /// policy runs on) this counts dead slots, size-class rounding and pooled chunks — the
    /// region part of RSS. It does not count per-object payloads allocated outside the regions
    /// (an object's field vectors) or GC side tables.
    pub committed_bytes:    u64,
    /// Heap upper bound (`None` = unlimited).
    pub max_bytes:          Option<u64>,
    /// Currently pinned roots (host-side `pin_root` count, includes frame pins).
    pub roots_pinned:       u64,
    /// Pending finalizer registrations (Phase 1: never invoked).
    pub finalizers_pending: u64,
    /// Active observers.
    pub observers:          u64,
    /// Aggregate pause-time histogram (add-gc-pause-histogram).
    pub pause_histogram:    PauseHistogram,
}

#[cfg(all(test, z42_test_facade))]
#[path = "types_tests.rs"]
mod types_tests;
