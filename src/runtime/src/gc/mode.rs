//! GC mode selection (add-concurrent-gc P0, 2026-05-22).
//!
//! [`GcMode`] selects which collection algorithm `ArcMagrGC` uses. The
//! default is `StwMarkSweep` — the proven path that landed in
//! `add-mark-sweep-collector`. `ConcurrentMarkSweep` is opt-in via
//! [`ArcMagrGC::set_mode`] or the `Z42_GC_MODE=concurrent` env var.
//!
//! The switch is the conservative shape: production keeps the safe
//! default; experimental concurrent code lives behind an explicit
//! opt-in. Future generational / semispace modes plug into the same
//! enum without further refactoring.

/// Selectable GC algorithm. Encoded as `u8` so `ArcMagrGC` can hold
/// the active mode in an `AtomicU8` field for lock-free mode reads on
/// the write-barrier hot path.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcMode {
    /// Default: stop-the-world mark-sweep. Reachable objects survive;
    /// unreachable objects are freed. Mark + sweep both pause all
    /// mutators. Landed in `add-mark-sweep-collector` (2026-05-21).
    StwMarkSweep = 0,
    /// Concurrent mark + STW sweep. STW root snapshot → background
    /// mark BFS while mutators run → short STW handshake to drain
    /// final-burst → STW sweep. Tricolor incremental update; barrier
    /// shades new heap-ref writes gray. Landing across
    /// `add-concurrent-gc` P0–P7.
    ConcurrentMarkSweep = 1,
    /// Generational mark-sweep. Heap split into young / old
    /// generations via per-entry `gen_age`; minor GC scans only
    /// `young_list` + cross-gen dirty cards (O(young) pause); major
    /// GC scans whole heap. Write barrier records old→young writes
    /// via per-chunk dirty bitmap. Promotion threshold N=2.
    ///
    /// **fix-minor-gc-skips-var-region (2026-09-08)**: all three regions take part —
    /// `region_var` (strings / closures / array element storage, ~45% of RSS) used to sit
    /// out every minor and wait for a major. Var blocks carry their age packed into
    /// `GcBlockHeader::type_tag` and keep their own young list; they need no card table of
    /// their own (they are never the source of a cross-generation write — see
    /// `docs/book/src/runtime/gc-tlab-chunk-exclusive.md`).
    /// Mutually exclusive with `ConcurrentMarkSweep` in v1.
    /// Landing across `add-generational-gc` P0–P4.
    GenerationalMarkSweep = 2,
}

impl Default for GcMode {
    fn default() -> Self { GcMode::StwMarkSweep }
}

impl GcMode {
    /// Resolve the GC mode from the process-wide `RuntimeConfig`.
    /// Unset / invalid `Z42_GC_MODE` → `StwMarkSweep` (default) — the
    /// warning lands once in `crate::config::parse_gc_mode` at first
    /// access, not per-callsite (runtime-config-phase2 2026-06-03).
    pub fn from_env() -> Self {
        crate::config::runtime_config().gc_mode
    }

    /// Convert from the `u8` representation used by `AtomicU8` storage.
    /// Returns `StwMarkSweep` for unknown values — defensive default
    /// so that even corrupt storage can't crash on a `match` exhaustivity
    /// check.
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => GcMode::StwMarkSweep,
            1 => GcMode::ConcurrentMarkSweep,
            2 => GcMode::GenerationalMarkSweep,
            _ => GcMode::StwMarkSweep,
        }
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    #[test]
    fn default_is_stw_mark_sweep() {
        assert_eq!(GcMode::default(), GcMode::StwMarkSweep);
    }

    #[test]
    fn from_u8_roundtrips_known_variants() {
        assert_eq!(GcMode::from_u8(GcMode::StwMarkSweep as u8), GcMode::StwMarkSweep);
        assert_eq!(GcMode::from_u8(GcMode::ConcurrentMarkSweep as u8), GcMode::ConcurrentMarkSweep);
        assert_eq!(GcMode::from_u8(GcMode::GenerationalMarkSweep as u8), GcMode::GenerationalMarkSweep);
    }

    #[test]
    fn from_u8_unknown_falls_back_to_stw() {
        assert_eq!(GcMode::from_u8(99), GcMode::StwMarkSweep);
        assert_eq!(GcMode::from_u8(255), GcMode::StwMarkSweep);
    }

    // Note: from_env() tests cannot reliably set env vars in a unit test
    // (Rust test harness shares process state across parallel tests). The
    // env-var path is exercised by the integration test in P0.9 (running
    // `Z42_GC_MODE=concurrent z42 xtask.zpkg test`) and verified via
    // ArcMagrGC::new() construction in `arc_heap_tests::mode_selection`.
}
