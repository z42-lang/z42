//! Resolved construction inputs; parsing and default selection belong to the facade.

use crate::gc::GcConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VmConfig {
    pub safepoint_throttle: u32,
    pub stack_alloc: crate::vm::exec::call::stack_alloc::StackAllocPolicy,
    pub gc: GcConfig,
    pub gc_max_bytes: Option<u64>,
    pub gc_trace: bool,
    pub sample_hz: Option<u32>,
    pub trace_timeline: bool,
    /// Construction still applies this override to the process-wide LOH policy.
    pub process_loh_bytes: Option<u64>,
}
