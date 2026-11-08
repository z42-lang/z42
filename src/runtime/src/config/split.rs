//! Convert resolved facade settings into subsystem-owned inputs.

use super::RuntimeConfig;
use crate::gc::GcConfig;

impl RuntimeConfig {
    /// Capture construction inputs without exposing facade settings to the VM builder.
    pub(crate) fn vm_config(&self) -> crate::vm::config::VmConfig {
        crate::vm::config::VmConfig {
            safepoint_throttle: self.safepoint_throttle,
            stack_alloc: match self.stackalloc.as_deref() {
                Some("off" | "0" | "heap") => crate::vm::exec::call::stack_alloc::StackAllocPolicy::Off,
                Some("stats") => crate::vm::exec::call::stack_alloc::StackAllocPolicy::Stats,
                _ => crate::vm::exec::call::stack_alloc::StackAllocPolicy::On,
            },
            gc: self.gc_config(),
            gc_max_bytes: self.gc_max_bytes,
            gc_trace: self.gc_trace,
            sample_hz: self.sample_hz,
            trace_timeline: self.trace_out.is_some(),
            process_loh_bytes: self.gc_loh_bytes,
        }
    }

    /// Capture the resolved JIT inputs; the backend never interprets facade settings.
    #[cfg(feature = "jit")]
    pub fn jit_config(&self) -> crate::jit::config::JitConfig {
        crate::jit::config::JitConfig {
            threshold: self.jit_threshold,
            osr_threshold: self.osr_threshold,
            interp_tierup: self.jit_interp_tierup,
            profile: self.jit_profile,
            debug_promote: self.jit_debug_promote,
        }
    }

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
