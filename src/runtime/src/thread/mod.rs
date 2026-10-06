//! Threading support — z42 coroutine / OS-thread abstractions.
//!
//! **STATUS: STUB** — no public API yet.
//!
//! Phase 1 reality (post extend-jit-helper-abi, 2026-04-28): the VM is still
//! single-threaded but **all** runtime-mutable state is now owned by an
//! explicit `VmContext` (see `crate::vm_context`). The previous `thread_local!`
//! slots `interp::dispatch::STATIC_FIELDS`, `interp::PENDING_EXCEPTION`,
//! `jit::helpers::STATIC_FIELDS`, `jit::helpers::PENDING_EXCEPTION`, and
//! `lazy_loader::STATE` are all gone. Multi-`VmContext` instances on the
//! same thread now run in genuine isolation (no cross-pollination through
//! shared thread-local store).
//!
//! **Future work** (L3+): introduce coroutine / OS-thread primitives backing
//! the `async` / `await` syntax already accepted by the parser. Implementation
//! awaits the GC (`super::gc`) so cross-thread reachability is sound.
