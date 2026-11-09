use super::*;

/// State shared by all contexts of one VM, owned through `Arc<VmCore>`.
/// Subsystems own their synchronization and teardown; contexts retain thread-local
/// frames, pending exceptions and allocation arenas. Concrete core-library and
/// interop resources live in frozen extension state.
pub struct VmCore {
    /// Instances stop before heap/engine teardown; their library leases outlive callbacks.
    pub(crate) components: std::sync::OnceLock<crate::components::VmComponents>,
    pub(crate) threads: crate::vm::threads::ThreadState,
    /// Shared execution state, including engine registrations and backend code lifetime.
    pub(crate) exec: crate::vm::exec::ExecState,
    /// Owned heap. Its root providers keep Weak<VmCore> to avoid ownership cycles.
    /// Metadata and interop providers remain alive until after heap teardown.
    pub(crate) heap:               Box<Heap>,
    pub(crate) runtime: crate::vm::runtime::RuntimeState,
    pub(crate) loader: crate::vm::loader::LoaderState,

    /// Observation state shared by this VM; per-thread counters stay on each context.
    pub(crate) diagnostics: crate::vm::diagnostics::DiagnosticsState,

    /// Released after the heap and engines so extension-owned resources outlive them.
    pub(crate) ext_state: crate::vm::ext::ExtensionState,
}

/// One pinned per-thread view of a VM. Subsystems retain their own synchronization;
/// the aggregate adds no lock. Core remains shared while all other state is private.
pub struct VmContext {
    pub(crate) core: Arc<VmCore>,
    pub(crate) exec: crate::vm::exec::ThreadExecState,
    pub(crate) diagnostics: crate::vm::diagnostics::ThreadDiagnosticsState,
    pub(crate) runtime: crate::vm::runtime::ThreadRuntimeState,
    pub(crate) threads: crate::vm::threads::ThreadLocalState,
    /// Keeps the registered context and nested JIT fields at stable addresses.
    pub(crate) _pin: PhantomPinned,
}

/// Actual nested offset of the per-thread safepoint throttle in the context.
/// Generated code must use this constant instead of assuming Rust field order.
pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET: usize =
    std::mem::offset_of!(VmContext, threads.safepoint_skip);

/// Stack bound read directly by every compiled prologue.
pub const VM_CONTEXT_STACK_LIMIT_OFFSET: usize =
    std::mem::offset_of!(VmContext, exec.jit_head.stack_limit);

/// Registry pointer read by inline string intrinsics' package-init gate.
pub const VM_CONTEXT_CCTORS_OFFSET: usize =
    std::mem::offset_of!(VmContext, exec.jit_head.cctors);

impl VmContext {
    #[inline] pub fn core(&self) -> &Arc<VmCore> { &self.core }
    #[inline] pub fn exec(&self) -> &crate::vm::exec::ThreadExecState { &self.exec }
    #[inline] pub fn diagnostics(&self) -> &crate::vm::diagnostics::ThreadDiagnosticsState { &self.diagnostics }
    #[inline] pub fn runtime(&self) -> &crate::vm::runtime::ThreadRuntimeState { &self.runtime }
    #[inline] pub fn threads(&self) -> &crate::vm::threads::ThreadLocalState { &self.threads }
}
impl VmCore {
    #[inline] pub fn components(&self) -> &std::sync::OnceLock<crate::components::VmComponents> { &self.components }
    #[inline] pub fn exec(&self) -> &crate::vm::exec::ExecState { &self.exec }
    #[inline] pub fn heap(&self) -> &Heap { &self.heap }
    #[inline] pub fn runtime(&self) -> &crate::vm::runtime::RuntimeState { &self.runtime }
    #[inline] pub fn diagnostics(&self) -> &crate::vm::diagnostics::DiagnosticsState { &self.diagnostics }
    #[inline] pub fn loader(&self) -> &crate::vm::loader::LoaderState { &self.loader }
    #[inline] pub fn threads(&self) -> &crate::vm::threads::ThreadState { &self.threads }
    #[inline] pub fn ext_state(&self) -> &crate::vm::ext::ExtensionState { &self.ext_state }
}
