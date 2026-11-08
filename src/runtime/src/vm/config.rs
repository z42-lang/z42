//! Resolved construction inputs; parsing and default selection belong to the facade.

use crate::gc::GcConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VmConfig {
    pub(crate) stack_alloc: crate::vm::exec::call::stack_alloc::StackAllocPolicy,
    pub(crate) gc: GcConfig,
    pub(crate) gc_max_bytes: Option<u64>,
    pub(crate) gc_trace: bool,
    pub(crate) sample_hz: Option<u32>,
    pub(crate) trace_timeline: bool,
    /// Construction still applies this override to the process-wide LOH policy.
    pub(crate) process_loh_bytes: Option<u64>,
}
