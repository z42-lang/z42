//! Convert resolved facade settings into subsystem-owned inputs.

use super::RuntimeConfig;
use crate::gc::GcConfig;

impl RuntimeConfig {
    /// Capture this runtime's heap policy without exposing the facade to GC.
    pub fn gc_config(&self) -> GcConfig {
        GcConfig {
            mode: self.gc_mode,
            minor_threshold: self.gc_minor_threshold,
            pause_window: self.gc_pause_window,
            soft_threshold: self.gc_soft_threshold,
            nursery_bytes: self.gc_nursery_bytes,
            promotion_age: self.gc_promotion_age,
            adaptive_promotion: self.gc_adaptive_promotion,
            incremental: self.gc_incremental,
            pause_target_ms: self.gc_pause_target_ms,
            slice_ms: self.gc_slice_ms,
            phases: self.gc_phases,
            verify_cards: self.gc_verify_cards,
            near_limit_ratio: self.gc_near_limit_ratio,
            pressure_ratio: self.gc_pressure_ratio,
        }
    }
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod split_tests;
