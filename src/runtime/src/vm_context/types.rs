use super::*;

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
    pub(crate) static_field_index: Mutex<FxHashMap<String, u32>>,
    /// On-demand zpkg loader. `None` until `install_lazy_loader[_with_deps]`
    /// is called (typically from `bootstrap.rs`). Shared across threads
    /// since zpkg resolution + module loading is a process-global operation.
    ///
    /// **fix-lazy-lookup-contention (2026-09-05)**: `RwLock`, not `Mutex`. 符号查找
    /// （ConstStr / 函数 / 类型）在**稳态下是纯读** —— registry 已命中就不碰任何可变状态，
    /// 只有真要加载 zpkg 才需要独占。原来一律取独占锁，导致多线程编译时所有 worker 在
    /// 这里串行化：`--jobs 8` 的原生采样里 7662 个阻塞样本落在 `try_lookup_string` /
    /// `try_lookup_function` / `try_lookup_type` 上，而 GC 分配锁只有 6 个。
    pub(crate) lazy_loader:        RwLock<Option<LazyLoader>>,
    /// defer-class-initialization: 待初始化的**所属类** FQN 队列。由
    /// `metadata::resolver` 解析 `StaticGet`/`StaticSet` 时入队（字段名去掉最后一段），
    /// 在 `VmContext::run_pending_static_inits` 里逐个 `try_lookup_type` 触发所属包加载
    /// + 初始化。这是「静态字段引用触发类初始化」的落点：热路径
    /// `static_get_by_id` 因 `Value::Null` 是合法值而无法区分「未初始化」，
    /// 故触发点前移到「名字→id 解析」这一每名一次的冷路径。
    pub(crate) pending_type_inits: Mutex<Vec<String>>,
    /// **cache-failed-name-resolution**: lock-free mirrors of "how much static-init
    /// work is outstanding", so `try_lookup_*` can prove the drain would be a no-op
    /// **without** taking two more mutexes and scanning `static_init_state` on every
    /// single lookup. `pending_type_init_count` mirrors `pending_type_inits.len()`;
    /// both are maintained at the mutation sites, under the same lock as the state
    /// they mirror, and are only ever trusted for the `== 0` ("provably nothing to
    /// do") direction.
    pub(crate) pending_type_init_count: std::sync::atomic::AtomicUsize,
    /// Number of `__static_init__` bodies currently `Running` on any thread. Bumped
    /// where `InitState::Running` is written and dropped where `Done` is written,
    /// both under the loader lock, so `== 0` implies no `Running` entry exists.
    pub(crate) running_static_inits: std::sync::atomic::AtomicUsize,
    /// fix-static-init-claim-window：**有线程正在处理一批初始化工作**（从两个队列
    /// 取走内容起，到这批处理完为止）。
    ///
    /// 光有 `pending_type_init_count` / `pending_static_inits` / `running_static_inits`
    /// 不够：`run_pending_static_inits` 把类型队列 `mem::take` 走并把计数清零之后，要等
    /// `try_lookup_type` 触发所属包加载，才会有新的 `__static_init__` 入队。这中间三个
    /// 指标全是零，别的线程据此判定「初始化已静止」→ 读到未赋值的静态字段。
    /// 本计数在**取队列之前**自增、整批处理完才自减，把这段窗口盖住。
    pub(crate) init_batch_inflight: std::sync::atomic::AtomicUsize,
    /// defer-class-initialization: 懒执行的 `__static_init__` 抛出/出错时记录首个失败，
    /// 由 `Vm::run` 在入口返回后转成运行错误（对齐变更前 boot 期 `bail!` 的响度）。
    pub(crate) static_init_error:  Mutex<Option<String>>,
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
    /// `kill`+reap / explicit `drop`. **M2**: id counter now embedded in the
    /// registry (was `VmContext::process_next_id`, per-thread) → ids are unique
    /// per-core like every other socket/handle table.
    pub(crate) processes:          ResourceRegistry<crate::corelib::process::ProcessSlot>,
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
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) threads:            ResourceRegistry<std::thread::JoinHandle<anyhow::Result<()>>>,
    /// **add-sync-primitives (2026-05-20)**: `Std.Threading.Mutex<T>`
    /// slot table. `__mutex_new` inserts; `__mutex_unlock` keeps the
    /// entry (Mutexes are reusable). The `Arc` lets the lock-acquire
    /// thread keep the inner mutex alive across builtin call boundaries
    /// via a thread-local guard registry — see `corelib/sync.rs`.
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) mutexes:            ResourceRegistry<Arc<parking_lot::Mutex<Value>>>,
    /// **add-sync-primitives (2026-05-20)**: `Std.Threading.Channel<T>`
    /// slot table. `__channel_new` inserts; `__channel_close` flips
    /// `sender = None` so subsequent recv sees disconnected. Entries
    /// are never removed in v0 (no `__channel_drop` builtin) — the
    /// Channel object's lifetime keeps the slot alive for the whole VM
    /// run, which is acceptable for normal workloads but documented as
    /// a Deferred for future cleanup (`add-sync-primitives-future-gc`).
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) channels:           ResourceRegistry<crate::corelib::sync::ChannelSlot>,
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
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) rwlocks:            ResourceRegistry<Arc<parking_lot::RwLock<Value>>>,
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
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) file_handles:       ResourceRegistry<crate::corelib::fs::FileHandleSlot>,

    // ── add-z42-net K1 (2026-05-24) ───────────────────────────────────────
    /// live `Std.Net.Sockets.TcpClient` streams keyed by monotonic u64 slot
    /// id. `__net_tcp_connect` / `__net_tcp_accept` insert; `__net_tcp_socket_drop`
    /// removes (TcpStream Drop closes the fd). wasm32 target: never populated.
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tcp_sockets:          ResourceRegistry<std::net::TcpStream>,
    /// live `Std.Net.Sockets.TcpListener` instances keyed by monotonic u64 slot id.
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tcp_listeners:        ResourceRegistry<std::net::TcpListener>,

    // ── add-z42-net-tls (2026-06-03) ──────────────────────────────────────
    /// live rustls client TLS streams (TCP + handshake state) keyed by
    /// monotonic u64 slot id. `__net_tls_connect` inserts; `__net_tls_drop`
    /// removes (the owned `StreamOwned` drops its `TcpStream` → closes the
    /// fd). Slot space is independent from `tcp_sockets`. wasm32: never
    /// populated (TLS builtins return KIND_UNSUPPORTED).
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) tls_sockets:
        ResourceRegistry<rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>>,

    // ── add-z42-net-udp (K2, 2026-05-25) ───────────────────────────────────
    /// live `Std.Net.Sockets.UdpClient` sockets keyed by monotonic u64 slot id.
    /// `__net_udp_bind` inserts; `__net_udp_drop` removes (UdpSocket Drop closes
    /// the fd). wasm32 target: never populated.
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) udp_sockets:          ResourceRegistry<std::net::UdpSocket>,

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

    /// **add-concurrency-probes (2026-08-23, script-profiling P1b)**: distribution
    /// of how long mutators spend PARKED at a GC safepoint (STW stall as seen by
    /// each stopped thread). Recorded in `gc::safepoint::park_until_idle`, which
    /// only runs during an actual GC pause — so this costs the hot safepoint-check
    /// path nothing (always-on). Reuses `PauseHistogram` (same shape as GC pauses).
    pub(crate) park_histogram: Mutex<crate::gc::types::PauseHistogram>,

    /// **add-concurrency-probes (2026-08-23)**: user-lock (`Std.Threading.Mutex` /
    /// `RwLock`) contention counters. Written ONLY when the VM is built with the
    /// `profile-contention` cargo feature (the probe in `corelib::sync` is
    /// `#[cfg]`-gated); the default build never touches them → they stay 0.
    /// `lock_contentions` = acquires that found the lock already held (`try_lock`
    /// failed); `lock_wait_us` = cumulative µs blocked on those contended acquires.
    pub(crate) lock_contentions: std::sync::atomic::AtomicU64,
    pub(crate) lock_wait_us:     std::sync::atomic::AtomicU64,

    /// **add-sampling-profiler (2026-08-24, script-profiling P2)**: safepoint
    /// sampling profiler. `Sampler::disabled()` unless `Z42_SAMPLE_HZ` is set,
    /// in which case a background timer thread flags samples that mutators
    /// snapshot at their next safepoint (`gc::safepoint::check_safepoint_slow`
    /// Idle tail). Default-off is zero-cost: no thread, one `enabled()` atomic
    /// load on the already-throttled slow path. Produces a folded flamegraph
    /// (+ optional perfetto sample-trace when `Z42_TRACE_OUT` is set).
    pub(crate) sampler: crate::gc::Sampler,
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
    /// add-escape-analysis-stack-alloc: per-thread arena holding escape-analysis
    /// stack-allocated objects/arrays (`Value::StackObject`/`StackArray` index it).
    /// LIFO-truncated by `pop_frame` to each frame's stamped base. Scanned as GC
    /// roots at safepoint (its slots may hold heap `GcRef`s). Mutex: owner-thread
    /// accesses uncontended; GC scanner is the only cross-thread reader.
    pub(crate) stack_arena:       Arc<Mutex<crate::interp::stack_alloc::StackArena>>,
    /// add-struct-value-semantics: per-thread byte arena holding value-struct blobs
    /// (`Value::StructRef` indexes it). Same lifetime model as `stack_arena`
    /// (LIFO-truncated by `pop_frame`, GC-scanned at safepoint).
    pub(crate) struct_arena:      Arc<Mutex<crate::interp::struct_arena::StructArena>>,
    /// make-value-copy: per-thread arena holding the payloads of the four transient,
    /// frame-scoped `Value` variants (`Ref`/`PinnedView`/`StackClosure`/`StructRefHeap`,
    /// which now carry only an 8B `{idx,frame_id}` handle so `Value` is `Copy`). Same
    /// lifetime model as `stack_arena` (LIFO-truncated by `pop_frame`, GC-scanned at safepoint).
    pub(crate) transient_arena:   Arc<Mutex<crate::interp::transient_arena::TransientArena>>,
    /// perf interp-frame-lock-slim: published lengths of the three arenas above,
    /// mirroring their inner `Vec` len(s) (`stack_arena` has two — objs / arrs).
    /// Written ONLY by the mutator thread — under the relevant arena's own `Mutex`
    /// on every alloc (via the `*_alloc` wrappers below) and on `pop_frame`'s
    /// truncate. Read lock-free (`Relaxed`) by `push_frame` (to capture a frame's
    /// truncation base without locking the arena) and by `pop_frame` (to skip the
    /// arena lock + truncate entirely when the length is unchanged — the
    /// overwhelmingly common call-heavy case where a frame allocates nothing on
    /// these arenas). Single-writer (mutator) ⇒ `Relaxed` suffices: a thread always
    /// observes its own prior writes in program order, and the GC scanner never
    /// touches these atomics (it reads arena DATA under the `Mutex`). This removes
    /// 3 arena locks from every `push_frame` and 3 from the common `pop_frame`.
    pub(crate) stack_obj_len:     std::sync::atomic::AtomicUsize,
    pub(crate) stack_arr_len:     std::sync::atomic::AtomicUsize,
    pub(crate) struct_len:        std::sync::atomic::AtomicUsize,
    pub(crate) transient_len:     std::sync::atomic::AtomicUsize,
    /// add-escape-analysis-stack-alloc: monotonic per-frame id source (stamped onto
    /// each interp `Frame` at entry; keys arena slots for stale-handle diagnostics).
    pub(crate) next_frame_id:     std::sync::atomic::AtomicU32,
    /// runtime-jit-tiering Phase 1.5 (mixed-mode): forward pointer to the active
    /// `JitModuleCtx`, mirroring the existing `JitModuleCtx.vm_ctx` back-pointer.
    /// Type-erased as `usize` (the `jit` module is `cfg`-gated; this field is not)
    /// — cast back to `*const jit::frame::JitModuleCtx` at the interp dispatch hook.
    /// Set by `JitModule::run_fn` for the duration of one entry call, 0 outside it.
    /// Lets an interp frame (running as a JIT cold-tier / fallback) route an
    /// already-compiled callee to its native code instead of re-interpreting.
    pub(crate) jit_ctx:           std::sync::atomic::AtomicUsize,
    /// 2026-05-02 add-method-group-conversion (D1b): module-level FuncRef cache
    /// slots. `LoadFnCached { slot_id }` 首次执行时把 `Value::FuncRef(name)`
    /// 写入 `func_ref_slots[slot_id]`；后续命中直接 load。
    pub(crate) func_ref_slots:    Arc<Mutex<Vec<Value>>>,
    /// **unify-gc-heap PR-4**: per-context lazy interning cache for `ConstStr` pool
    /// literals. The `Str` bytes moved into the GC heap, but the interned pool is
    /// built at module *load* time when no heap exists — so instead of an eager
    /// load-time interned pool, the first `ConstStr(idx)` allocates a GC
    /// string from the live heap and caches it here (keyed by `(module ptr, idx)`).
    /// Cached entries are **GC roots** (scanned by the external root scanner), so the
    /// interned strings survive collection while this context is alive; subsequent
    /// hits copy the 8-byte handle (no re-alloc). Per-context (not shared) so no
    /// module mutation / cross-thread interning — a thread re-interns its own literals
    /// (negligible for z42's 1–2 threads). See `interp::exec_value::const_str`.
    pub(crate) interned_cache:    Arc<Mutex<FxHashMap<(usize, u32), crate::metadata::vstr::Str>>>,
    /// **optimize-subclass-check**: memoizes `is_subclass_or_eq_td(derived, target) → bool`
    /// (interp `is`/`as`/`catch`/vcall dispatch). Without it, every `x is T` check walks the
    /// derived type's whole base+interface chain and — because the module's `type_registry`
    /// rarely holds cross-zpkg types (e.g. `z42.ir`'s `IrInstr` subclasses while z42c
    /// serializes) — falls through to `try_lookup_type` (the `lazy_loader` lock) per level.
    /// z42c's zpkg serialization dispatches each instruction through a ~60-way `is`-chain,
    /// making this the top interp hotspot (profiled). The relationship is a global,
    /// monotonic fact (a loaded type's bases/interfaces never change; lazy-load only ADDS
    /// types), so the result is cacheable; nested `String→String→bool` map so a hit resolves
    /// by `&str` with zero allocation. Per-context (not shared) → no lock contention under
    /// parallel `--jobs` compile. Cleared on explicit module (re)load (REPL redefinition).
    pub(crate) subclass_memo:     Mutex<FxHashMap<String, FxHashMap<String, bool>>>,
    /// perf-vm-isa-cache (2026-09-03): identity-keyed direct-mapped front cache for
    /// `is` / `as` / typed `catch` in front of `subclass_memo` — a hit is two relaxed loads,
    /// no lock, no string hashing. See `isa_cache.rs` for the key/lifetime contract; cleared
    /// together with the memo on explicit module (re)load.
    pub(crate) isa_cache:         super::isa_cache::IsaCache,
    /// **add-vmcontext-registry (2026-05-20)**: marks `VmContext: !Unpin`,
    /// so callers cannot `mem::swap` / move out of the `Pin<Box<VmContext>>`
    /// returned by [`new`]. Required so the raw pointer registered in
    /// [`VmCore::vm_contexts`] stays valid for the entire lifetime.
    pub(crate) _pin: PhantomPinned,
    // heap moved to VmCore (Phase 2.2)

    // native_types / native_libs / pinned_owned_buffers moved to VmCore (Phase 1.7-1.9)

    // processes moved to VmCore (Phase 2.1); **M2**: the process slot-id
    // counter moved with it, into `VmCore::processes` (ResourceRegistry) —
    // it was the last per-thread counter for a shared table.
    /// **add-gc-safepoint-counter-throttling (2026-05-21)**: per-thread
    /// throttle counter. `check_safepoint`'s fast path decrements; only
    /// when it reaches 0 does the slow path probe `gc_phase` and drain
    /// `needs_auto_collect`. Initial value comes from
    /// [`crate::gc::safepoint::throttle_n`] (default 1024, env-overridable).
    pub(crate) safepoint_skip:    std::sync::atomic::AtomicU32,
}

/// Byte offset of [`VmContext::safepoint_skip`] within `VmContext`.
///
/// **inline-jit-safepoint-check (2026-08-01)**: the JIT emits a native
/// load/store of the throttle counter (`jit::translate::emit_safepoint_check`)
/// instead of a helper call. `offset_of!` is compile-time and independent of
/// field reordering / `#[repr(Rust)]`, so it always yields the actual offset.
pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET: usize =
    std::mem::offset_of!(VmContext, safepoint_skip);

// `Default` removed: `new()` now returns `Pin<Box<VmContext>>`, which
// cannot satisfy `Default::default() -> Self`. Test helpers that
// previously used `VmContext::default()` should call `VmContext::new()`
// directly and accept `Pin<Box<VmContext>>` (deref still works for
// method calls).
