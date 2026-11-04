use super::*;

/// State shared by all contexts of one VM, owned through `Arc<VmCore>`.
/// Subsystems own their synchronization and teardown; contexts retain thread-local
/// frames, pending exceptions and allocation arenas. Concrete core-library and
/// interop resources live in frozen extension state.
pub struct VmCore {
    pub(crate) threads: crate::vm::threads::ThreadState,
    /// Static fields: one stable-address cell per `StaticFieldId` (read / written without a
    /// lock), plus the FQN → id registration (lazy, first use; cross-zpkg fields can be met in
    /// any order). See `vm_context::static_table`.
    pub(crate) statics: super::static_table::StaticTable,
    /// add-static-constructors：有静态构造器的类型的按类型初始化状态。**只登记有 cctor
    /// 的类型**；热路径通过其无锁 `pending` 计数短路（`== 0` ⇒ 全程序无待初始化 cctor
    /// ⇒ 屏障免费）。详见 `vm_context/cctor.rs` 模块注释。
    /// fix-crosspkg-static-call-cctor：`Arc` 是为了让惰性加载器也持有一份——它在类型入表的那一刻
    /// 登记 cctor（见 `LazyLoader::insert_type`）。
    pub(crate) cctors: Arc<super::cctor::CctorRegistry>,
    /// defer-class-initialization: 懒执行的 `__static_init__` 抛出/出错时记录首个失败，
    /// 由 `Vm::run` 在入口返回后转成运行错误（对齐变更前 boot 期 `bail!` 的响度）。
    pub(crate) static_init_error:  Mutex<Option<String>>,
    /// **GC subsystem**. Moved here in Phase 2.2 so it can be shared across
    /// threads (single global heap). Backing today is `Heap`; Phase 3
    /// swaps to Arc + Send + Sync. Stored as `Box<Heap>` (no inner
    /// lock) because all `Heap` methods take `&self` and the impl handles
    /// its own interior mutability.
    ///
    /// **Scanner cycle avoidance**: the external root scanner closure
    /// captures `Weak<VmCore>` (not `Arc<VmCore>`) for static_fields access
    /// — otherwise `VmCore` → heap → scanner → Arc<VmCore> forms a cycle
    /// and the core never drops. Per-thread roots (call_stack /
    /// pending_exception) stay captured via `Rc<RefCell>`
    /// clones from the unique VmContext.
    pub(crate) heap:               Box<Heap>,
    pub(crate) runtime: crate::vm::runtime::RuntimeState,
    pub(crate) loader: crate::vm::loader::LoaderState,
    /// **add-z42-launcher **: command-line arguments passed to
    /// the running z42 program — everything after the `--` separator on the
    /// `z42vm` command line. Exposed to z42 code via
    /// `Std.IO.Environment.GetCommandLineArgs()` (builtin `__env_args`).
    /// Shared on `VmCore` so worker threads see the same argv as main.
    /// Set once at startup by `VmContext::set_program_args`; empty by default
    /// (bare `z42vm file entry` with no `--` → no program args).
    pub(crate) program_args:       Mutex<Vec<String>>,

    /// Observation state shared by this VM; per-thread counters stay on each context.
    pub diagnostics: crate::vm::diagnostics::DiagnosticsState,

    /// Per-VM engine registrations and backend-owned tier/code lifetime.
    pub(crate) engines: crate::vm::exec::engine::EngineRegistry,
    /// Released after the heap and engines so extension-owned resources outlive them.
    pub(crate) ext_state: crate::vm::ext::ExtensionState,
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
/// It also caches the VM's neutral tier header pointer (`jit`). Backend state
/// remains owned by the registered tier; compiled field layout stays fixed.
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
    pub(crate) fn new(core: &VmCore) -> Self {
        JitHead {
            stack_limit: std::sync::atomic::AtomicUsize::new(0),
            cctors: Arc::as_ptr(&core.cctors),
            #[cfg(feature = "jit")]
            jit: std::sync::atomic::AtomicPtr::new(
                core.engines.tier().map_or(std::ptr::null_mut(), |rt| rt as *const _ as *mut _)),
        }
    }
}

/// Runtime-mutable state shared across one VM instance's interp + JIT paths.
///
/// All `RefCell` fields take `&self` so JIT extern-C call sites (which reach
/// the receiver through `*mut VmContext`) can avoid producing `&mut`. The
/// `heap` field is `Box<Heap>` without `RefCell` because it is set once
/// in `new()` and never replaced; trait methods take `&self` and the
/// implementation handles its own interior mutability.
///
/// **Phase 3d.1 **: `static_fields` / `pending_exception` /
/// `lazy_loader` 改用 `Rc<RefCell<...>>` 包装，让 `Heap` 的 external
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
    /// must be `Send + Sync` (Heap trait now requires it), which forces
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
    /// Active script frames, one [`crate::vm::exec::call::frames::VmFrame`] per interp /
    /// JIT activation: GC roots (raw `regs` pointer) and
    /// stack-trace rows (`func` + `pc`) in one entry.
    ///
    /// Owner-thread only, no lock — another thread reads it only while this
    /// context is parked (GC root scanning, `scan_frames_parked`); the crash
    /// signal handler reads just its published depth. See
    /// [`crate::vm::exec::call::frame_stack`].
    ///
    /// Raw ptrs valid only while the owning Rust frame
    /// (`interp::Frame` / `JitFrame`) is alive — `FrameGuard` RAII for
    /// interp and `jit::invoke::enter_compiled` for JIT pop before the owner
    /// returns.
    pub(crate) call_stack:        crate::vm::exec::call::frame_stack::FrameStack,
    /// Free-list of register files for interp `Frame`s and `JitFrame`s.
    /// Owner-thread only, see [`crate::vm::exec::call::reg_pool`].
    pub(crate) reg_pool:          crate::vm::exec::call::reg_pool::RegPool,
    /// VM / heap thread-locals installed at the bottom frame's push and
    /// restored at the last pop. Owner-thread only, see [`super::engine_guards`].
    pub(crate) engine_guards:     super::engine_guards::EngineGuards,
    /// add-escape-analysis-stack-alloc: per-thread arena holding escape-analysis
    /// stack-allocated objects/arrays (`Value::StackObject`/`StackArray` index it).
    /// LIFO-truncated by `pop_frame` to each frame's stamped base. Scanned as GC
    /// roots at safepoint (its slots may hold heap `ArrRef`s). Mutex: owner-thread
    /// accesses uncontended; GC scanner is the only cross-thread reader.
    pub(crate) stack_arena:       Arc<Mutex<crate::vm::exec::call::stack_alloc::StackArena>>,
    /// add-struct-value-semantics: per-thread byte arena holding value-struct blobs
    /// (`Value::StructRef` indexes it). Same lifetime model as `stack_arena`
    /// (LIFO-truncated by `pop_frame`, GC-scanned at safepoint).
    pub(crate) struct_arena:      Arc<Mutex<crate::vm::exec::call::struct_arena::StructArena>>,
    /// make-value-copy: per-thread arena holding the payloads of the three transient,
    /// frame-scoped `Value` variants (`Ref`/`PinnedView`/`StructRefHeap`,
    /// which now carry only an 8B `{idx,frame_id}` handle so `Value` is `Copy`). Same
    /// lifetime model as `stack_arena` (LIFO-truncated by `pop_frame`, GC-scanned at safepoint).
    pub(crate) transient_arena:   Arc<Mutex<crate::vm::exec::call::transient_arena::TransientArena>>,
    /// This thread's TLAB claims while it is parked in a blocking native call
    /// (`vm::threads::safepoint::native_park_incr`); a collector retires them if it runs meanwhile.
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
    pub(crate) isa_cache:         crate::vm::runtime::dispatch::isa_cache::IsaCache,
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
    pub(crate) type_lookup_cache: Mutex<FxHashMap<Box<str>, std::sync::Arc<crate::object::TypeDesc>>>,
    pub(crate) fn_lookup_cache:   Mutex<FxHashMap<Box<str>, std::sync::Arc<crate::vm::runtime::Function>>>,
    /// **add-vmcontext-registry **: marks `VmContext: !Unpin`,
    /// so callers cannot `mem::swap` / move out of the `Pin<Box<VmContext>>`
    /// returned by [`new`]. Required so the raw pointer registered in
    /// [`VmCore::vm_contexts`] stays valid for the entire lifetime.
    pub(crate) _pin: PhantomPinned,
    // heap moved to VmCore (Phase 2.2)

    /// **add-gc-safepoint-counter-throttling **: per-thread
    /// throttle counter. `check_safepoint`'s fast path decrements; only
    /// when it reaches 0 does the slow path probe `gc_phase` and drain
    /// `needs_auto_collect`. Initial value comes from
    /// [`crate::vm::threads::safepoint::throttle_n`] (default 1024, env-overridable).
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
