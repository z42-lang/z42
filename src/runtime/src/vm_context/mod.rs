//! `VmContext` — runtime-mutable state for one VM instance.
//!
//! Single canonical owner of all per-VM mutable state. Replaces the historical
//! `thread_local!` constellation under `interp/` + `jit/` (consolidate-vm-state,
//! 2026-04-28). Fields:
//!
//! - **`static_fields`** — user-class static field storage
//! - **`pending_exception`** — JIT extern-C exception ABI bridge slot
//! - **`lazy_loader`** — on-demand zpkg loader registry
//! - **`exec_stack`** — interp/JIT frame.regs raw pointers (Phase 3f / 3f-2 GC roots)
//! - **`heap`** — `Box<dyn MagrGC>` GC subsystem (default `ArcMagrGC`)
//! - **`native_types`** / **`native_libs`** — Tier 1 native interop registry (spec C2)
//! - **`pinned_owned_buffers`** — owned byte buffers backing `Value::PinnedView` (spec C4)
//!
//! The only remaining `thread_local!` in the runtime is `jit/frame.rs::FRAME_POOL`
//! (pure allocator cache, not state) and `native/exports.rs::CURRENT_VM` (FFI
//! callback bridge, scoped via `VmGuard` RAII).
//!
//! # Lifecycle
//!
//! ```ignore
//! let mut ctx = VmContext::new();
//! ctx.install_lazy_loader_with_deps(libs_dir, main_pool_len, declared, loaded);
//! Vm::new(module, mode).run(&mut ctx, hint)?;
//! ```
//!
//! # Threading
//!
//! `VmContext` is **not** `Send` / `Sync` (intentionally — `Rc<RefCell<...>>`
//! interiors throughout). One ctx serves one OS thread at a time; multi-threaded
//! VM is a roadmap follow-up.
//!
//! # JIT integration
//!
//! `JitModuleCtx::vm_ctx: *mut VmContext` carries the ctx pointer through the
//! `extern "C"` boundary. The pointer is set by `JitModule::run` for the
//! duration of one entry-point invocation and cleared on return. JIT helpers
//! access fields through `(*jit_ctx).vm_ctx` and call ctx methods.
//!
//! See `docs/design/runtime/vm-architecture.md` "VmContext —— 运行时状态归口" 段 for
//! the full state-collapse rationale.


use std::collections::HashMap;
use rustc_hash::FxHashMap;
use std::marker::PhantomPinned;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use parking_lot::{Mutex, RwLock};

use crate::gc::{MagrGC, ArcMagrGC};

mod resource_registry;
mod types;
mod construct;
mod resources;
mod native;
mod frames;
mod statics;
mod lookup;
mod isa_cache;

pub use types::{VmCore, VmContext, VM_CONTEXT_SAFEPOINT_SKIP_OFFSET};
pub(crate) use resource_registry::ResourceRegistry;

/// Type-erased pointer to a registered [`VmContext`].
///
/// Stored in [`VmCore::vm_contexts`] so the GC scanner can walk every thread's
/// per-thread state (call stack / pending exception / func-ref slots) during
/// mark phase. Without this registry, the scanner could only see the first
/// VmContext it was given a clone of — multi-threading would silently miss
/// roots and free live cross-thread objects.
///
/// # Safety
///
/// - The pointer is registered by [`VmContext::new`] AFTER `Pin<Box<...>>`
///   wrapping guarantees address stability for the entire lifetime of the
///   VmContext (the Box's heap allocation address is stable, and Pin prevents
///   `mem::swap` / move-out).
/// - It is deregistered by [`VmContext::drop`] BEFORE the Box is freed
///   (`retain` runs in Drop, prior to memory deallocation), so any
///   dereference performed while the entry is in `vm_contexts` is on a live
///   VmContext.
/// - Cross-thread access: every per-thread field on VmContext is itself
///   `Arc<Mutex<...>>` (Send + Sync), so reading them from another thread
///   under registry-lock-then-deref discipline is sound.
pub(crate) struct VmContextPtr(pub(crate) *const VmContext);

// SAFETY: see SAFETY block on `VmContextPtr` above — the raw pointer is
// kept alive by the Box/Pin ownership of the registering thread, registry
// updates happen under `vm_contexts: Mutex<...>`, and dereferenced fields
// are themselves Send + Sync.
unsafe impl Send for VmContextPtr {}
unsafe impl Sync for VmContextPtr {}

/// Process-wide registry of all live [`VmCore`] instances.
///
/// **add-os-signal-handler (2026-05-25)**: the POSIX signal handler in
/// [`crate::signal_handler`] (Phase 2 of D4 panic-hook story) walks this
/// list to capture z42 call stacks across all VMs in the process. Stored
/// as `Weak` so a VmCore drop doesn't require explicit deregistration;
/// the handler ignores entries whose `upgrade()` returns `None` and the
/// next `VmCore::new` call lazy-prunes dead entries via `retain`.
///
/// Use the public [`vm_cores_snapshot`] to grab a Vec of live cores from
/// non-signal contexts (tests, debug tools); the signal handler uses
/// `try_lock` directly to avoid deadlock on contended mutator threads.
pub(crate) static VM_CORES: std::sync::Mutex<Vec<Weak<VmCore>>> =
    std::sync::Mutex::new(Vec::new());

/// Snapshot all live `Arc<VmCore>` instances. Convenience for tests +
/// diagnostic tools. The signal handler does NOT use this (uses
/// `VM_CORES.try_lock()` directly to avoid blocking on contended cores).
pub fn vm_cores_snapshot() -> Vec<Arc<VmCore>> {
    VM_CORES
        .lock()
        .ok()
        .map(|g| g.iter().filter_map(|w| w.upgrade()).collect())
        .unwrap_or_default()
}

use crate::metadata::lazy_loader::{LazyLoader, ZpkgCandidate};
use crate::metadata::{Function, TypeDesc, Value};

/// **add-lazy-context-unload (2026-08-05)**: routes GC-driven collectible-context
/// reclamation to `VmCore.context_registry`. Captures `Weak<VmCore>` (cycle
/// avoidance, like the root scanner) + a clone of the unloading-count flag for a
/// lock-free `is_unloading` gate. Registered via `heap.set_context_reclaimer`.
struct CoreContextReclaimer {
    core: Weak<VmCore>,
    unloading: Arc<std::sync::atomic::AtomicUsize>,
}

impl crate::gc::arc_heap::ContextReclaim for CoreContextReclaimer {
    fn is_unloading(&self) -> bool {
        self.unloading.load(std::sync::atomic::Ordering::Relaxed) > 0
    }
    fn snapshot(&self) -> crate::metadata::context::ContextLiveness {
        self.core
            .upgrade()
            .map(|c| c.context_registry.lock().liveness_snapshot())
            .unwrap_or_default()
    }
    fn reclaim(&self, live: &std::collections::HashSet<crate::metadata::context::ContextId>) {
        if let Some(c) = self.core.upgrade() {
            c.context_registry.lock().reclaim(live);
        }
    }
}

#[cfg(test)]
#[path = "../vm_context_tests.rs"]
mod vm_context_tests;
#[cfg(test)]
mod isa_cache_tests;
