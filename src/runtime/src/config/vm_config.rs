//! Resolved construction inputs; parsing and default selection belong to the facade.

use crate::gc::GcConfig;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VmConfig {
    pub safepoint_throttle: u32,
    pub stack_alloc: crate::exec::stack_alloc::StackAllocPolicy,
    pub gc: GcConfig,
    pub gc_max_bytes: Option<u64>,
    pub gc_trace: bool,
    pub sample_hz: Option<u32>,
    pub trace_timeline: bool,
    /// Worker-thread stack size, captured at construction (runtime-audit P0-4:
    /// z42 recursion on the native stack is fatal, so spawned workers take this
    /// instead of std's 2 MiB default).
    pub vm_thread_stack_size: usize,
}
