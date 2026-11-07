//! Execution state owned by one running context, independent of concrete engines.
use std::sync::{Arc, atomic::{AtomicU32, AtomicUsize}};
use parking_lot::Mutex;
use crate::object::Value;

pub(crate) struct ThreadExecState {
    /// GC-rooted handoff across a JIT error boundary.
    pub(crate) pending_exception: Arc<Mutex<Option<Value>>>,
    /// Callback builtins preserve the original thrown object through this root.
    pub(crate) pending_thrown: Mutex<Option<Value>>,
    /// Owner-thread only; other threads read only after the owner is parked.
    pub(crate) call_stack: super::call::frame_stack::FrameStack,
    pub(crate) reg_pool: super::call::reg_pool::RegPool,
    /// Installed at the bottom push and restored after the last pop.
    pub(crate) engine_guards: super::call::engine_guards::EngineGuards,
    /// LIFO frame-scoped payloads, scanned under each arena lock during a pause.
    pub(crate) stack_arena: Arc<Mutex<super::call::stack_alloc::StackArena>>,
    pub(crate) struct_arena: Arc<Mutex<super::call::struct_arena::StructArena>>,
    pub(crate) transient_arena: Arc<Mutex<super::call::transient_arena::TransientArena>>,
    /// Single-mutator published lengths; GC reads arena data under its locks.
    /// Relaxed reads let push/pop skip locks for unchanged arena lengths.
    pub(crate) stack_obj_len: AtomicUsize,
    pub(crate) stack_arr_len: AtomicUsize,
    pub(crate) struct_len: AtomicUsize,
    pub(crate) transient_len: AtomicUsize,
    /// Owner-thread source; zero is reserved for a frame that has taken no id.
    pub(crate) next_frame_id: AtomicU32,
    pub(crate) jit_head: JitHead,
}

impl ThreadExecState {
    pub(crate) fn new(cctors: &Arc<crate::vm::runtime::statics::CctorRegistry>, engines: &super::engine::EngineRegistry) -> Self {
        Self {
            pending_exception: Arc::new(Mutex::new(None)),
            pending_thrown: Mutex::new(None),
            call_stack: Default::default(),
            reg_pool: Default::default(),
            engine_guards: Default::default(),
            stack_arena: Arc::new(Mutex::new(Default::default())),
            struct_arena: Arc::new(Mutex::new(Default::default())),
            transient_arena: Arc::new(Mutex::new(Default::default())),
            stack_obj_len: AtomicUsize::new(0),
            stack_arr_len: AtomicUsize::new(0),
            struct_len: AtomicUsize::new(0),
            transient_len: AtomicUsize::new(0),
            next_frame_id: AtomicU32::new(1),
            jit_head: JitHead::new(cctors, engines),
        }
    }
}

/// The part of a [`crate::vm::context::VmContext`] that JIT-compiled code reads at fixed offsets.
///
/// Every compiled function and JIT helper receives the **running thread's own**
/// `VmContext` as its context parameter, so these are per-thread values read
/// without a helper call:
///
/// - `stack_limit` — the prologue's stack-overflow bound
///   ([`crate::vm::context::VM_CONTEXT_STACK_LIMIT_OFFSET`]);
/// - `cctors` — the VM's cctor registry, whose package-init gate counters the
///   inline string intrinsics read ([`crate::vm::context::VM_CONTEXT_CCTORS_OFFSET`]).
///
/// It also caches the VM's neutral tier header pointer (`jit`). Backend state
/// remains owned by the registered tier; compiled reads use layout-derived offsets.
///
/// The safepoint throttle counter is the third JIT-read field; it is stored in
/// the context's ThreadLocalState (the interpreter decrements it too) and is
/// addressed by [`crate::vm::context::VM_CONTEXT_SAFEPOINT_SKIP_OFFSET`].
#[repr(C)]
pub(crate) struct JitHead {
    /// Lowest stack address a JIT function may start at: `vm::exec::stack_guard::limit()`
    /// of the thread driving this context, stored at its bottom frame push
    /// (`push_frame` → `EngineGuards::install`), i.e. before any compiled code
    /// runs on it. `0` until then (`sp < 0` never holds: no check). Written and
    /// read by the owner thread only; atomic only to keep the context `Sync`.
    pub(crate) stack_limit: std::sync::atomic::AtomicUsize,
    /// `Arc::as_ptr(cctors)` — stable for the core's life, set at
    /// construction.
    pub(crate) cctors: *const crate::vm::runtime::statics::CctorRegistry,
    /// The registered neutral tier header (null until installed):
    /// taken at construction, published by the tier installer for the
    /// installing context, and refreshed at the bottom frame push for a context
    /// that existed before the install. Points into the core this context keeps
    /// alive. Written and read by the owner thread only.
    #[cfg(feature = "jit")]
    pub(crate) jit: std::sync::atomic::AtomicPtr<crate::vm::exec::engine::TierApi>,
}

// SAFETY: `cctors` points into the `Arc<CctorRegistry>` the context's own core
// keeps alive; the registry is `Sync` and only read through the pointer.
unsafe impl Send for JitHead {}
unsafe impl Sync for JitHead {}

impl JitHead {
    pub(crate) fn new(cctors: &Arc<crate::vm::runtime::statics::CctorRegistry>, engines: &super::engine::EngineRegistry) -> Self {
        #[cfg(not(feature = "jit"))]
        let _ = engines;
        JitHead {
            stack_limit: std::sync::atomic::AtomicUsize::new(0),
            cctors: Arc::as_ptr(cctors),
            #[cfg(feature = "jit")]
            jit: std::sync::atomic::AtomicPtr::new(
                engines.tier().map_or(std::ptr::null_mut(), |rt| rt as *const _ as *mut _)),
        }
    }
}

