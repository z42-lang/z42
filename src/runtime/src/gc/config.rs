//! Immutable heap policy inputs. Parsing and process settings belong to the facade.

use super::{GcMode, PROMOTION_THRESHOLD};

/// Policy values captured when a heap is constructed. Mutable mode and heap limits
/// retain their existing heap APIs; this is the initial policy, not live settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GcConfig {
    pub mode: GcMode,
    pub minor_threshold: f32,
    pub pause_window: usize,
    pub soft_threshold: f64,
    pub nursery_bytes: Option<u64>,
    pub promotion_age: Option<u8>,
    pub adaptive_promotion: bool,
    pub incremental: bool,
    pub pause_target_ms: f64,
    pub slice_ms: f64,
    pub phases: bool,
    pub verify_cards: bool,
    pub near_limit_ratio: f64,
    pub pressure_ratio: f64,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            mode: GcMode::default(),
            minor_threshold: 0.75,
            pause_window: super::types::PAUSE_WINDOW_DEFAULT_CAP,
            soft_threshold: 0.80,
            nursery_bytes: None,
            promotion_age: None,
            adaptive_promotion: true,
            incremental: true,
            pause_target_ms: 10.0,
            slice_ms: 2.0,
            phases: false,
            verify_cards: false,
            near_limit_ratio: 0.90,
            pressure_ratio: 0.75,
        }
    }
}

impl GcConfig {
    /// Clamp to the age encoded by the object header before caching it in the heap
    /// and region. Every cached copy must use the same line.
    pub fn promotion_age(&self) -> u8 {
        let want = self.promotion_age.unwrap_or(PROMOTION_THRESHOLD);
        let max = crate::object::header::MAX_GEN_AGE;
        if want < 1 || want > max {
            let clamped = want.clamp(1, max);
            eprintln!("z42: Z42_GC_PROMOTION_AGE={want} out of range (1..={max}); using {clamped}");
            return clamped;
        }
        want
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "config_tests.rs"]
mod config_tests;
