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
use std::marker::PhantomPinned;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use parking_lot::{Mutex, RwLock};

use crate::gc::{MagrGC, ArcMagrGC};

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

/// **`VmCore`** —— state shared across all threads sharing one VM instance
/// (add-multithreading-foundation, 2026-05-19, Phase 1 / spec
/// `2026-05-19-add-multithreading-foundation`).
///
/// Holds fields that are **process-globally singular**: static fields, type
/// registries, native interop registry, pinned buffer table, etc. Per-thread
/// state (call stack, pending exception, frame guards, func-ref cache slots)
/// stays directly on [`VmContext`], which references this struct through
/// `Arc<VmCore>`.
///
/// Phase 1 (this commit): only `static_fields` + `static_field_index` are
/// here. Subsequent phases move `lazy_loader` / `native_types` /
/// `native_libs` / `pinned_owned_buffers` / `processes` / `gc heap` in.
///
/// `VmCore` will become `Send + Sync` once `GcRef<T>` backing switches to
/// `Arc<...>` (Phase 3 of the spec). Until then, `Mutex` is wrapped around
/// fields that hold `Value` (which transitively contains `Rc<RefCell>` via
/// the current `GcRef` backing), so the API surface is already stable —
/// Send-safety completes at Phase 3 without further VmCore API changes.
pub struct VmCore {
    /// Static field storage indexed by `StaticFieldId.0` (introduce-method-token,
    /// 2026-05-08). Slot 0 reserved? No; ids start at 0 (allocation order).
    pub(crate) static_fields:      Mutex<Vec<Value>>,
    /// FQN → slot id map. Lazy-allocated on first access; cross-zpkg lazy
    /// fields can be encountered in any order.
    pub(crate) static_field_index: Mutex<HashMap<String, u32>>,
    /// On-demand zpkg loader. `None` until `install_lazy_loader[_with_deps]`
    /// is called (typically from `bootstrap.rs`). Shared across threads
    /// since zpkg resolution + module loading is a process-global operation.
    pub(crate) lazy_loader:        Mutex<Option<LazyLoader>>,
    /// **add-load-context-model (2026-07-30)**: registry of load contexts +
    /// assemblies (dotnet ALC 地基). Root context (id 0) + root assembly (id 0)
    /// pre-populated. `collectible` contexts created via `AssemblyLoadContext.CreateCollectible`;
    /// `AssemblyLoadContext.Load` registers assemblies with private `Module` arenas.
    /// Phase 1: boundary + reflection identity only (no unload / execution).
    pub(crate) context_registry:   Mutex<crate::metadata::context::ContextRegistry>,
    /// Native interop Tier 1 — registered native types keyed by `(module, type)`.
    /// **`RwLock`** (Decision 6): read-mostly path — `CallNative` dispatch /
    /// `z42_resolve_type` are pure reads, writes only happen during module
    /// load / `z42_register_type`. Concurrent reads from multiple threads
    /// not serialized.
    #[cfg(feature = "native-interop")]
    pub(crate) native_types:       RwLock<HashMap<(String, String), Arc<crate::native::RegisteredType>>>,
    /// Loaded native libraries kept alive for VM lifetime so function
    /// pointers stored in `native_types` stay valid. Lock contention low
    /// (only `dlopen` adds entries) → plain Mutex.
    #[cfg(feature = "native-interop")]
    pub(crate) native_libs:        Mutex<Vec<libloading::Library>>,
    /// Spec C10 — owned byte buffers backing `Value::PinnedView` instances.
    /// Keyed by buffer data pointer so `UnpinPtr` can drop the entry.
    pub(crate) pinned_owned_buffers: Mutex<HashMap<u64, Box<[u8]>>>,
    /// add-method-invoke-non-generic — carries a z42 exception VALUE out of a
    /// callback builtin (reflection `MethodInfo.Invoke`) so the ORIGINAL thrown
    /// exception (preserving its type) propagates to z42 `try/catch`, instead of
    /// being wrapped into a generic `Std.Exception`. Set right before the builtin
    /// returns `Err`; `exec_call::builtin` takes it in its error handler.
    pub(crate) pending_thrown: Mutex<Option<Value>>,
    /// add-std-process (2026-05-13) — live `Std.IO.Process` children
    /// spawned via `__process_spawn`. Keyed by monotonic u64 slot id
    /// that z42 `ProcessHandle` carries; removed (`take_*`) on `wait` /
    /// `kill`+reap / explicit `drop`.
    pub(crate) processes:          Mutex<HashMap<u64, crate::corelib::process::ProcessSlot>>,
    /// **GC subsystem**. Moved here in Phase 2.2 so it can be shared across
    /// threads (single global heap). Backing today is `ArcMagrGC`; Phase 3
    /// swaps to Arc + Send + Sync. Stored as `Box<dyn MagrGC>` (no inner
    /// lock) because all `MagrGC` methods take `&self` and the impl handles
    /// its own interior mutability.
    ///
    /// **Scanner cycle avoidance**: the external root scanner closure
    /// captures `Weak<VmCore>` (not `Arc<VmCore>`) for static_fields access
    /// — otherwise `VmCore` → heap → scanner → Arc<VmCore> forms a cycle
    /// and the core never drops. Per-thread roots (call_stack /
    /// pending_exception / func_ref_slots) stay captured via `Rc<RefCell>`
    /// clones from the unique VmContext.
    pub(crate) heap:               Box<dyn MagrGC>,
    /// **add-vmcontext-registry (2026-05-20)**: registry of all live
    /// [`VmContext`] instances on this VmCore (one per OS thread).
    /// Populated by `VmContext::new()`; cleared by `VmContext::drop()`.
    /// The GC scanner closure walks this list under lock to find every
    /// thread's per-thread roots. See `VmContextPtr` SAFETY block.
    pub(crate) vm_contexts:        Mutex<Vec<VmContextPtr>>,
    /// **add-threading-stdlib (2026-05-20)**: the user's compiled Module,
    /// shared via `Arc` across all threads on this VmCore. `None` in test
    /// paths that don't need a real Module (most cargo unit tests construct
    /// VmContext via `VmContext::new()` 0-arg which leaves this `None`).
    /// Production paths use `VmContext::with_module(module)` to populate.
    /// `__thread_spawn` requires this to be `Some` (panics in test paths if
    /// missing, which is acceptable since tests don't spawn threads).
    pub(crate) module:             Option<Arc<crate::metadata::Module>>,
    /// **add-threading-stdlib (2026-05-20)**: live `Std.Threading.Thread`
    /// instances keyed by monotonic u64 slot id. `__thread_spawn` inserts;
    /// `__thread_join` takes-out + joins. Pattern mirrors
    /// `add-std-process` processes registry.
    pub(crate) threads:            Mutex<std::collections::HashMap<u64, std::thread::JoinHandle<anyhow::Result<()>>>>,
    /// **add-threading-stdlib (2026-05-20)**: monotonic thread slot id
    /// counter; never reused (u64 effectively unbounded).
    pub(crate) next_thread_id:     std::sync::atomic::AtomicU64,
    /// **add-sync-primitives (2026-05-20)**: `Std.Threading.Mutex<T>`
    /// slot table. `__mutex_new` inserts; `__mutex_unlock` keeps the
    /// entry (Mutexes are reusable). The `Arc` lets the lock-acquire
    /// thread keep the inner mutex alive across builtin call boundaries
    /// via a thread-local guard registry — see `corelib/sync.rs`.
    pub(crate) mutexes:            Mutex<HashMap<u64, Arc<parking_lot::Mutex<Value>>>>,
    /// **add-sync-primitives (2026-05-20)**: monotonic mutex slot id
    /// counter; never reused.
    pub(crate) next_mutex_id:      std::sync::atomic::AtomicU64,
    /// **add-sync-primitives (2026-05-20)**: `Std.Threading.Channel<T>`
    /// slot table. `__channel_new` inserts; `__channel_close` flips
    /// `sender = None` so subsequent recv sees disconnected. Entries
    /// are never removed in v0 (no `__channel_drop` builtin) — the
    /// Channel object's lifetime keeps the slot alive for the whole VM
    /// run, which is acceptable for normal workloads but documented as
    /// a Deferred for future cleanup (`add-sync-primitives-future-gc`).
    pub(crate) channels:           Mutex<HashMap<u64, crate::corelib::sync::ChannelSlot>>,
    /// **add-sync-primitives (2026-05-20)**: monotonic channel slot id
    /// counter; never reused.
    pub(crate) next_channel_id:    std::sync::atomic::AtomicU64,
    /// **add-gc-safepoint (2026-05-20)**: cooperative-polling GC safepoint
    /// phase. Mutators read this at each `check_safepoint` and park when
    /// non-Idle. The collector flips Idle → Requested → Marking → Idle
    /// under the protocol in [`crate::gc::safepoint`].
    pub(crate) gc_phase:           Mutex<crate::gc::safepoint::GcPhase>,
    /// **add-gc-safepoint (2026-05-20)**: Condvar used by both sides —
    /// mutators wait on it to resume; the collector waits on it to learn
    /// when `parked_count` reached its threshold.
    pub(crate) gc_phase_cv:        parking_lot::Condvar,
    /// **add-gc-safepoint (2026-05-20)**: number of mutator VmContexts
    /// currently parked at a safepoint (excludes the collector). Used by
    /// the collector to know when stop-the-world is in effect.
    pub(crate) parked_count:       std::sync::atomic::AtomicUsize,
    /// **add-multi-collector-arbitration (2026-05-21)**: exclusive
    /// collector claim. `request_gc_pause` CAS-es false→true; only the
    /// winner becomes the active collector for one round. Losers
    /// park-as-mutator and return `None`. Cleared by `GcPauseGuard::drop`.
    pub(crate) collector_active:   std::sync::atomic::AtomicBool,
    /// **add-gc-safepoint-auto-threshold (2026-05-20)**: shared AtomicBool
    /// that `ArcMagrGC::maybe_auto_collect` sets on pressure trip;
    /// `check_safepoint(ctx)` swaps it to `false` and takes ownership of
    /// the round's stop-the-world collect. Cross-thread safe via the
    /// safepoint protocol.
    pub(crate) needs_auto_collect: Arc<std::sync::atomic::AtomicBool>,
    /// **add-z42-launcher (2026-06-02)**: command-line arguments passed to
    /// the running z42 program — everything after the `--` separator on the
    /// `z42vm` command line. Exposed to z42 code via
    /// `Std.IO.Environment.GetCommandLineArgs()` (builtin `__env_args`).
    /// Shared on `VmCore` so worker threads see the same argv as main.
    /// Set once at startup by `VmContext::set_program_args`; empty by default
    /// (bare `z42vm file entry` with no `--` → no program args).
    pub(crate) program_args:       Mutex<Vec<String>>,
    /// **add-sync-primitives-rwlock (2026-05-20)**: `Std.Threading.RwLock<T>`
    /// slot table. Multiple shared (read) holders OR a single exclusive
    /// (write) holder. Same Arc-+-thread-local-guard parking pattern as
    /// Mutex, with an additional Read/Write variant tracked per slot so
    /// release picks the correct unlock path.
    pub(crate) rwlocks:            Mutex<HashMap<u64, Arc<parking_lot::RwLock<Value>>>>,
    /// **add-sync-primitives-rwlock (2026-05-20)**: monotonic RwLock slot
    /// id counter; never reused.
    pub(crate) next_rwlock_id:     std::sync::atomic::AtomicU64,
    /// **add-z42-compression (2026-05-22)**: stdlib native extension builtins
    /// (e.g. `__deflate_compress` from libz42_compression). Populated at VM
    /// startup by `crate::native::ext::load_all`, which scans the SDK native
    /// search path, dlopens each `libz42_*.{so,dylib,dll}`, and lets it
    /// register `(name, fn_ptr)` pairs. Lookup parallels static `BUILTINS[]`;
    /// see `corelib::ext_builtin_id_of` for the resolver fallback.
    /// Only present when `native-interop` feature is enabled (gated alongside
    /// the `native` module in lib.rs).
    #[cfg(feature = "native-interop")]
    pub(crate) ext_builtins:       Mutex<crate::native::ext::ExtBuiltinTable>,
    /// **add-z42-io-filestream (2026-05-24)**: live `Std.IO.FileStream`
    /// handles keyed by monotonic slot id. `__file_open` inserts;
    /// `__file_close` removes (or marks slot dead). Pattern mirrors
    /// `processes` / `mutexes` / `channels` / `compressors` slot tables.
    pub(crate) file_handles:       Mutex<HashMap<u64, crate::corelib::fs::FileHandleSlot>>,
    /// **add-z42-io-filestream (2026-05-24)**: monotonic file-handle slot
    /// id counter; never reused.
    pub(crate) next_file_handle_id: std::sync::atomic::AtomicU64,

    // ── add-z42-net K1 (2026-05-24) ───────────────────────────────────────
    /// live `Std.Net.Sockets.TcpClient` streams keyed by monotonic u64 slot
    /// id. `__net_tcp_connect` / `__net_tcp_accept` insert; `__net_tcp_socket_drop`
    /// removes (TcpStream Drop closes the fd). wasm32 target: never populated.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tcp_sockets:          Mutex<HashMap<u64, std::net::TcpStream>>,
    /// live `Std.Net.Sockets.TcpListener` instances keyed by monotonic u64 slot id.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tcp_listeners:        Mutex<HashMap<u64, std::net::TcpListener>>,
    /// monotonic TCP socket slot id; never reused.
    pub(crate) next_tcp_socket_id:   std::sync::atomic::AtomicU64,
    /// monotonic TCP listener slot id; never reused.
    pub(crate) next_tcp_listener_id: std::sync::atomic::AtomicU64,

    // ── add-z42-net-tls (2026-06-03) ──────────────────────────────────────
    /// live rustls client TLS streams (TCP + handshake state) keyed by
    /// monotonic u64 slot id. `__net_tls_connect` inserts; `__net_tls_drop`
    /// removes (the owned `StreamOwned` drops its `TcpStream` → closes the
    /// fd). Slot space is independent from `tcp_sockets`. wasm32: never
    /// populated (TLS builtins return KIND_UNSUPPORTED).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tls_sockets:
        Mutex<HashMap<u64, rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>>>,
    /// monotonic TLS socket slot id; never reused.
    pub(crate) next_tls_socket_id:   std::sync::atomic::AtomicU64,

    // ── add-z42-net-udp (K2, 2026-05-25) ───────────────────────────────────
    /// live `Std.Net.Sockets.UdpClient` sockets keyed by monotonic u64 slot id.
    /// `__net_udp_bind` inserts; `__net_udp_drop` removes (UdpSocket Drop closes
    /// the fd). wasm32 target: never populated.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) udp_sockets:          Mutex<HashMap<u64, std::net::UdpSocket>>,
    /// monotonic UDP socket slot id; never reused.
    pub(crate) next_udp_socket_id:   std::sync::atomic::AtomicU64,

    /// **add-runtime-counters (2026-05-26)**: atomic observation-only
    /// counters for JIT compiles / builtin calls / native calls /
    /// exception traffic. Surfaced via `--print-stats-on-exit` and
    /// (future) scripted `Std.Diagnostics.RuntimeStats.Snapshot()`.
    /// `Arc` wrapper enables cheap cloning into Phase 2 increment sites
    /// that don't already have `&VmCore` access. docs/review.md Part 4 D6.
    pub counters: Arc<crate::counters::RuntimeCounters>,

    /// **add-runtime-observer (2026-05-26)**: push-based event stream
    /// registry. Symmetric to existing `GcObserver` (which stays in the
    /// heap) but for non-GC events (`ModuleLoaded`, future JIT compile /
    /// exception / native call). Embedders register via
    /// [`VmContext::add_runtime_observer`]; subsystems fire via
    /// [`VmContext::fire_runtime_event`]. docs/review.md Part 4 D3.
    pub runtime_observers: crate::observer::RuntimeObserverRegistry,
}

/// Runtime-mutable state shared across one VM instance's interp + JIT paths.
///
/// All `RefCell` fields take `&self` so JIT extern-C call sites (which reach
/// the receiver through `*mut VmContext`) can avoid producing `&mut`. The
/// `heap` field is `Box<dyn MagrGC>` without `RefCell` because it is set once
/// in `new()` and never replaced; trait methods take `&self` and the
/// implementation handles its own interior mutability.
///
/// **Phase 3d.1 (2026-04-29)**: `static_fields` / `pending_exception` /
/// `lazy_loader` 改用 `Rc<RefCell<...>>` 包装，让 `ArcMagrGC` 的 external
/// root scanner 闭包能 clone Rc 共享访问，从而 mark_reachable_set 把这些
/// 字段持有的 Value 也纳入 GC roots（修复 cycle collector 漏扫 static_fields
/// 导致误清的 bug）。
pub struct VmContext {
    /// Shared-across-threads state (add-multithreading-foundation, 2026-05-19).
    /// See [`VmCore`] for the field list and per-phase migration table.
    /// Static fields, type registries, native interop registry etc. accessed
    /// through `self.core.<field>.lock()` (or `.read()` for RwLock variants).
    pub(crate) core: Arc<VmCore>,
    /// **Phase 3 revision of Decision 2** (2026-05-20): per-thread state is
    /// Arc<Mutex<>> instead of Rc<RefCell<>> because the GC scanner closure
    /// must be `Send + Sync` (MagrGC trait now requires it), which forces
    /// every closure capture to also be Send + Sync. Single-thread overhead
    /// is small (~few ns per lock vs RefCell borrow), worth the architectural
    /// consistency. Tracked in design.md Decision 2 amendment.
    pub(crate) pending_exception: Arc<Mutex<Option<Value>>>,
    /// 2026-05-10 unify-frame-chain: single source of truth for active
    /// script frames. Each [`crate::exception::VmFrame`] carries the
    /// `(name, file, line, column)` trace metadata **and** raw pointers
    /// to `regs` / `env_arena` for GC root scanning.
    ///
    /// Raw ptrs valid only while the owning Rust frame
    /// (`interp::Frame` / `JitFrame`) is alive — `FrameGuard` RAII for
    /// interp + paired `push_frame` / `pop_frame` in JIT helpers ensure
    /// the pop runs before the owner returns.
    pub(crate) call_stack:        Arc<Mutex<Vec<crate::exception::VmFrame>>>,
    /// 2026-05-02 add-method-group-conversion (D1b): module-level FuncRef cache
    /// slots. `LoadFnCached { slot_id }` 首次执行时把 `Value::FuncRef(name)`
    /// 写入 `func_ref_slots[slot_id]`；后续命中直接 load。
    pub(crate) func_ref_slots:    Arc<Mutex<Vec<Value>>>,
    /// **add-vmcontext-registry (2026-05-20)**: marks `VmContext: !Unpin`,
    /// so callers cannot `mem::swap` / move out of the `Pin<Box<VmContext>>`
    /// returned by [`new`]. Required so the raw pointer registered in
    /// [`VmCore::vm_contexts`] stays valid for the entire lifetime.
    _pin: PhantomPinned,
    // heap moved to VmCore (Phase 2.2)

    // native_types / native_libs / pinned_owned_buffers moved to VmCore (Phase 1.7-1.9)

    // processes moved to VmCore (Phase 2.1)
    pub(crate) process_next_id:   std::sync::atomic::AtomicU64,
    /// **add-gc-safepoint-counter-throttling (2026-05-21)**: per-thread
    /// throttle counter. `check_safepoint`'s fast path decrements; only
    /// when it reaches 0 does the slow path probe `gc_phase` and drain
    /// `needs_auto_collect`. Initial value comes from
    /// [`crate::gc::safepoint::throttle_n`] (default 1024, env-overridable).
    pub(crate) safepoint_skip:    std::sync::atomic::AtomicU32,
}

// `Default` removed: `new()` now returns `Pin<Box<VmContext>>`, which
// cannot satisfy `Default::default() -> Self`. Test helpers that
// previously used `VmContext::default()` should call `VmContext::new()`
// directly and accept `Pin<Box<VmContext>>` (deref still works for
// method calls).

impl VmContext {
    /// Public accessor for the shared compiled Module installed by
    /// [`with_module`](Self::with_module). Returns `None` if VmContext was
    /// built via [`new`](Self::new) (test path).
    pub fn module(&self) -> Option<&Arc<crate::metadata::Module>> {
        self.core.module.as_ref()
    }

    /// Clone the shared `Arc<VmCore>` — needed by external integration
    /// tests / embedders that spawn raw OS threads and want to construct
    /// a child VmContext via [`new_with_core`](Self::new_with_core). The
    /// `core` field itself is `pub(crate)`; this is the public escape hatch.
    pub fn core_arc(&self) -> Arc<VmCore> {
        Arc::clone(&self.core)
    }

    /// Public accessor for runtime atomic counters. Used by main.rs
    /// `--print-stats-on-exit` flag and embedders that want to observe
    /// JIT compiles / builtin calls / exception traffic.
    /// docs/review.md Part 4 D6 Phase 1 (2026-05-26).
    pub fn counters(&self) -> &crate::counters::RuntimeCounters {
        &self.core.counters
    }

    /// Register a [`crate::observer::RuntimeObserver`]. The observer
    /// receives every subsequent [`crate::observer::RuntimeEvent`] fired
    /// via [`Self::fire_runtime_event`]. Multiple observers fan-out.
    /// docs/review.md Part 4 D3 Phase 1 (2026-05-26).
    pub fn add_runtime_observer(&self, obs: Arc<dyn crate::observer::RuntimeObserver>) {
        self.core.runtime_observers.add(obs);
    }

    /// Fire a runtime event to all registered observers. Returns the
    /// number of observers that received the event. No-op when registry
    /// is empty (cost = one lock acquire + length check). Safe to call
    /// from any thread; observer callbacks must be `Send + Sync` so they
    /// handle cross-thread invocation themselves.
    pub fn fire_runtime_event(&self, event: &crate::observer::RuntimeEvent) -> usize {
        self.core.runtime_observers.fire(event)
    }

    /// add-gc-safepoint-counter-throttling (2026-05-21): force the next
    /// `check_safepoint` call into the slow path immediately, bypassing
    /// the throttle counter. For tests and embedders that need
    /// deterministic safepoint timing — production code should not need
    /// this (the throttle counter caps GC pause latency at N iterations
    /// which is small enough in practice).
    pub fn force_safepoint(&self) {
        self.safepoint_skip.store(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// **add-z42-launcher (2026-06-02)**: install the running program's
    /// command-line arguments (the tokens after `--` on the `z42vm`
    /// invocation). Read back by the `__env_args` builtin →
    /// `Std.IO.Environment.GetCommandLineArgs()`. Called once at startup
    /// from `main.rs`; safe to call from any context sharing this `VmCore`.
    pub fn set_program_args(&self, args: Vec<String>) {
        *self.core.program_args.lock() = args;
    }

    /// **add-z42-launcher (2026-06-02)**: snapshot the program argv.
    pub fn program_args(&self) -> Vec<String> {
        self.core.program_args.lock().clone()
    }

    /// Standard test entry: constructs a VmContext with `VmCore.module = None`.
    /// Cargo unit tests use this — they don't need a real Module for
    /// builtin / static-field / alloc tests. Production paths use
    /// [`with_module`](Self::with_module).
    pub fn new() -> std::pin::Pin<Box<Self>> {
        Self::new_internal(None)
    }

    /// Production entry: constructs a VmContext with the user's compiled
    /// Module wrapped in `Arc` for cross-thread sharing. Required by any
    /// path that may invoke `__thread_spawn` (which dispatches into the
    /// shared module from the spawned thread).
    pub fn with_module(module: crate::metadata::Module) -> std::pin::Pin<Box<Self>> {
        Self::new_internal(Some(Arc::new(module)))
    }

    /// Spawned-thread entry: build a VmContext that **shares** an existing
    /// `Arc<VmCore>` instead of creating a new one. Used by
    /// `__thread_spawn`'s spawned closure so the worker thread sees the
    /// same module / static_fields / heap / lazy_loader / native_libs
    /// state as the parent thread.
    ///
    /// The new VmContext registers itself in `core.vm_contexts` for GC root
    /// scanning, so the worker's per-thread roots (`pending_exception` /
    /// `call_stack` / `func_ref_slots`) are visible to the cycle collector.
    /// On drop, the entry is removed under the same Mutex discipline as
    /// the primary path.
    pub fn new_with_core(core: Arc<VmCore>) -> std::pin::Pin<Box<Self>> {
        let pending_exception: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let func_ref_slots: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let call_stack: Arc<Mutex<Vec<crate::exception::VmFrame>>> = Arc::new(Mutex::new(Vec::new()));

        let ctx = Self {
            core,
            pending_exception,
            call_stack,
            func_ref_slots,
            process_next_id: std::sync::atomic::AtomicU64::new(1),
            safepoint_skip: std::sync::atomic::AtomicU32::new(crate::gc::safepoint::throttle_n()),
            _pin: PhantomPinned,
        };
        let boxed = Box::new(ctx);
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        boxed.core.vm_contexts.lock().push(ptr);
        unsafe { std::pin::Pin::new_unchecked(boxed) }
    }

    fn new_internal(module: Option<Arc<crate::metadata::Module>>) -> std::pin::Pin<Box<Self>> {
        let pending_exception: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let func_ref_slots: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        // 2026-05-10 unify-frame-chain: single Vec<VmFrame> replaces the
        // previous trio (exec_stack / env_arena_stack / call_stack).
        let call_stack: Arc<Mutex<Vec<crate::exception::VmFrame>>> = Arc::new(Mutex::new(Vec::new()));

        // Construct VmCore with heap embedded; scanner is installed AFTER
        // wrapping in Arc so the closure can capture Weak<VmCore> (cycle
        // avoidance: heap owns the scanner, scanner refs core, core owns
        // heap → strong-Arc loop = leak). Weak.upgrade() per call.
        let core: Arc<VmCore> = Arc::new(VmCore {
            static_fields:      Mutex::new(Vec::new()),
            static_field_index: Mutex::new(HashMap::new()),
            lazy_loader:        Mutex::new(None),
            context_registry:   Mutex::new(crate::metadata::context::ContextRegistry::new()),
            #[cfg(feature = "native-interop")]
            native_types:       RwLock::new(HashMap::new()),
            #[cfg(feature = "native-interop")]
            native_libs:        Mutex::new(Vec::new()),
            pinned_owned_buffers: Mutex::new(HashMap::new()),
            pending_thrown:       Mutex::new(None),
            processes:            Mutex::new(HashMap::new()),
            heap:                 Box::new(ArcMagrGC::new()),
            vm_contexts:          Mutex::new(Vec::new()),
            module,
            threads:              Mutex::new(HashMap::new()),
            next_thread_id:       std::sync::atomic::AtomicU64::new(1),
            mutexes:              Mutex::new(HashMap::new()),
            next_mutex_id:        std::sync::atomic::AtomicU64::new(1),
            channels:             Mutex::new(HashMap::new()),
            next_channel_id:      std::sync::atomic::AtomicU64::new(1),
            gc_phase:             Mutex::new(crate::gc::safepoint::GcPhase::Idle),
            gc_phase_cv:          parking_lot::Condvar::new(),
            parked_count:         std::sync::atomic::AtomicUsize::new(0),
            collector_active:     std::sync::atomic::AtomicBool::new(false),
            needs_auto_collect:   Arc::new(std::sync::atomic::AtomicBool::new(false)),
            program_args:         Mutex::new(Vec::new()),
            rwlocks:              Mutex::new(HashMap::new()),
            next_rwlock_id:       std::sync::atomic::AtomicU64::new(1),
            #[cfg(feature = "native-interop")]
            ext_builtins:         Mutex::new(crate::native::ext::ExtBuiltinTable::default()),
            file_handles:         Mutex::new(HashMap::new()),
            next_file_handle_id:  std::sync::atomic::AtomicU64::new(1),
            #[cfg(not(target_arch = "wasm32"))]
            tcp_sockets:          Mutex::new(HashMap::new()),
            #[cfg(not(target_arch = "wasm32"))]
            tcp_listeners:        Mutex::new(HashMap::new()),
            next_tcp_socket_id:   std::sync::atomic::AtomicU64::new(1),
            next_tcp_listener_id: std::sync::atomic::AtomicU64::new(1),
            #[cfg(not(target_arch = "wasm32"))]
            tls_sockets:          Mutex::new(HashMap::new()),
            next_tls_socket_id:   std::sync::atomic::AtomicU64::new(1),
            #[cfg(not(target_arch = "wasm32"))]
            udp_sockets:          Mutex::new(HashMap::new()),
            next_udp_socket_id:   std::sync::atomic::AtomicU64::new(1),

            // add-runtime-counters (2026-05-26): all-zero start; Phase 1
            // increment site = corelib::exec_builtin (builtin_calls).
            counters: Arc::new(crate::counters::RuntimeCounters::new()),
            // add-runtime-observer (2026-05-26): empty registry; embedders
            // attach via `VmContext::add_runtime_observer`. Phase 1 emits
            // ModuleLoaded from main.rs after each load_artifact.
            runtime_observers: crate::observer::RuntimeObserverRegistry::new(),
        });

        // add-os-signal-handler (2026-05-25): register this Arc<VmCore> into
        // the process-wide VM_CORES registry so the POSIX signal handler
        // can walk it to capture z42 call stacks on hard crash. Lazy-prune
        // dead Weak entries at the same time (cheap O(n) sweep amortized
        // across VmCore creations).
        if let Ok(mut g) = VM_CORES.lock() {
            g.retain(|w| w.strong_count() > 0);
            g.push(Arc::downgrade(&core));
        }

        // add-gc-safepoint-auto-threshold (2026-05-20): wire the
        // needs_auto_collect flag into the heap so its pressure-trip
        // path defers to the next safepoint check instead of collecting
        // inline (the inline path has no &VmContext and would race with
        // concurrent mutators' frame.regs writes).
        core.heap.set_external_needs_collect_flag(Arc::clone(&core.needs_auto_collect));

        // External GC root scanner — invoked by the cycle collector during
        // mark phase. Walks all out-of-heap Value sources so cycles whose
        // only roots are static fields / pending exception / live frame
        // regs / stack closure envs / func-ref slots stay alive.
        //
        // **add-vmcontext-registry (2026-05-20)**: scanner walks the
        // `vm_contexts` registry to find every live VmContext on this
        // VmCore. Each VmContext contributes its own per-thread roots
        // (pending_exception / call_stack frames / func_ref_slots). The
        // closure captures `Weak<VmCore>` ONLY — no per-thread Arc clones.
        {
            let core_weak = Arc::downgrade(&core);
            core.heap.set_external_root_scanner(Box::new(move |visit| {
                let Some(c) = core_weak.upgrade() else { return; };
                // 1. Shared static fields.
                for v in c.static_fields.lock().iter() {
                    visit(v);
                }
                // 2-4. Per-thread roots, one VmContext per OS thread.
                //
                // SAFETY: each VmContextPtr was registered via
                // `VmContext::new()` *after* its `Pin<Box<...>>` heap
                // allocation, and `VmContext::drop` removes the entry
                // BEFORE the Box is dealloc'd. We hold `vm_contexts.lock()`
                // for the full walk, so a concurrent drop on another thread
                // blocks until we release — no use-after-free possible.
                let registry = c.vm_contexts.lock();
                for ctx_ptr in registry.iter() {
                    let ctx = unsafe { &*ctx_ptr.0 };
                    // pending_exception
                    if let Some(v) = ctx.pending_exception.lock().as_ref() {
                        visit(v);
                    }
                    // live z42 frame state — unified VmFrame entries.
                    //
                    // SAFETY (frame.regs / env_arena): raw ptrs valid for
                    // the lifetime of the owning Rust frame (FrameGuard
                    // RAII for interp; paired push/pop for JIT). GC
                    // collect is invoked from inside script code, so
                    // every walk sees pointers still in-bounds.
                    for frame in ctx.call_stack.lock().iter() {
                        unsafe {
                            for v in (*frame.regs).iter() {
                                visit(v);
                            }
                            if !frame.env_arena.is_null() {
                                for env in (*frame.env_arena).iter() {
                                    for v in env.iter() {
                                        visit(v);
                                    }
                                }
                            }
                        }
                    }
                    // method group conversion cache slots (D1b).
                    for v in ctx.func_ref_slots.lock().iter() {
                        visit(v);
                    }
                }
            }));
        }

        let ctx = Self {
            core,
            pending_exception,
            call_stack,
            func_ref_slots,
            process_next_id: std::sync::atomic::AtomicU64::new(1),
            safepoint_skip: std::sync::atomic::AtomicU32::new(crate::gc::safepoint::throttle_n()),
            _pin: PhantomPinned,
        };
        // Heap-allocate so the address is stable for the scanner registry.
        let boxed = Box::new(ctx);
        // SAFETY: VmContext registers its OWN address into VmCore.vm_contexts
        // here and removes the entry in Drop (running BEFORE Box dealloc).
        // The Pin wrapper + PhantomPinned prevents any subsequent move-out.
        let ptr = VmContextPtr(&*boxed as *const VmContext);
        boxed.core.vm_contexts.lock().push(ptr);

        // add-z42-compression (2026-05-22): scan native search paths +
        // dlopen each lib*.{so,dylib,dll}, populating `ext_builtins`. Run
        // once at primary-VM init only (workers via `new_with_core` reuse
        // the parent's populated table). Failures are logged but never
        // abort startup — apps that don't need any ext lib still boot.
        #[cfg(feature = "native-interop")]
        if let Err(e) = crate::native::ext::load_all(&boxed) {
            tracing::warn!("native ext loader: {:#}", e);
        }

        // SAFETY: We never expose `&mut Box<VmContext>` to user code (only
        // `Pin<&mut VmContext>` via `Pin::as_mut`, which respects
        // `PhantomPinned`), so the contents stay at a stable address until
        // Drop. Constructing the Pin here is the standard idiom for
        // self-referential heap data.
        unsafe { std::pin::Pin::new_unchecked(boxed) }
    }

    // ── Process slot table (add-std-process, 2026-05-13) ──────────────────

    /// Allocate a new slot id and store `slot` under it. Returns the id
    /// for the z42 `ProcessHandle` to carry. Counter is monotonic and
    /// never reused; u64 overflow is not a practical concern (10^19
    /// spawns).
    pub fn alloc_process_slot(&self, slot: crate::corelib::process::ProcessSlot) -> u64 {
        // Phase 3 (multi-threading foundation): Cell<u64> → AtomicU64 for
        // Send/Sync. Relaxed ordering is fine — slot IDs only need to be
        // unique within the VM and the registry lock orders observations.
        let id = self.process_next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.core.processes.lock().insert(id, slot);
        id
    }

    /// Remove and return the slot. Used by `wait` / `kill`+reap / `drop`
    /// which take ownership of `child` etc.
    pub fn take_process_slot(&self, slot_id: u64) -> Option<crate::corelib::process::ProcessSlot> {
        self.core.processes.lock().remove(&slot_id)
    }

    /// Peek at the slot in-place. Returns `None` if the slot id is
    /// unknown. Callback runs while the outer `RefCell` is borrowed —
    /// callers must not invoke other slot methods inside.
    pub fn with_process_slot<T>(
        &self,
        slot_id: u64,
        f: impl FnOnce(&mut crate::corelib::process::ProcessSlot) -> T,
    ) -> Option<T> {
        let mut map = self.core.processes.lock();
        map.get_mut(&slot_id).map(f)
    }

    /// Number of currently allocated process slots. Used by tests to
    /// verify cleanup paths drop entries.
    pub fn process_slot_count(&self) -> usize {
        self.core.processes.lock().len()
    }

    // ── add-z42-net K1 (2026-05-24): TCP socket / listener slot helpers ──
    //
    // Pattern mirrors `alloc_process_slot` exactly. Counter lives in `core`
    // (shared across threads) so a server thread accepting connections + a
    // worker thread reading from them can't collide on slot ids.

    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_tcp_socket_slot(&self, stream: std::net::TcpStream) -> u64 {
        let id = self.core.next_tcp_socket_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.core.tcp_sockets.lock().insert(id, stream);
        id
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_tcp_listener_slot(&self, listener: std::net::TcpListener) -> u64 {
        let id = self.core.next_tcp_listener_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.core.tcp_listeners.lock().insert(id, listener);
        id
    }

    /// add-z42-net-tls (2026-06-03): register a connected + handshaken rustls
    /// stream and return its slot id. `__net_tls_drop` removes (closing the fd).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_tls_socket_slot(
        &self,
        stream: rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>,
    ) -> u64 {
        let id = self.core.next_tls_socket_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.core.tls_sockets.lock().insert(id, stream);
        id
    }

    /// Number of currently allocated TLS socket slots. Used by tests to
    /// verify cleanup paths drop entries.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tls_socket_slot_count(&self) -> usize {
        self.core.tls_sockets.lock().len()
    }

    /// Number of currently allocated TCP socket slots. Used by tests to
    /// verify cleanup paths drop entries.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tcp_socket_slot_count(&self) -> usize {
        self.core.tcp_sockets.lock().len()
    }

    /// Number of currently allocated TCP listener slots.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tcp_listener_slot_count(&self) -> usize {
        self.core.tcp_listeners.lock().len()
    }

    // ── add-z42-net-udp (K2, 2026-05-25) ────────────────────────────────

    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_udp_socket_slot(&self, sock: std::net::UdpSocket) -> u64 {
        let id = self.core.next_udp_socket_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.core.udp_sockets.lock().insert(id, sock);
        id
    }

    /// Number of currently allocated UDP socket slots.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn udp_socket_slot_count(&self) -> usize {
        self.core.udp_sockets.lock().len()
    }

    // ── Native interop (Tier 1, spec C2) ──────────────────────────────────
    //
    // 2026-05-12 add-platform-wasm Stage 0: entire interop API gated on
    // `native-interop` feature. wasm builds drop these methods (and the
    // backing fields) entirely.

    /// Register a native type with this VM. Returns `false` (with [`crate::native::error::set`]
    /// already populated) on duplicate `(module, name)`. Internally invoked
    /// from `z42_register_type`; tests may also call this directly with a
    /// pre-built [`RegisteredType`].
    #[cfg(feature = "native-interop")]
    pub fn register_native_type(
        &self,
        ty: Arc<crate::native::RegisteredType>,
    ) -> bool {
        let key = (ty.module().to_string(), ty.type_name().to_string());
        let mut map = self.core.native_types.write();
        if map.contains_key(&key) {
            return false;
        }
        map.insert(key, ty);
        true
    }

    /// Look up a previously registered native type. Returns `None` when the
    /// `(module, name)` pair is unknown.
    #[cfg(feature = "native-interop")]
    pub fn resolve_native_type(
        &self,
        module: &str,
        name: &str,
    ) -> Option<Arc<crate::native::RegisteredType>> {
        let key = (module.to_string(), name.to_string());
        self.core.native_types.read().get(&key).cloned()
    }

    /// Total number of registered native types — primarily for tests.
    #[cfg(feature = "native-interop")]
    pub fn native_type_count(&self) -> usize {
        self.core.native_types.read().len()
    }

    // ── Pinned owned buffers (spec C10 — Array<u8> pin) ──────────────────

    /// Register an owned byte buffer (the snapshot of an `Array<u8>` taken
    /// during `PinPtr`). Returns the buffer's data pointer for storage in
    /// the `Value::PinnedView`. The buffer remains alive until
    /// [`release_owned_buffer`] is called from a matching `UnpinPtr`.
    pub fn pin_owned_buffer(&self, buf: Box<[u8]>) -> u64 {
        let ptr = buf.as_ptr() as u64;
        self.core.pinned_owned_buffers.lock().insert(ptr, buf);
        ptr
    }

    /// Drop an owned buffer previously registered via [`pin_owned_buffer`].
    /// Idempotent: silently no-ops if `ptr` isn't registered (e.g. `Str`
    /// pins which never enter the table).
    pub fn release_owned_buffer(&self, ptr: u64) {
        let _ = self.core.pinned_owned_buffers.lock().remove(&ptr);
    }

    /// Total number of currently-pinned owned buffers — exposed for
    /// tests asserting that UnpinPtr cleaned up.
    pub fn pinned_owned_buffer_count(&self) -> usize {
        self.core.pinned_owned_buffers.lock().len()
    }

    // ── Pending reflected throw (add-method-invoke-non-generic) ──────────────

    /// Stash a z42 exception value to propagate through a callback builtin's
    /// error channel (see `pending_thrown`). Set immediately before the builtin
    /// returns `Err`; consumed once by `take_pending_thrown`.
    pub fn set_pending_thrown(&self, val: Value) {
        *self.core.pending_thrown.lock() = Some(val);
    }

    /// Take (and clear) a pending thrown exception value, if any. Called by
    /// `exec_call::builtin` in its error handler so the ORIGINAL thrown value
    /// (with its real type) re-enters z42 exception handling.
    pub fn take_pending_thrown(&self) -> Option<Value> {
        self.core.pending_thrown.lock().take()
    }

    /// Load a native library and invoke its `<basename>_register` entry point.
    /// The library handle is stored on `self` until VM drop. Errors are
    /// returned as `anyhow::Error` and mirrored into the thread-local
    /// last-error slot so C callers see the same diagnostic via
    /// [`z42_last_error`](z42_abi::z42_last_error).
    #[cfg(feature = "native-interop")]
    pub fn load_native_library(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<()> {
        crate::native::loader::load_library(self, path.as_ref())
    }

    // ── Interp exec stack（Phase 3f） ─────────────────────────────────────

    /// Push current frame's regs pointer onto exec_stack, used by GC root
    /// scanning. Caller must guarantee pointer stays valid until matching
    /// `pop_frame_regs()` (typically via `FrameGuard` RAII).
    /// 2026-05-02 add-method-group-conversion (D1b): ensure VmContext has at
    /// least `n` FuncRef cache slots allocated. Idempotent — only grows.
    pub fn alloc_func_ref_slots(&self, n: u32) {
        let mut s = self.func_ref_slots.lock();
        if s.len() < n as usize {
            s.resize(n as usize, Value::Null);
        }
    }

    /// LoadFnCached read: returns slot value, or `Value::Null` if uninitialised
    /// (caller's responsibility to fill on first miss). Bounds-checked.
    pub(crate) fn func_ref_slot(&self, idx: u32) -> Value {
        self.func_ref_slots
            .lock()
            .get(idx as usize)
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// LoadFnCached write: store a `Value::FuncRef` into the slot for future hits.
    pub(crate) fn set_func_ref_slot(&self, idx: u32, value: Value) {
        let mut s = self.func_ref_slots.lock();
        if (idx as usize) >= s.len() {
            s.resize((idx as usize) + 1, Value::Null);
        }
        s[idx as usize] = value;
    }

    // ── Frame chain (2026-05-10 unify-frame-chain) ────────────────────────
    //
    // Single push_frame / pop_frame replaces the previously-separate
    // (push_frame_state / pop_frame_regs) + (push_call_frame / pop_call_frame)
    // pairs. Atomic push of one VmFrame holds GC roots + trace metadata
    // together — caller cannot "forget half".

    /// Push one [`crate::exception::VmFrame`] onto the active script frame
    /// chain. Pop is the caller's responsibility (typically via the
    /// interp `FrameGuard` RAII or the explicit pair in JIT helpers).
    pub(crate) fn push_frame(&self, frame: crate::exception::VmFrame) {
        self.call_stack.lock().push(frame);
    }

    /// Pop the most recently pushed frame. No-op when empty (defensive).
    pub(crate) fn pop_frame(&self) {
        self.call_stack.lock().pop();
    }

    /// Update the *top* (currently executing) frame's source position.
    /// Called by callers right before they invoke a callee, so the snapshot
    /// at a downstream `throw` shows the call site, not 0.
    ///
    /// `column = 0` means unknown — the snapshot formats as `(file:line)`
    /// rather than `(file:line:col)`.
    pub(crate) fn update_top_frame_pos(&self, line: u32, column: u32) {
        if let Some(top) = self.call_stack.lock().last() {
            top.line.set(line);
            top.column.set(column);
        }
    }

    /// Snapshot the entire call stack for stack-trace formatting at a
    /// `throw` site. Cheap clone (small-string + u32 per frame); only
    /// invoked on the throw path so per-instruction overhead is zero.
    pub(crate) fn snapshot_call_stack(&self) -> Vec<crate::exception::FrameSnapshot> {
        self.call_stack.lock().iter().map(|f| f.snapshot()).collect()
    }

    /// Current depth of the call stack — debugging / tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn call_stack_depth(&self) -> usize {
        self.call_stack.lock().len()
    }

    /// Spec impl-ref-out-in-runtime (Decision R1): index into the frame
    /// chain and return a raw pointer to that frame's `regs` Vec.
    /// Used by `Value::Ref { kind: RefKind::Stack { frame_idx, .. } }`
    /// transparent deref in `Frame::get/set`.
    ///
    /// # Safety
    /// Caller must:
    ///   1. Use the returned pointer only while the corresponding frame is
    ///      still alive (guaranteed by spec design Decision 9: refs never
    ///      escape the call stack — popped frames cannot be referenced).
    ///   2. Not race with concurrent push/pop on the same VmContext (single
    ///      RefCell borrow boundary; deref is synchronous within a frame).
    pub(crate) fn frame_state_at(&self, idx: usize) -> Option<*const Vec<Value>> {
        let stack = self.call_stack.lock();
        stack.get(idx).map(|f| f.regs)
    }

    /// Current depth of the frame chain. `frame_state_at(depth - 1)` is
    /// the most recent frame. Used by codegen-generated `LoadLocalAddr`
    /// to produce a `RefKind::Stack { frame_idx }` referencing the
    /// current frame at emission time.
    pub(crate) fn frame_stack_depth(&self) -> usize {
        self.call_stack.lock().len()
    }

    // ── GC heap ───────────────────────────────────────────────────────────

    /// Borrow the GC heap as a trait object. All script-driven allocations go
    /// through this entry point; see `docs/design/runtime/gc.md`.
    pub fn heap(&self) -> &dyn MagrGC {
        self.core.heap.as_ref()
    }

    // ── Static fields ─────────────────────────────────────────────────────
    //
    // Layout (introduce-method-token, 2026-05-08):
    //   `static_fields: Vec<Value>`           — slot storage by StaticFieldId.0
    //   `static_field_index: HashMap<&str, u32>` — name → id (lazy-allocated)
    //
    // The legacy by-name `static_get` / `static_set` API lazy-allocates an
    // ID on first write and looks up by name on read (returning Null if no
    // ID is yet allocated). The dispatch hot path uses `static_get_by_id`
    // / `static_set_by_id` after `metadata::resolver` has populated
    // per-instruction `StaticFieldId` cache slots.

    /// Resolve (or lazy-allocate) a `StaticFieldId` for the given full-qualified
    /// static-field name. Idempotent. Called by `metadata::resolver` at module
    /// load and by hot paths on cache miss (cross-zpkg lazy fields).
    pub fn resolve_static_field_id(&self, name: &str) -> crate::metadata::tokens::StaticFieldId {
        let mut idx = self.core.static_field_index.lock();
        if let Some(&id) = idx.get(name) {
            return crate::metadata::tokens::StaticFieldId(id);
        }
        let id = idx.len() as u32;
        idx.insert(name.to_string(), id);
        // Extend backing Vec to match index.
        let mut sf = self.core.static_fields.lock();
        if (id as usize) >= sf.len() {
            sf.resize_with((id + 1) as usize, || Value::Null);
        }
        crate::metadata::tokens::StaticFieldId(id)
    }

    /// Read a user-class static field by name. Unset fields read as
    /// `Value::Null`. Lazy fallback for cross-zpkg paths and JIT helpers
    /// not yet threading `StaticFieldId`.
    pub fn static_get(&self, field: &str) -> Value {
        let idx = self.core.static_field_index.lock();
        match idx.get(field) {
            Some(&id) => self
                .core
                .static_fields
                .lock()
                .get(id as usize)
                .cloned()
                .unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    /// Write a user-class static field by name. Lazy-allocates the id on
    /// first write.
    pub fn static_set(&self, field: &str, val: Value) {
        let id = self.resolve_static_field_id(field);
        self.static_set_by_id(id, val);
    }

    /// Hot-path read by id (no hash). Caller must have a resolved id.
    /// Returns `Value::Null` if id ≥ Vec length (unallocated slot).
    #[inline]
    pub fn static_get_by_id(&self, id: crate::metadata::tokens::StaticFieldId) -> Value {
        self.core
            .static_fields
            .lock()
            .get(id.0 as usize)
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// Hot-path write by id. Caller must have a resolved id; the slot
    /// is auto-extended if id ≥ current Vec length.
    #[inline]
    pub fn static_set_by_id(&self, id: crate::metadata::tokens::StaticFieldId, val: Value) {
        let mut sf = self.core.static_fields.lock();
        if (id.0 as usize) >= sf.len() {
            sf.resize_with((id.0 + 1) as usize, || Value::Null);
        }
        sf[id.0 as usize] = val;
    }

    /// Drop all static fields (used by `run_with_static_init` to ensure a
    /// clean slate before each entry-point run). Resets values to Null but
    /// **keeps the index** so previously-allocated `StaticFieldId`s stay
    /// stable across runs (resolver-populated IDs in `Function.resolved`
    /// remain valid after re-init).
    pub fn static_fields_clear(&self) {
        let mut sf = self.core.static_fields.lock();
        for slot in sf.iter_mut() {
            *slot = Value::Null;
        }
    }

    // ── JIT exception bridge ──────────────────────────────────────────────

    /// JIT helpers store a thrown user value here; the JIT entry sees the
    /// `extern "C"` return code = 1 and pulls the value via
    /// `take_exception()` to propagate as `ExecOutcome::Thrown`.
    pub fn set_exception(&self, val: Value) {
        *self.pending_exception.lock() = Some(val);
    }

    /// Pop the pending exception (called once per `extern "C"` failure).
    pub fn take_exception(&self) -> Option<Value> {
        self.pending_exception.lock().take()
    }

    /// Peek at the pending exception without removing it. Used by JIT catch-type
    /// dispatch (catch-by-generic-type, 2026-05-06): the throw helper has set the
    /// exception, the dispatch helper inspects its class to decide which catch
    /// handler to jump to, and a later `take_exception` (via `jit_install_catch`)
    /// hands the value to the chosen catch register.
    pub fn peek_exception(&self) -> Option<Value> {
        self.pending_exception.lock().clone()
    }

    // ── Lazy loader (delegates to LazyLoader struct) ─────────────────────

    /// Install with no declared dependencies — for tests / single-file
    /// scripts without stdlib references.
    pub fn install_lazy_loader(&self, libs_dir: Option<PathBuf>, main_pool_len: usize) {
        self.install_lazy_loader_with_deps(
            libs_dir.into_iter().collect(), main_pool_len, Vec::new(), Vec::new());
    }

    /// Install with declared deps (see `LazyLoader::new` for parameter docs).
    /// `search_dirs` are consulted in order to resolve transitive dep zpkgs by
    /// file name (typically `[entry-zpkg dir, stdlib libs dir]`).
    pub fn install_lazy_loader_with_deps(
        &self,
        search_dirs: Vec<PathBuf>,
        main_pool_len: usize,
        declared: Vec<(String, ZpkgCandidate)>,
        initially_loaded: Vec<String>,
    ) {
        *self.core.lazy_loader.lock() = Some(LazyLoader::new(
            search_dirs,
            main_pool_len,
            declared,
            initially_loaded,
        ));
    }

    /// Clear the lazy loader (used in tests).
    pub fn uninstall_lazy_loader(&self) {
        *self.core.lazy_loader.lock() = None;
    }

    /// fix-cross-pkg-subclass-fields (2026-05-14): seed the lazy loader's
    /// `type_registry` with TypeDescs from eagerly-loaded modules (e.g. the
    /// merged main module). Used by both the z42vm CLI and the in-process
    /// test runner immediately after `install_lazy_loader_with_deps` so the
    /// fixup pass can find eagerly-loaded base classes when lazy-loading a
    /// subclass.
    pub fn seed_lazy_loader_types(&self, types: &HashMap<String, Arc<TypeDesc>>) {
        let mut state = self.core.lazy_loader.lock();
        if let Some(loader) = state.as_mut() {
            loader.seed_types_for_lookup(types);
        }
    }

    /// add-crosspkg-impl-reflection: seed `(target_fq, trait_fq)` impl pairs
    /// from an eagerly-loaded artifact (main zpkg / eagerly-loaded deps) into
    /// the lazy loader's impls registry. Companion of `seed_lazy_loader_types`.
    pub fn seed_lazy_loader_impls(&self, pairs: &[(String, String)]) {
        let mut state = self.core.lazy_loader.lock();
        if let Some(loader) = state.as_mut() {
            loader.seed_impls(pairs);
        }
    }

    /// Look up a function by FQ name; triggers lazy load if needed.
    ///
    /// review.md D3 Phase 2 (2026-05-27): emits `RuntimeEvent::ModuleLoaded`
    /// for every zpkg the resolver pulled in transitively, so observers
    /// see lazy-load activity (not just boot-time eager loads).
    /// Load a compiled artifact at `path` into the live registries (functions +
    /// types become callable / reflectable) and return its TIDX test entries.
    /// Backs `Std.Test.ModuleLoader.Load` for the reflective test runner.
    /// Errors if no lazy loader is installed (a bare VM with no dep resolution).
    pub fn load_module_into_vm(
        &self, path: &str,
    ) -> anyhow::Result<Vec<crate::metadata::test_index::LoadedTestEntry>> {
        let mut state = self.core.lazy_loader.lock();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadModule: no lazy loader installed (cannot register loaded module)")
        })?;
        loader.load_module_from_path(path)
    }

    /// In-memory sibling of [`load_module_into_vm`]: load a compiled artifact
    /// from raw bytes (packed zpkg / bare zbc) into the live registries. Backs
    /// the `__load_bytecode_in_memory` builtin used by `z42.scripting` (REPL) so
    /// freshly compiled bytecode is invocable with zero disk I/O. (add-z42-repl)
    /// Returns the freshly-loaded module's own `*.__static_init__` function names
    /// so the caller runs only this round's static init (REPL carry-forward — a full
    /// clear+rerun would wipe prior rounds' mutated static state).
    pub fn load_module_bytes_into_vm(
        &self, raw: &[u8],
    ) -> anyhow::Result<Vec<String>> {
        let mut state = self.core.lazy_loader.lock();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadBytecodeInMemory: no lazy loader installed (cannot register loaded module)")
        })?;
        loader.load_module_from_bytes(raw)
    }

    pub fn try_lookup_function(&self, func_name: &str) -> Option<Arc<Function>> {
        let (result, newly_loaded) = {
            let mut state = self.core.lazy_loader.lock();
            let loader = state.as_mut()?;
            let before: std::collections::HashSet<String> = loader.loaded_zpkgs.clone();
            let result = loader.resolve_function(func_name);
            let newly: Vec<String> = loader.loaded_zpkgs.difference(&before).cloned().collect();
            (result, newly)
        };
        for name in newly_loaded {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
        result
    }

    /// Look up a class TypeDesc by FQ name; triggers lazy load if needed.
    /// Same `ModuleLoaded` emit semantics as `try_lookup_function`.
    pub fn try_lookup_type(&self, class_name: &str) -> Option<Arc<TypeDesc>> {
        let (result, newly_loaded) = {
            let mut state = self.core.lazy_loader.lock();
            let loader = state.as_mut()?;
            let before: std::collections::HashSet<String> = loader.loaded_zpkgs.clone();
            let result = loader.resolve_type(class_name);
            let newly: Vec<String> = loader.loaded_zpkgs.difference(&before).cloned().collect();
            (result, newly)
        };
        for name in newly_loaded {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
        result
    }

    /// add-crosspkg-impl-reflection (unify P1-e): traits added to `target_fq`
    /// via cross-package `impl Trait for Type`, aggregated from every loaded
    /// package's IMPL section (returns owned Vec — the registry lives behind
    /// the lazy-loader lock). Empty when no loader / no impls.
    pub fn impl_traits_for(&self, target_fq: &str) -> Vec<String> {
        let state = self.core.lazy_loader.lock();
        match state.as_ref() {
            Some(loader) => loader.impl_traits_for(target_fq).to_vec(),
            None => Vec::new(),
        }
    }

    /// All currently-loaded type names — entry module's `type_registry` plus the
    /// lazy loader's already-loaded types. add-nested-types: `GetNestedTypes`
    /// scans these for `<outer>+<simple>` members. A type's nested types live in
    /// the same package as their declaring type, so resolving the declaring type
    /// (its handle is in hand) has already force-loaded the package — no need to
    /// force-load everything. Deduped across both sources.
    pub fn loaded_type_names(&self) -> Vec<String> {
        let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(m) = self.module() {
            for name in m.type_registry.keys() {
                set.insert(name.clone());
            }
        }
        {
            let state = self.core.lazy_loader.lock();
            if let Some(loader) = state.as_ref() {
                for name in loader.iter_type_names() {
                    set.insert(name.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    /// Resolve an "overflow" ConstStr index past the main module's pool.
    /// Returns `Arc<str>` (review.md C3 Phase 1, 2026-06-03) so callers can
    /// wrap directly into `Value::Str` without a second allocation.
    pub fn try_lookup_string(&self, absolute_idx: usize) -> Option<std::sync::Arc<str>> {
        let state = self.core.lazy_loader.lock();
        let loader = state.as_ref()?;
        loader.try_lookup_string(absolute_idx)
    }

    /// All namespaces declared by lazy-loadable zpkgs (for static-init scan).
    pub fn declared_namespaces(&self) -> Vec<String> {
        let state = self.core.lazy_loader.lock();
        match state.as_ref() {
            Some(loader) => loader.declared_namespaces(),
            None         => Vec::new(),
        }
    }

    /// Force-load every declared zpkg, then return a sorted list of all
    /// `*.__static_init__` function names across loaded zpkgs.
    ///
    /// fix-multi-file-static-init (2026-05-15): the compiler now emits
    /// `<ns>.<source-stem>.__static_init__` (one per CU). A single
    /// per-namespace lookup can't find them all, so the runtime force-loads
    /// each declared zpkg and enumerates the loader's function table for
    /// the suffix. Sorted for determinism.
    pub fn collect_lazy_static_init_names(&self) -> Vec<String> {
        let mut state = self.core.lazy_loader.lock();
        let Some(loader) = state.as_mut() else { return Vec::new(); };
        loader.force_load_all_declared();
        let mut names: Vec<String> = loader.iter_function_names()
            .filter(|n| n.ends_with(".__static_init__"))
            .cloned()
            .collect();
        names.sort();
        names
    }
}

impl Drop for VmContext {
    /// **add-vmcontext-registry (2026-05-20)**: deregister this `VmContext`
    /// from `VmCore.vm_contexts` so the GC scanner stops trying to walk a
    /// soon-to-be-freed allocation. Runs BEFORE the underlying `Box` storage
    /// is released (Rust drop order: contents → Box dealloc), so any GC
    /// scan racing this Drop will block on the registry lock and see the
    /// post-removed list.
    fn drop(&mut self) {
        let ptr = self as *const Self;
        self.core.vm_contexts.lock().retain(|p| p.0 != ptr);
        // Wake any collector sleeping in request_handshake_pause so it
        // re-evaluates the required park count. Our removal from vm_contexts
        // may lower vm_contexts.len()-1 below the current parked_count,
        // satisfying the wait condition. Must hold gc_phase lock to prevent
        // a missed-wakeup between the condition re-check and the wait call.
        let _g = self.core.gc_phase.lock();
        self.core.gc_phase_cv.notify_all();
    }
}

#[cfg(test)]
#[path = "vm_context_tests.rs"]
mod vm_context_tests;
