use super::*;

/// **`VmCore`** —— state shared across all threads sharing one VM instance
/// (add-multithreading-foundation, Phase 1 / spec
/// `2026-05-19-add-multithreading-foundation`).
///
/// Holds fields that are **process-globally singular**: static fields, type
/// registries, native interop registry, pinned buffer table, etc. Per-thread
/// state (call stack, pending exception, frame guards, func-ref cache slots)
/// stays directly on [`VmContext`], which references this struct through
/// `Arc<VmCore>`.
///
/// It also holds the VM's lazy loader, cctor registry, native interop registry,
/// GC heap and resource tables.
///
/// `VmCore` will become `Send + Sync` once the GC handle backing switches to
/// `Arc<...>` (Phase 3 of the spec). Until then, `Mutex` is wrapped around
/// fields that hold `Value` (which transitively contains `Rc<RefCell>` via
/// the current `ArrRef` backing), so the API surface is already stable —
/// Send-safety completes at Phase 3 without further VmCore API changes.
pub struct VmCore {
    /// Static fields: one stable-address cell per `StaticFieldId` (read / written without a
    /// lock), plus the FQN → id registration (lazy, first use; cross-zpkg fields can be met in
    /// any order). See `vm_context::static_table`.
    pub(crate) statics: super::static_table::StaticTable,
    /// On-demand zpkg loader. `None` until `install_lazy_loader[_with_deps]`
    /// is called (typically from `bootstrap.rs`). Shared across threads
    /// since zpkg resolution + module loading is a process-global operation.
    ///
    /// **fix-lazy-lookup-contention **: `RwLock`, not `Mutex`. 符号查找
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
    /// add-static-constructors：有静态构造器的类型的按类型初始化状态。**只登记有 cctor
    /// 的类型**；热路径通过其无锁 `pending` 计数短路（`== 0` ⇒ 全程序无待初始化 cctor
    /// ⇒ 屏障免费）。详见 `vm_context/cctor.rs` 模块注释。
    /// fix-crosspkg-static-call-cctor：`Arc` 是为了让惰性加载器也持有一份——它在类型入表的那一刻
    /// 登记 cctor（见 `LazyLoader::insert_type`）。
    pub(crate) cctors: Arc<super::cctor::CctorRegistry>,
    /// **cache-failed-name-resolution**: lock-free mirrors of "how much static-init
    /// work is outstanding", so `try_lookup_*` can prove the drain would be a no-op
    /// **without** taking two more mutexes and scanning `static_init_state` on every
    /// single lookup. `pending_type_init_count` mirrors `pending_type_inits.len()`;
    /// both are maintained at the mutation sites, under the same lock as the state
    /// they mirror, and are only ever trusted for the `== 0` ("provably nothing to
    /// do") direction.
    pub(crate) pending_type_init_count: std::sync::atomic::AtomicUsize,
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
    /// **add-load-context-model **: registry of load contexts +
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
    /// runtime-audit P0-4: a fatal VM error (stack overflow) is unwinding —
    /// no z42 handler runs and no error becomes a z42 exception (`stack_guard`).
    /// Per VM: every thread of this VM, no other VM in the process.
    pub(crate) fatal: std::sync::atomic::AtomicBool,
    /// add-std-process  — live `Std.IO.Process` children
    /// spawned via `__process_start`. Keyed by monotonic u64 slot id
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
    /// pending_exception) stay captured via `Rc<RefCell>`
    /// clones from the unique VmContext.
    pub(crate) heap:               Box<dyn MagrGC>,
    /// **add-vmcontext-registry **: registry of all live
    /// [`VmContext`] instances on this VmCore (one per OS thread).
    /// Populated by `VmContext::new()`; cleared by `VmContext::drop()`.
    /// The GC scanner closure walks this list under lock to find every
    /// thread's per-thread roots. See `VmContextPtr` SAFETY block.
    pub(crate) vm_contexts:        Mutex<Vec<VmContextPtr>>,
    /// **add-threading-stdlib **: the user's compiled Module,
    /// shared via `Arc` across all threads on this VmCore. `None` in test
    /// paths that don't need a real Module (most cargo unit tests construct
    /// VmContext via `VmContext::new()` 0-arg which leaves this `None`).
    /// Production paths use `VmContext::with_module(module)` to populate.
    /// `__thread_spawn` requires this to be `Some` (panics in test paths if
    /// missing, which is acceptable since tests don't spawn threads).
    pub(crate) module:             Option<Arc<crate::metadata::Module>>,
    /// Process-level function identity (`FnId` → `Function`, lock-free reads).
    /// Entry-module functions are registered at construction as ids `0..n`
    /// (= `module.functions` index); the lazy loader appends each package's
    /// functions as they register. Only registers today — no lookup path reads
    /// it yet. See `metadata::func_table`.
    pub(crate) funcs:              Arc<crate::metadata::func_table::FuncTable>,
    /// Process-level type identity (`TypeId` → latest `TypeDesc` version, lock-free reads).
    /// Entry-module types are registered at construction; the lazy loader publishes its
    /// registry after every change. Also reserves type-test keys for target names that
    /// name no registered type. See `metadata::type_table`.
    pub(crate) types:              Arc<crate::metadata::type_table::TypeTable>,
    /// String-literal identity (`ConstStr` id → text + interned GC string, lock-free reads
    /// once interned). Entry-module pool = ids `0..n`; the lazy loader appends each package's
    /// pool. Per VM because the strings live in this VM's heap; its interned strings are GC
    /// roots. See `metadata::str_table`.
    pub(crate) strings:            Arc<crate::metadata::str_table::StrTable>,
    /// **add-threading-stdlib **: live `Std.Threading.Thread`
    /// instances keyed by monotonic u64 slot id. `__thread_spawn` inserts;
    /// `__thread_wait` takes-out + joins. Pattern mirrors
    /// `add-std-process` processes registry.
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) threads:            ResourceRegistry<std::thread::JoinHandle<anyhow::Result<()>>>,
    /// **add-sync-primitives **: `Std.Threading.Mutex<T>`
    /// slot table. `__mutex_new` inserts; `__mutex_unlock` keeps the
    /// entry (Mutexes are reusable). The `Arc` lets the lock-acquire
    /// thread keep the inner mutex alive across builtin call boundaries
    /// via a thread-local guard registry — see `corelib/sync.rs`.
    /// (**M2**: slot-id counter embedded in the registry.)
    /// **add-sync-primitives **: `Std.Threading.Channel<T>`
    /// slot table. `__channel_new` inserts; `__channel_close` flips
    /// `sender = None` so subsequent recv sees disconnected. Entries
    /// are never removed in v0 (no `__channel_drop` builtin) — the
    /// Channel object's lifetime keeps the slot alive for the whole VM
    /// run, which is acceptable for normal workloads but documented as
    /// a Deferred for future cleanup (`add-sync-primitives-future-gc`).
    /// (**M2**: slot-id counter embedded in the registry.)
    /// **add-gc-safepoint **: cooperative-polling GC safepoint
    /// phase. Mutators read this at each `check_safepoint` and park when
    /// non-Idle. The collector flips Idle → Requested → Marking → Idle
    /// under the protocol in [`crate::gc::safepoint`].
    pub(crate) gc_phase:           Mutex<crate::gc::safepoint::GcPhase>,
    /// **add-gc-safepoint **: Condvar used by both sides —
    /// mutators wait on it to resume; the collector waits on it to learn
    /// when `parked_count` reached its threshold.
    pub(crate) gc_phase_cv:        parking_lot::Condvar,
    /// **add-gc-safepoint **: number of mutator VmContexts
    /// currently parked at a safepoint (excludes the collector). Used by
    /// the collector to know when stop-the-world is in effect.
    pub(crate) parked_count:       std::sync::atomic::AtomicUsize,
    /// **add-multi-collector-arbitration **: exclusive
    /// collector claim. `request_gc_pause` CAS-es false→true; only the
    /// winner becomes the active collector for one round. Losers
    /// park-as-mutator and return `None`. Cleared by `GcPauseGuard::drop`.
    pub(crate) collector_active:   std::sync::atomic::AtomicBool,
    /// **add-gc-safepoint-auto-threshold **: shared AtomicBool
    /// that `ArcMagrGC::maybe_auto_collect` sets on pressure trip;
    /// `check_safepoint(ctx)` swaps it to `false` and takes ownership of
    /// the round's stop-the-world collect. Cross-thread safe via the
    /// safepoint protocol.
    pub(crate) needs_auto_collect: Arc<std::sync::atomic::AtomicBool>,
    /// **add-z42-launcher **: command-line arguments passed to
    /// the running z42 program — everything after the `--` separator on the
    /// `z42vm` command line. Exposed to z42 code via
    /// `Std.IO.Environment.GetCommandLineArgs()` (builtin `__env_args`).
    /// Shared on `VmCore` so worker threads see the same argv as main.
    /// Set once at startup by `VmContext::set_program_args`; empty by default
    /// (bare `z42vm file entry` with no `--` → no program args).
    pub(crate) program_args:       Mutex<Vec<String>>,
    /// **add-sync-primitives-rwlock **: `Std.Threading.RwLock<T>`
    /// slot table. Multiple shared (read) holders OR a single exclusive
    /// (write) holder. Same Arc-+-thread-local-guard parking pattern as
    /// Mutex, with an additional Read/Write variant tracked per slot so
    /// release picks the correct unlock path.
    /// (**M2**: slot-id counter embedded in the registry.)
    /// **add-z42-compression **: stdlib native extension builtins
    /// (e.g. `__deflate_compress` from libz42_compression). Filled when a
    /// library is first needed: a lookup miss loads the library that provides
    /// the name (`crate::native::ext::ensure_lib_for`), which registers its
    /// `(name, fn_ptr)` pairs. Lookup parallels static `BUILTINS[]`;
    /// see `corelib::ext_builtin_id_of` for the resolver fallback.
    /// Only present when `native-interop` feature is enabled (gated alongside
    /// the `native` module in lib.rs).
    #[cfg(feature = "native-interop")]
    pub(crate) ext_builtins:       Mutex<crate::native::ext::ExtBuiltinTable>,
    /// **add-z42-io-filestream **: live `Std.IO.FileStream`
    /// handles keyed by monotonic slot id. `__file_open` inserts;
    /// `__file_close` removes (or marks slot dead). Pattern mirrors
    /// `processes` / `compressors` slot tables（`mutexes`/`channels`/`rwlocks` 随旧同步原语退役一并删除）。
    /// (**M2**: slot-id counter embedded in the registry.)
    pub(crate) file_handles:       ResourceRegistry<crate::corelib::fs::FileHandleSlot>,

    // ── add-z42-net K1  ───────────────────────────────────────
    /// live `Std.Net.Sockets.TcpClient` streams keyed by monotonic u64 slot
    /// id. `__tcp_connect` / `__tcp_accept` insert; `__tcp_close`
    /// removes and shuts the stream down. Read / write clone the `Arc` instead of
    /// taking the stream out, so the socket is full duplex; the fd closes when the
    /// last `Arc` drops. Present with the `net` feature only.
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(feature = "net")]
    pub(crate) tcp_sockets:          ResourceRegistry<std::sync::Arc<std::net::TcpStream>>,
    /// live `Std.Net.Sockets.TcpListener` instances keyed by monotonic u64 slot id.
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(feature = "net")]
    pub(crate) tcp_listeners:        ResourceRegistry<crate::corelib::network::ListenerSlot>,

    // ── add-z42-net-tls  ──────────────────────────────────────
    /// live rustls client TLS streams (TCP + handshake state) keyed by
    /// monotonic u64 slot id. `__tls_connect` inserts; `__tls_close`
    /// removes and shuts the socket down; the fd closes when the last `Arc`
    /// drops. Slot space is independent from `tcp_sockets`. Present with the
    /// `tls` feature only (without it the TLS builtins throw `NetUnsupportedException`).
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(feature = "tls")]
    pub(crate) tls_sockets:
        ResourceRegistry<std::sync::Arc<crate::corelib::tls::TlsSlot>>,

    // ── add-z42-net-udp (K2) ───────────────────────────────────
    /// live `Std.Net.Sockets.UdpClient` sockets keyed by monotonic u64 slot id.
    /// `__udp_bind` inserts; `__udp_close` removes. Send / recv clone the
    /// `Arc`; the fd closes when the last `Arc` drops. Present with the `net` feature only.
    /// (**M2**: slot-id counter embedded in the registry.)
    #[cfg(feature = "net")]
    pub(crate) udp_sockets:          ResourceRegistry<std::sync::Arc<std::net::UdpSocket>>,

    /// **add-runtime-counters **: atomic observation-only
    /// counters for JIT compiles / builtin calls / native calls /
    /// exception traffic. Surfaced via `--print-stats-on-exit` and
    /// (future) scripted `Std.Diagnostics.RuntimeStats.Snapshot()`.
    /// `Arc` wrapper enables cheap cloning into Phase 2 increment sites
    /// that don't already have `&VmCore` access. docs/review.md Part 4 D6.
    ///
    /// runtime-audit P0-15: threads count on their own `VmContext::counters`;
    /// this one only accumulates the counts of contexts already dropped.
    /// Read the total with `VmContext::counters_snapshot`.
    pub counters: Arc<crate::counters::RuntimeCounters>,

    /// **add-runtime-observer **: push-based event stream
    /// registry. Symmetric to existing `GcObserver` (which stays in the
    /// heap) but for non-GC events (`ModuleLoaded`, future JIT compile /
    /// exception / native call). Embedders register via
    /// [`VmContext::add_runtime_observer`]; subsystems fire via
    /// [`VmContext::fire_runtime_event`]. docs/review.md Part 4 D3.
    pub runtime_observers: crate::observer::RuntimeObserverRegistry,

    /// **add-concurrency-probes (script-profiling P1b)**: distribution
    /// of how long mutators spend PARKED at a GC safepoint (STW stall as seen by
    /// each stopped thread). Recorded in `gc::safepoint::park_until_idle`, which
    /// only runs during an actual GC pause — so this costs the hot safepoint-check
    /// path nothing (always-on). Reuses `PauseHistogram` (same shape as GC pauses).
    pub(crate) park_histogram: Mutex<crate::gc::types::PauseHistogram>,

    /// **add-concurrency-probes **: user-lock (`Std.Threading.Mutex` /
    /// `RwLock`) contention counters. Written ONLY when the VM is built with the
    /// `profile-contention` cargo feature (the probe in `corelib::sync_contention` is
    /// `#[cfg]`-gated); the default build never touches them → they stay 0.
    /// `lock_contentions` = acquires that found the lock already held (`try_lock`
    /// failed); `lock_wait_us` = cumulative µs blocked on those contended acquires.
    pub(crate) lock_contentions: std::sync::atomic::AtomicU64,
    pub(crate) lock_wait_us:     std::sync::atomic::AtomicU64,

    /// **add-sampling-profiler (script-profiling P2)**: safepoint
    /// sampling profiler. `Sampler::disabled()` unless `Z42_SAMPLE_HZ` is set,
    /// in which case a background timer thread flags samples that mutators
    /// snapshot at their next safepoint (`gc::safepoint::check_safepoint_slow`
    /// Idle tail). Default-off is zero-cost: no thread, one `enabled()` atomic
    /// load on the already-throttled slow path. Produces a folded flamegraph
    /// (+ optional perfetto sample-trace when `Z42_TRACE_OUT` is set).
    pub(crate) sampler: crate::gc::Sampler,

    /// The VM's JIT: one per VM, shared by every thread (slots table, the
    /// compiler and its code, OSR entries, tier thresholds — see
    /// `jit::runtime`). Set once by the first `jit::run` on this VM and kept
    /// until the core drops, so compiled code stays valid for every thread
    /// still holding the core. Unset ⇒ no thread of this VM runs native code
    /// (interp mode).
    #[cfg(feature = "jit")]
    pub(crate) jit: std::sync::OnceLock<crate::jit::runtime::JitRuntime>,
}

/// The part of a [`VmContext`] that JIT-compiled code reads at fixed offsets.
///
/// Every compiled function and JIT helper receives the **running thread's own**
/// `VmContext` as its context parameter, so these are per-thread values read
/// without a helper call:
///
/// - `stack_limit` — the prologue's stack-overflow bound
///   ([`VM_CONTEXT_STACK_LIMIT_OFFSET`]);
/// - `cctors` — the VM's cctor registry, whose package-init gate counters the
///   inline string intrinsics read ([`VM_CONTEXT_CCTORS_OFFSET`]).
///
/// It also caches the VM's JIT runtime pointer (`jit`), so helpers and the
/// interpreter's mixed-mode diverts reach the shared JIT state with one load.
///
/// The safepoint throttle counter is the third JIT-read field; it stays on
/// [`VmContext::safepoint_skip`] (the interpreter decrements it too) and is
/// addressed by [`VM_CONTEXT_SAFEPOINT_SKIP_OFFSET`].
#[repr(C)]
pub(crate) struct JitHead {
    /// Lowest stack address a JIT function may start at: `stack_guard::limit()`
    /// of the thread driving this context, stored at its bottom frame push
    /// (`push_frame` → `EngineGuards::install`), i.e. before any compiled code
    /// runs on it. `0` until then (`sp < 0` never holds: no check). Written and
    /// read by the owner thread only; atomic only to keep the context `Sync`.
    pub(crate) stack_limit: std::sync::atomic::AtomicUsize,
    /// `Arc::as_ptr(&core.cctors)` — stable for the core's life, set at
    /// construction.
    pub(crate) cctors: *const super::cctor::CctorRegistry,
    /// `core.jit` once installed (null until then, and always in interp mode):
    /// taken at construction, stored by `jit::runtime::install` for the
    /// installing context, and refreshed at the bottom frame push for a context
    /// that existed before the install. Points into the core this context keeps
    /// alive. Written and read by the owner thread only.
    #[cfg(feature = "jit")]
    pub(crate) jit: std::sync::atomic::AtomicPtr<crate::jit::runtime::JitRuntime>,
}

// SAFETY: `cctors` points into the `Arc<CctorRegistry>` the context's own core
// keeps alive; the registry is `Sync` and only read through the pointer.
unsafe impl Send for JitHead {}
unsafe impl Sync for JitHead {}

impl JitHead {
    pub(crate) fn new(core: &VmCore) -> Self {
        JitHead {
            stack_limit: std::sync::atomic::AtomicUsize::new(0),
            cctors: Arc::as_ptr(&core.cctors),
            #[cfg(feature = "jit")]
            jit: std::sync::atomic::AtomicPtr::new(
                core.jit.get().map_or(std::ptr::null_mut(), |rt| rt as *const _ as *mut _)),
        }
    }
}

/// Runtime-mutable state shared across one VM instance's interp + JIT paths.
///
/// All `RefCell` fields take `&self` so JIT extern-C call sites (which reach
/// the receiver through `*mut VmContext`) can avoid producing `&mut`. The
/// `heap` field is `Box<dyn MagrGC>` without `RefCell` because it is set once
/// in `new()` and never replaced; trait methods take `&self` and the
/// implementation handles its own interior mutability.
///
/// **Phase 3d.1 **: `static_fields` / `pending_exception` /
/// `lazy_loader` 改用 `Rc<RefCell<...>>` 包装，让 `ArcMagrGC` 的 external
/// root scanner 闭包能 clone Rc 共享访问，从而 mark_reachable_set 把这些
/// 字段持有的 Value 也纳入 GC roots（修复 cycle collector 漏扫 static_fields
/// 导致误清的 bug）。
pub struct VmContext {
    /// Shared-across-threads state (add-multithreading-foundation).
    /// See [`VmCore`] for the field list and per-phase migration table.
    /// Static fields, type registries, native interop registry etc. accessed
    /// through `self.core.<field>.lock()` (or `.read()` for RwLock variants).
    pub(crate) core: Arc<VmCore>,
    /// **Phase 3 revision of Decision 2** : per-thread state is
    /// Arc<Mutex<>> instead of Rc<RefCell<>> because the GC scanner closure
    /// must be `Send + Sync` (MagrGC trait now requires it), which forces
    /// every closure capture to also be Send + Sync. Single-thread overhead
    /// is small (~few ns per lock vs RefCell borrow), worth the architectural
    /// consistency. Tracked in design.md Decision 2 amendment.
    pub(crate) pending_exception: Arc<Mutex<Option<Value>>>,
    /// add-method-invoke-non-generic — carries a z42 exception VALUE out of a
    /// callback builtin (reflection `MethodInfo.Invoke`) so the ORIGINAL thrown
    /// exception (preserving its type) propagates to z42 `try/catch`, instead of
    /// being wrapped into a generic `Std.Exception`. Set right before the builtin
    /// returns `Err`; `exec_call::builtin` takes it in its error handler.
    ///
    /// Per thread (a builtin running on one thread must not hand its exception
    /// to a builtin failing on another) and a GC root (the value can sit here
    /// across an allocation before the caller takes it).
    pub(crate) pending_thrown:    Mutex<Option<Value>>,
    /// runtime-audit P0-15: this thread's runtime counters (its own cache line,
    /// no cross-thread contention). Totals: `VmContext::counters_snapshot`.
    pub(crate) counters: crate::counters::RuntimeCounters,
    /// Active script frames, one [`crate::exception::VmFrame`] per interp /
    /// JIT activation: GC roots (raw `regs` pointer) and
    /// stack-trace rows (`func` + `pc`) in one entry.
    ///
    /// Owner-thread only, no lock — another thread reads it only while this
    /// context is parked (GC root scanning, `scan_frames_parked`); the crash
    /// signal handler reads just its published depth. See
    /// [`super::frame_stack`].
    ///
    /// Raw ptrs valid only while the owning Rust frame
    /// (`interp::Frame` / `JitFrame`) is alive — `FrameGuard` RAII for
    /// interp and `jit::invoke::call_native` for JIT pop before the owner
    /// returns.
    pub(crate) call_stack:        super::frame_stack::FrameStack,
    /// Free-list of register files for interp `Frame`s and `JitFrame`s.
    /// Owner-thread only, see [`super::reg_pool`].
    pub(crate) reg_pool:          super::reg_pool::RegPool,
    /// VM / heap thread-locals installed at the bottom frame's push and
    /// restored at the last pop. Owner-thread only, see [`super::engine_guards`].
    pub(crate) engine_guards:     super::engine_guards::EngineGuards,
    /// add-escape-analysis-stack-alloc: per-thread arena holding escape-analysis
    /// stack-allocated objects/arrays (`Value::StackObject`/`StackArray` index it).
    /// LIFO-truncated by `pop_frame` to each frame's stamped base. Scanned as GC
    /// roots at safepoint (its slots may hold heap `ArrRef`s). Mutex: owner-thread
    /// accesses uncontended; GC scanner is the only cross-thread reader.
    pub(crate) stack_arena:       Arc<Mutex<crate::interp::stack_alloc::StackArena>>,
    /// add-struct-value-semantics: per-thread byte arena holding value-struct blobs
    /// (`Value::StructRef` indexes it). Same lifetime model as `stack_arena`
    /// (LIFO-truncated by `pop_frame`, GC-scanned at safepoint).
    pub(crate) struct_arena:      Arc<Mutex<crate::interp::struct_arena::StructArena>>,
    /// make-value-copy: per-thread arena holding the payloads of the three transient,
    /// frame-scoped `Value` variants (`Ref`/`PinnedView`/`StructRefHeap`,
    /// which now carry only an 8B `{idx,frame_id}` handle so `Value` is `Copy`). Same
    /// lifetime model as `stack_arena` (LIFO-truncated by `pop_frame`, GC-scanned at safepoint).
    pub(crate) transient_arena:   Arc<Mutex<crate::interp::transient_arena::TransientArena>>,
    /// This thread's TLAB claims while it is parked in a blocking native call
    /// (`gc::safepoint::native_park_incr`); a collector retires them if it runs meanwhile.
    pub(crate) parked_tlab:       crate::gc::tlab::ParkedTlabSlot,
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
    /// Frame-id source: a frame takes an id the first time it allocates in one of
    /// the arenas above (interp `Frame::frame_id`, JIT `struct_ops::frame_id_of`);
    /// arena slots are keyed by it for stale-handle diagnostics. Written only by
    /// the owner thread (load + store, no RMW); an atomic only to keep the
    /// context `Sync`.
    pub(crate) next_frame_id:     std::sync::atomic::AtomicU32,
    /// What compiled code reads off this context at fixed offsets (stack limit,
    /// cctor gate). See [`JitHead`].
    pub(crate) jit_head:          JitHead,
    /// **optimize-subclass-check**: memoizes the type-test verdict, keyed by
    /// `(receiver TypeId, target key)` packed into a `u64` (`dispatch::isa_key`). Backs the
    /// direct-mapped `isa_cache` (its misses and collisions) and the name-only callers
    /// (reflection, debug assertions). The relationship is a global, monotonic fact (a loaded
    /// type's bases/interfaces never change; lazy-load only ADDS types) and ids are never
    /// reused, so the result is cacheable. Per-context (not shared) → no lock contention under
    /// parallel `--jobs` compile. Cleared on explicit module (re)load (REPL redefinition).
    pub(crate) subclass_memo:     Mutex<FxHashMap<u64, bool>>,
    /// perf-vm-isa-cache: direct-mapped front cache for `is` / `as` / typed `catch` in front of
    /// `subclass_memo`, keyed by `(receiver TypeId, target key)` — a hit is one relaxed load,
    /// no lock, no hashing. See `isa_cache.rs` for the key contract; cleared with the memo.
    pub(crate) isa_cache:         super::isa_cache::IsaCache,
    /// parallel-parse-and-cache : per-context front caches for
    /// `try_lookup_type` / `try_lookup_function` hits. The shared `lazy_loader`
    /// RwLock was the top contention point under parallel `--jobs` compile
    /// (native samples at jobs=8: ~3000 in `lock_shared_slow` under
    /// `try_lookup_type`) — z42c's own types live in the lazy loader, so every
    /// `obj_new` took the shared read lock and all threads bounced one cache line.
    /// Only **complete** answers are cached (base chain merged): the registry only
    /// mutates a TypeDesc during inheritance fixup, which `probe_type` never returns,
    /// and the function table is first-loaded-wins. Cleared with the memo on
    /// explicit module (re)load (REPL redefinition).
    pub(crate) type_lookup_cache: Mutex<FxHashMap<Box<str>, std::sync::Arc<crate::metadata::TypeDesc>>>,
    pub(crate) fn_lookup_cache:   Mutex<FxHashMap<Box<str>, std::sync::Arc<crate::metadata::Function>>>,
    /// **add-vmcontext-registry **: marks `VmContext: !Unpin`,
    /// so callers cannot `mem::swap` / move out of the `Pin<Box<VmContext>>`
    /// returned by [`new`]. Required so the raw pointer registered in
    /// [`VmCore::vm_contexts`] stays valid for the entire lifetime.
    pub(crate) _pin: PhantomPinned,
    // heap moved to VmCore (Phase 2.2)

    // native_types / native_libs / pinned_owned_buffers moved to VmCore (Phase 1.7-1.9)

    // processes moved to VmCore (Phase 2.1); **M2**: the process slot-id
    // counter moved with it, into `VmCore::processes` (ResourceRegistry) —
    // it was the last per-thread counter for a shared table.
    /// **add-gc-safepoint-counter-throttling **: per-thread
    /// throttle counter. `check_safepoint`'s fast path decrements; only
    /// when it reaches 0 does the slow path probe `gc_phase` and drain
    /// `needs_auto_collect`. Initial value comes from
    /// [`crate::gc::safepoint::throttle_n`] (default 1024, env-overridable).
    pub(crate) safepoint_skip:    std::sync::atomic::AtomicU32,
}

/// Byte offset of [`VmContext::safepoint_skip`] within `VmContext`.
///
/// **inline-jit-safepoint-check **: the JIT emits a native
/// load/store of the throttle counter (`jit::translate::emit_safepoint_check`)
/// instead of a helper call. `offset_of!` is compile-time and independent of
/// field reordering / `#[repr(Rust)]`, so it always yields the actual offset.
pub const VM_CONTEXT_SAFEPOINT_SKIP_OFFSET: usize =
    std::mem::offset_of!(VmContext, safepoint_skip);

/// Byte offset of [`JitHead::stack_limit`] within `VmContext` (loaded by every
/// compiled prologue).
pub const VM_CONTEXT_STACK_LIMIT_OFFSET: usize =
    std::mem::offset_of!(VmContext, jit_head.stack_limit);

/// Byte offset of [`JitHead::cctors`] within `VmContext` (loaded by the inline
/// string intrinsics' package-init gate).
pub const VM_CONTEXT_CCTORS_OFFSET: usize =
    std::mem::offset_of!(VmContext, jit_head.cctors);

// `Default` removed: `new()` now returns `Pin<Box<VmContext>>`, which
// cannot satisfy `Default::default() -> Self`. Test helpers that
// previously used `VmContext::default()` should call `VmContext::new()`
// directly and accept `Pin<Box<VmContext>>` (deref still works for
// method calls).
