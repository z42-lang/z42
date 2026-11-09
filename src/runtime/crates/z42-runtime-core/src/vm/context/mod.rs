//! `VmContext` — the per-thread half of one VM instance's mutable state.
//!
//! A VM is one shared [`VmCore`] (behind `Arc`: static fields, lazy loader, GC
//! heap, thread coordination, extension state, observers, …) plus one
//! `VmContext` per thread that runs z42 code. The context holds what belongs to
//! that thread:
//!
//! - **exec**: frames, register pool, activation guards, arenas and exception slots;
//! - **runtime**: per-thread lookup and ISA caches;
//! - **diagnostics**: the thread's counter shard;
//! - **threads**: parked TLAB and safepoint throttle.
//!
//! # Lifecycle
//!
//! ```ignore
//! let ctx = VmContext::with_module(module);
//! ctx.install_lazy_loader_with_deps(libs_dir, main_pool_len, declared, loaded);
//! Vm::new(mode).run(&ctx, hint)?;
//! ```
//!
//! # Threading
//!
//! `VmContext` is `Send + Sync` (its fields are `Arc` / `Mutex` / atomics, plus
//! the `unsafe impl Sync` frame stack), but each context is driven by the one
//! thread that runs z42 code through it. A VM-created thread gets its own
//! context via [`VmContext::new_with_core`], sharing the same `VmCore`. Other
//! threads reach a context only through `VmCore.threads.vm_contexts` (GC root
//! scanning, under the registry lock — and the frame stack only while its
//! owner is parked).
//!
//! # JIT integration
//!
//! Every compiled function and JIT helper takes the running thread's own
//! context as its `extern "C"` context parameter (`*const VmContext`), so JIT
//! code on any thread reaches that thread's state directly; the VM's shared JIT
//! is owned by a registered TierApi on `VmCore::exec`; the neutral per-thread header belongs to exec, with no concrete backend selection.
//!
//! See `docs/internals/src/runtime/vm-architecture.md` "VmContext —— 运行时状态归口" 段 for
//! the full state-collapse rationale.


use std::collections::HashMap;
use rustc_hash::FxHashMap;
use std::marker::PhantomPinned;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use parking_lot::{Mutex, RwLock};

use crate::gc::{Heap};

mod types;
mod construct;
mod native;
mod frames;
mod statics;
mod lookup;
// fix-silent-symbol-resolution：「确定不存在」的判定（两后端共用）。
pub mod symres;

pub use types::{
    VmCore, VmContext, VM_CONTEXT_CCTORS_OFFSET, VM_CONTEXT_SAFEPOINT_SKIP_OFFSET,
    VM_CONTEXT_STACK_LIMIT_OFFSET,
};

/// Type-erased pointer to a registered [`VmContext`].
///
/// Stored in `VmCore.threads.vm_contexts` so the GC scanner can walk every thread's
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
/// - Cross-thread access: the per-thread fields on VmContext are
///   `Arc<Mutex<...>>` / atomics (Send + Sync), so reading them from another
///   thread under registry-lock-then-deref discipline is sound. The one
///   exception is `call_stack`, readable cross-thread only while its owner is
///   parked (`VmContext::scan_frames_parked`).
pub struct VmContextPtr(pub(crate) *const VmContext);

// SAFETY: see SAFETY block on `VmContextPtr` above — the raw pointer is
// kept alive by the Box/Pin ownership of the registering thread, registry
// updates happen under `vm_contexts: Mutex<...>`, and dereferenced fields
// are themselves Send + Sync.
unsafe impl Send for VmContextPtr {}
unsafe impl Sync for VmContextPtr {}

/// Process-wide registry of all live [`VmCore`] instances.
///
/// **add-os-signal-handler **: the POSIX signal handler in
/// [`crate::host::signal`] (Phase 2 of D4 panic-hook story) walks this
/// list to capture z42 call stacks across all VMs in the process. Stored
/// as `Weak` so a VmCore drop doesn't require explicit deregistration;
/// the handler ignores entries whose `upgrade()` returns `None` and the
/// next `VmCore::new` call lazy-prunes dead entries via `retain`.
///
/// Use the public [`vm_cores_snapshot`] to grab a Vec of live cores from
/// non-signal contexts (tests, debug tools); the signal handler uses
/// `try_lock` directly to avoid deadlock on contended mutator threads.
pub static VM_CORES: std::sync::Mutex<Vec<Weak<VmCore>>> =
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

use crate::vm::loader::lazy::{LazyLoader, ZpkgCandidate};
use crate::vm::runtime::Function;
use crate::object::{TypeDesc, Value};

#[cfg(all(test, z42_test_facade))]
#[path = "vm_context_tests.rs"]
mod vm_context_tests;
#[cfg(all(test, z42_test_facade))]
mod symres_tests;

mod bootstrap;

#[cfg(all(test, z42_test_facade))]
use crate::assembly::context::VmContextFactory as _;

pub use bootstrap::RuntimeInputs;
