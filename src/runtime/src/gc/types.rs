//! Supporting types for the [`MagrGC`](super::heap::MagrGC) trait.
//!
//! Houses every data type that the GC interface contracts on (handles, events,
//! observer trait, profiler types, snapshots, …) so that `heap.rs` can stay
//! focused on the trait shape itself.
//!
//! Threading bounds: every callback type (`GcObserver`, `AllocSamplerFn`,
//! `FinalizerFn`) carries `Send + Sync`. The host is expected to wire these
//! into multi-threaded telemetry / metrics pipelines, so the bound is required
//! even though `VmContext` itself is currently single-threaded.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::refs::WeakGcRef;
use crate::metadata::ScriptObject;

// ── Handles ──────────────────────────────────────────────────────────────────

/// Opaque handle returned by [`MagrGC::pin_root`](super::heap::MagrGC::pin_root);
/// pass to [`MagrGC::unpin_root`](super::heap::MagrGC::unpin_root) to release.
///
/// Modeled after V8's `Persistent<T>::Reset()` and .NET's `GCHandle.Free()` —
/// host has explicit ownership semantics rather than RAII.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RootHandle(pub u64);

/// Frame scope marker: the depth of the pin-stack at the moment of
/// [`MagrGC::enter_frame`](super::heap::MagrGC::enter_frame). Pass to
/// [`MagrGC::leave_frame`](super::heap::MagrGC::leave_frame) to drop every
/// pin added inside that frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMark(pub u32);

/// Observer identity returned by
/// [`MagrGC::add_observer`](super::heap::MagrGC::add_observer); used with
/// [`MagrGC::remove_observer`](super::heap::MagrGC::remove_observer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObserverId(pub u64);

// ── GC events / observer ─────────────────────────────────────────────────────

/// GC kind discriminator carried in [`GcEvent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcKind {
    /// Generational minor collection (Phase 4+).
    Minor,
    /// Stop-the-world full collection (Phase 3+).
    Full,
    /// Reference cycle collector (Phase 2+).
    CycleCollector,
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
    /// Heap usage crossed >90% of `max_bytes` (deduped per ArcMagrGC instance).
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
    /// Microseconds since `ArcMagrGC::EPOCH` (process start, monotonic).
    pub timestamp_us: u64,
}

/// Allocation sampler callback installed via
/// [`MagrGC::set_alloc_sampler`](super::heap::MagrGC::set_alloc_sampler).
pub type AllocSamplerFn = Arc<dyn Fn(&AllocSample) + Send + Sync>;

// ── Finalization ─────────────────────────────────────────────────────────────

/// Finalizer callback registered via
/// [`MagrGC::register_finalizer`](super::heap::MagrGC::register_finalizer).
///
/// **Phase 1 RC mode**: registration is recorded but the callback is never
/// invoked (RC has no Drop hook for arbitrary objects). Phase 3 mark-sweep
/// schedules finalizers post-collection.
pub type FinalizerFn = Arc<dyn Fn() + Send + Sync>;

// ── Collection result ────────────────────────────────────────────────────────

/// Result of [`MagrGC::force_collect`](super::heap::MagrGC::force_collect).
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
/// Constructed via [`MagrGC::make_weak`](super::heap::MagrGC::make_weak);
/// upgraded via [`MagrGC::upgrade_weak`](super::heap::MagrGC::upgrade_weak).
///
/// Atomic value variants (`I64` / `F64` / `Bool` / `Char` / `Str` / `Null`)
/// cannot be weak-referenced — `make_weak` returns `None` for them.
#[derive(Debug, Clone)]
pub struct WeakRef {
    pub(crate) inner: WeakRefInner,
}

#[derive(Debug, Clone)]
pub(crate) enum WeakRefInner {
    Object(WeakGcRef<ScriptObject>),
    Array (WeakGcRef<crate::metadata::types::ArrayObj>),
}

// ── GC handle table ──────────────────────────────────────────────────────────

/// Discriminator on slots in [`MagrGC::handle_alloc`](super::heap::MagrGC::handle_alloc).
///
/// - **`Strong`**: slot stores a reference that anchors the target across GC
///   collection — equivalent to C# `GCHandleType.Normal`. In Phase 1 RC mode
///   this is a `Rc::clone` of the wrapped value (atomic values are stored by
///   value clone). The slot keeps the target alive until the slot is freed.
/// - **`Weak`**: slot stores a non-anchoring reference; if all strong refs to
///   the target drop, [`MagrGC::handle_target`](super::heap::MagrGC::handle_target)
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

/// Default capacity of the per-heap rolling pause-time window
/// (add-gc-pause-window, 2026-05-24). Overridable via
/// `Z42_GC_PAUSE_WINDOW`; clamped to `[1, 65536]`. The actual default +
/// clamp live in `RuntimeConfig::gc_pause_window`. Kept as a public
/// constant for any downstream that wants to refer to the default
/// without going through the config singleton.
pub const PAUSE_WINDOW_DEFAULT_CAP: usize = 1024;

/// Hard ceiling on `Z42_GC_PAUSE_WINDOW` override. 65536 × 8 bytes =
/// 512 KB per heap — generous but prevents a hostile env from
/// allocating GB-sized deques. Reflected in
/// `crate::config::parse_gc_pause_window`.
pub const PAUSE_WINDOW_MAX_CAP: usize = 65536;

/// Capacity for the per-heap pause-time window. Delegates to
/// process-wide [`crate::config::runtime_config()`] (parsed once at
/// startup; runtime-config-phase2 2026-06-03).
pub fn pause_window_cap_from_env() -> usize {
    crate::config::runtime_config().gc_pause_window
}

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
/// **add-gc-pause-window (2026-05-24)**: also maintains
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
    /// construction (see `pause_window_cap_from_env`).
    pub recent_pauses: VecDeque<u64>,
    /// Fixed capacity of `recent_pauses`. Stored so callers /
    /// snapshots can self-describe (`Std.GC.PauseWindowCapacity`).
    pub window_cap:    usize,
}

impl Default for PauseHistogram {
    fn default() -> Self {
        let cap = pause_window_cap_from_env();
        Self {
            buckets:       [0; 8],
            min_us:        u64::MAX,
            max_us:        0,
            total_us:      0,
            count:         0,
            recent_pauses: VecDeque::with_capacity(cap),
            window_cap:    cap,
        }
    }
}

impl PauseHistogram {
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
    /// **add-gc-pause-window (2026-05-24)**: also appends to the rolling
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
/// [`MagrGC::stats`](super::heap::MagrGC::stats).
///
/// **add-gc-pause-window (2026-05-24)**: dropped `Copy` (transitive on
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
    /// Generational **minor** (young-gen) collections. Bumped once per
    /// `run_cycle_collection_minor`. `minor + major` need NOT equal
    /// `gc_cycles`: an escalated generational cycle bumps `gc_cycles` once but
    /// both `minor` and `major`; non-generational paths bump only `major`.
    pub minor_collections:  u64,
    /// **Major** (whole-heap) collections: generational escalation +
    /// concurrent mark-sweep + STW full cycle + `force_collect`.
    pub major_collections:  u64,
    /// Cumulative bytes reclaimed across all collection cycles (saturating).
    pub reclaimed_bytes:    u64,
    /// Approximate live bytes. **RC mode caveat**: monotonically increases
    /// (`Rc<T>` drop is not observable); Phase 3 tracing GC will be precise.
    pub used_bytes:         u64,
    /// Heap upper bound (`None` = unlimited).
    pub max_bytes:          Option<u64>,
    /// Currently pinned roots (host-side `pin_root` count, includes frame pins).
    pub roots_pinned:       u64,
    /// Pending finalizer registrations (Phase 1: never invoked).
    pub finalizers_pending: u64,
    /// Active observers.
    pub observers:          u64,
    /// Aggregate pause-time histogram (add-gc-pause-histogram, 2026-05-22).
    pub pause_histogram:    PauseHistogram,
}

// ── Heap snapshot ────────────────────────────────────────────────────────────

/// Coverage discriminator on [`HeapSnapshot`].
///
/// Phase 1 RC mode cannot enumerate all live objects (no global Rc registry);
/// the snapshot covers only objects reachable from currently-pinned roots.
/// Phase 3 tracing GC will produce `Full` snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotCoverage {
    Full,
    ReachableFromPinnedRoots,
}

impl Default for SnapshotCoverage {
    fn default() -> Self { Self::ReachableFromPinnedRoots }
}

/// Per-type aggregate in a [`HeapSnapshot`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectStats {
    pub count: u64,
    pub bytes: u64,
}

/// Heap composition snapshot.
#[derive(Debug, Clone, Default)]
pub struct HeapSnapshot {
    /// Per type-name aggregate.
    pub objects_by_type: HashMap<String, ObjectStats>,
    pub total_objects:   u64,
    pub total_bytes:     u64,
    /// Microseconds since `ArcMagrGC::EPOCH`.
    pub timestamp_us:    u64,
    pub coverage:        SnapshotCoverage,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
