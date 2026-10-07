/// JIT frame and module context types.
///
/// `JitFrame` is the runtime stack frame passed (as a raw pointer) to every
/// JIT-compiled function.  `JitModuleCtx` is the read-only module-level context
/// that is shared across all calls within a single module execution.

use crate::metadata::seg_vec::SparseSegTable;
use crate::metadata::tokens::FnId;
use crate::metadata::{Function, Value};
use crate::vm_context::VmContext;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

// ── JitFrame ─────────────────────────────────────────────────────────────────

/// Runtime stack frame for a JIT-compiled function.
/// Pure register machine — all variables use integer register IDs, no named slots.
///
/// `#[repr(C)]` because compiled code reads and writes two fields at fixed
/// offsets: the prologue loads `regs_ptr` ([`JIT_FRAME_REGS_PTR_OFFSET`], pinned
/// to 0) and `Ret %r` stores into `ret` / `has_ret` ([`JIT_FRAME_RET_OFFSET`],
/// [`JIT_FRAME_HAS_RET_OFFSET`]). Neither goes through a helper call.
#[repr(C)]
pub struct JitFrame {
    /// `regs.as_mut_ptr()`, set by every constructor once `regs` is sized. The
    /// register file is never resized while the frame exists (sized to the
    /// function's `max_reg + 1` up front; JIT code and helpers only index it —
    /// and the one cross-frame resize, `interp::frame::store_thru_ref`, can't
    /// reach a JIT frame: `LoadLocalAddr` is JIT-unsupported, and a slot index
    /// is always `<= max_reg` anyway), so the buffer never moves and this stays
    /// valid until [`JitFrame::recycle`] hands `regs` back to the pool. Moving the
    /// `JitFrame` itself doesn't move the heap buffer.
    pub regs_ptr: *mut Value,
    /// Return value, written by compiled code at `Ret %r` (a 16-byte copy of the
    /// register slot — `Value` is `Copy`, no refcount / barrier). Meaningful only
    /// when `has_ret != 0`. Not a GC root: nothing reaches a safepoint between
    /// the store and `call_native` taking it out.
    pub ret: Value,
    /// `1` once `Ret %r` stored `ret`; stays `0` for a void `Ret` — the
    /// `Returned(None)` vs `Returned(Some(Null))` distinction.
    pub has_ret: u8,
    /// Register file indexed by SSA register number.
    pub regs: Vec<Value>,
    /// 2026-05-02 impl-closure-l3-escape-stack: frame-local arena for
    /// non-escaping closure envs. `Value::StackClosure { env_idx }` indexes
    /// here. Released as part of `JitFrame::recycle` (envs hold normal Drop
    /// semantics — GcRef contents inside env Vec follow their own RC chains).
    pub env_arena: Vec<Vec<Value>>,
    /// This activation's id for arena slots it allocates (struct values, stack
    /// closures), `0` until taken: `struct_ops::frame_id_of` takes one from
    /// `VmContext::next_frame_id()` on first use, and an OSR hand-off inherits
    /// the interp frame's (taken or `0`). The id keys the arena staleness guard
    /// exactly like an interp frame's; the arena's LIFO truncation is driven by
    /// the `VmFrame` bases (push/pop_frame), not by the id.
    pub frame_id: u32,
}

impl JitFrame {
    /// Allocate a new frame with `max_reg + 1` register slots from `vm`'s register
    /// pool (all `Null`). The first `args.len()` registers are initialised with
    /// the call arguments.
    pub fn new(vm: &VmContext, max_reg: usize, args: &[Value]) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        for (i, v) in args.iter().enumerate() {
            if i < size {
                regs[i] = v.clone();
            }
        }
        Self::with_regs(regs)
    }

    /// add-osr-loop-tiering: build a JitFrame from an interpreter frame's live
    /// register file at an OSR hand-off. Interp and JIT share the exact same
    /// register model (`Vec<Value>` indexed by IR reg number), so this is a straight
    /// copy of the live state — the native OSR entry resumes the hot loop reading
    /// these values back through `regs_base`. Sized to the JIT function's
    /// `max_reg + 1`; interp slots beyond that are dropped (same function → same
    /// max_reg), missing ones stay `Null`.
    pub fn from_interp_regs(vm: &VmContext, interp_regs: &[Value], max_reg: usize) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        for (i, v) in interp_regs.iter().enumerate() {
            if i < size { regs[i] = v.clone(); }
        }
        Self::with_regs(regs)
    }

    /// Allocate a frame and fill its first registers directly from the caller's
    /// register file, indexed by `arg_indices`. Avoids the intermediate
    /// `Vec<Value>` collect + the resulting double-clone that `new(_, &args)`
    /// incurs on the hot `jit_call` path (perf: per-call malloc/free + one of
    /// two arg clones eliminated; reg Vec still pooled). Each argument is cloned
    /// exactly once (caller reg → callee reg).
    pub fn new_args_from(
        vm: &VmContext, max_reg: usize, caller_regs: &[Value], arg_indices: &[u32],
    ) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        for (i, &r) in arg_indices.iter().enumerate() {
            if i < size {
                regs[i] = caller_regs[r as usize].clone();
            }
        }
        Self::with_regs(regs)
    }

    /// Like `new_args_from`, but for a method call: register 0 is the receiver
    /// (`this`), and registers `1..` are the positional args read from the
    /// caller's register file via `arg_indices`. Avoids the
    /// `vec![obj]` + `append(extra_args)` two-Vec dance on the hot `jit_vcall`
    /// path. The receiver is moved in (already cloned by the caller).
    pub fn new_method_args_from(
        vm: &VmContext, max_reg: usize, receiver: Value, caller_regs: &[Value], arg_indices: &[u32],
    ) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        if size > 0 { regs[0] = receiver; }
        for (i, &r) in arg_indices.iter().enumerate() {
            let slot = i + 1;
            if slot < size {
                regs[slot] = caller_regs[r as usize].clone();
            }
        }
        Self::with_regs(regs)
    }

    /// Wrap an already-sized, already-filled register file. `regs` must not be
    /// resized afterwards (see [`JitFrame::regs_ptr`]).
    #[inline]
    fn with_regs(mut regs: Vec<Value>) -> Self {
        JitFrame {
            regs_ptr: regs.as_mut_ptr(), ret: Value::Null, has_ret: 0,
            regs, env_arena: Vec::new(), frame_id: 0,
        }
    }

    /// The value `Ret %r` stored, or `None` after a void `Ret`.
    #[inline]
    pub fn take_ret(&self) -> Option<Value> {
        if self.has_ret != 0 { Some(self.ret) } else { None }
    }

    /// Hand the register file back to `vm`'s pool once the frame is popped.
    /// `env_arena` just drops (closure envs are rare; not pooled).
    pub fn recycle(self, vm: &VmContext) {
        vm.reg_pool.give(self.regs);
    }
}

/// Byte offset of [`JitFrame::regs_ptr`] (loaded by every compiled prologue).
pub const JIT_FRAME_REGS_PTR_OFFSET: usize = std::mem::offset_of!(JitFrame, regs_ptr);
/// Byte offset of [`JitFrame::ret`] (stored by compiled `Ret %r`).
pub const JIT_FRAME_RET_OFFSET: usize = std::mem::offset_of!(JitFrame, ret);
/// Byte offset of [`JitFrame::has_ret`] (set to 1 by compiled `Ret %r`).
pub const JIT_FRAME_HAS_RET_OFFSET: usize = std::mem::offset_of!(JitFrame, has_ret);
const _: () = assert!(JIT_FRAME_REGS_PTR_OFFSET == 0, "regs_ptr must be JitFrame's first field");
const _: () = assert!(std::mem::size_of::<Value>() == 16 && JIT_FRAME_RET_OFFSET % 8 == 0,
    "Ret copies a Value as two aligned 8-byte words");

// ── FnEntry ──────────────────────────────────────────────────────────────────

/// A compiled native function entry inside the JIT module.
///
/// `Clone` (not `Copy`) because of the shared `owner_init` cell.
#[derive(Clone)]
pub struct FnEntry {
    /// Pointer to the native machine code of the function.
    pub ptr:     *const u8,
    /// Size of the register file needed by this function (`max_reg`).
    pub max_reg: usize,
    /// The function this code was compiled from — the `VmFrame` the call
    /// pushes points at it, stack traces derive name / file / line from it, and
    /// `jit_call`'s barriers read its name. Valid for the whole run: an entry
    /// function lives in the module, a lazily loaded one is kept alive by its
    /// `FuncTable` slot. Null for [`FnEntry::rejected`].
    pub func:    *const Function,
    /// fix-ctor-arity-skew: 可接受的**物理**实参数区间，编译时从 `&Function` 算好。
    /// `jit_obj_new` 的 native 分支只拿得到 `FnEntry`（跨包构造器正是惰性加载、
    /// 最容易 tier 到 native 的那批），没有它就得为每次构造再查一次函数元数据。
    pub arity:   crate::vm_context::symres::CallArity,
    /// The function's `owner_init` cell (shared, see `Function::owner_init`),
    /// so `jit_call`'s cctor barrier needs no name-based owner lookup.
    pub owner_init: crate::metadata::bytecode::OwnerInitCell,
}

// Raw pointer — the JITModule that owns the code lives alongside this entry.
unsafe impl Send for FnEntry {}
unsafe impl Sync for FnEntry {}

impl FnEntry {
    /// Negative-cache marker of the OSR entry cache: a null-`ptr` entry meaning
    /// "no OSR variant (untranslatable or compile failed) — keep interpreting".
    /// (Per-function slots keep their verdict in [`JitSlot`]'s state instead.)
    pub fn rejected() -> Self {
        FnEntry {
            ptr: std::ptr::null(), max_reg: 0, func: std::ptr::null(),
            // rejected 项永远不会被当作可调用体，区间取全放行。
            arity: crate::vm_context::symres::CallArity { min: 0, max: u16::MAX },
            owner_init: Default::default(),
        }
    }
    #[inline]
    pub fn is_rejected(&self) -> bool { self.ptr.is_null() }
}

// ── JitSlot ──────────────────────────────────────────────────────────────────

/// [`JitSlot::state`]: not decided yet (cold, or never reached the threshold).
const SLOT_UNTRIED: u8 = 0;
/// [`JitSlot::state`]: untranslatable or compile failed — stays on the interpreter.
/// Compilation is deterministic, so the verdict is final.
const SLOT_REJECTED: u8 = 1;

/// Per-function JIT state, one per JIT id (see [`JitModuleCtx::slots`]).
///
/// Lifecycle: `Untried` (counting calls) → `entry` filled (compiled) or
/// `state = Rejected`. Both outcomes are final; neither re-scans nor recompiles.
pub struct JitSlot {
    /// Compiled native entry. Set once, under the compiler lock.
    entry: OnceLock<FnEntry>,
    /// Tier-up counter: tiered resolves count calls while the slot is untried and
    /// compile at `jit_threshold`. Frozen once the slot is decided.
    count: AtomicU32,
    /// `SLOT_UNTRIED` / `SLOT_REJECTED` (negative cache). A racing reader may miss
    /// a fresh `Rejected` and redo the (deterministic) verdict — harmless.
    state: AtomicU8,
}

impl Default for JitSlot {
    fn default() -> Self {
        JitSlot { entry: OnceLock::new(), count: AtomicU32::new(0), state: AtomicU8::new(SLOT_UNTRIED) }
    }
}

// ── JitModuleCtx ─────────────────────────────────────────────────────────────

/// Module-level context threaded through every JIT call.
///
/// **JIT ids.** Every function the JIT can run is named by one `usize` id, which
/// is the function's `FnId` (`VmCore.funcs`): entry-module functions are
/// `0..module.functions.len()` (= their `module.functions` index), lazily loaded
/// package functions the ids after that. Interp `Call` tokens
/// (`ResolvedTokens.method_tokens`) hold the same ids, so `jit_call` bakes and
/// caches them directly. When `module` is not the `FuncTable`'s entry module
/// (unit tests running a bare `VmContext::new()`), only `module.functions`
/// indices are ids and lazily loaded functions have none — they run on the
/// interpreter, exactly as the interp token rule (`FuncTable::is_entry`).
pub struct JitModuleCtx {
    /// Per-function compile state indexed by JIT id. Lock-free read (a segment
    /// is allocated on first touch and never moves), so a compiled call is one
    /// `get` + one `OnceLock::get`. Slots live as long as the `JITModule` whose
    /// code they point at.
    pub slots:       SparseSegTable<JitSlot>,
    /// Back-pointer to the bytecode module for class descriptors, function
    /// bodies, `func_index`, etc.
    /// SAFETY: the Module must outlive this ctx.
    pub module:      *const crate::metadata::Module,
    /// Lazy per-function compiler (owns the cranelift `JITModule` + helper ids),
    /// Mutex-guarded so concurrent first-calls compile each function exactly
    /// once. SAFETY: the `Mutex<LazyCompiler>` is owned by the `JitModule` that
    /// outlives this ctx; never null once constructed.
    pub lazy:        *const Mutex<super::lazy::LazyCompiler>,
    /// Mutable VM state (static fields, pending exception, lazy loader).
    /// Set by `JitModule::run` for the duration of one entry-point invocation;
    /// reset to null on return. JIT helpers reach mutable VM state via this
    /// pointer.
    /// SAFETY: the VmContext must outlive `JitModule::run` and be unique
    /// (no concurrent JIT entry on the same JitModule).
    pub vm_ctx:      *mut crate::vm_context::VmContext,
    /// Tier-up threshold: compile a function on its `jit_threshold`-th tiered
    /// call (N=1 → compile-on-first-call). From the `jit-threshold` runtime knob
    /// (default 2; see `jit/mod.rs`), clamped ≥ 1.
    pub jit_threshold: u32,
    /// Compiled OSR entries keyed by `(JIT id, loop-header block K)`. Keyed by K
    /// too because a function with two loops can OSR at different headers — a K1
    /// entry must not be reused for a K2 hand-off. Populated at most once per key
    /// (OSR is rare); the `FnEntry` is cloned out (owned) so callers don't hold the
    /// lock across the native call. `rejected()` marks untranslatable / compile-failed.
    pub osr_entries: Mutex<HashMap<(usize, usize), FnEntry>>,
    /// OSR trigger threshold (loop back-edges in the interp).
    /// From `Z42_OSR_THRESHOLD`, clamped ≥ 1.
    pub osr_threshold: u32,
    /// runtime-audit P0-4: lowest stack address a JIT function may start at
    /// (`stack_guard::limit()` of the thread running `JitModule::run_fn`; JIT code
    /// runs on that thread only). Read by every function prologue. `0` = no check.
    pub stack_limit: usize,
}

/// Byte offset of [`JitModuleCtx::stack_limit`] (read by the inlined prologue check).
pub const JIT_MODULE_CTX_STACK_LIMIT_OFFSET: usize =
    std::mem::offset_of!(JitModuleCtx, stack_limit);

/// Byte offset of [`JitModuleCtx::vm_ctx`] within `JitModuleCtx`.
///
/// **inline-jit-safepoint-check (2026-08-01)**: the inlined safepoint check
/// loads the `*mut VmContext` from `ctx_val + this offset` before touching the
/// throttle counter. Compile-time via `offset_of!`, layout-reorder-safe.
pub const JIT_MODULE_CTX_VM_CTX_OFFSET: usize =
    std::mem::offset_of!(JitModuleCtx, vm_ctx);

impl JitModuleCtx {
    /// A ctx with no compiled functions and no running VM (`vm_ctx` null).
    pub fn new(
        module: *const crate::metadata::Module,
        lazy: *const Mutex<super::lazy::LazyCompiler>,
        jit_threshold: u32,
        osr_threshold: u32,
    ) -> Self {
        JitModuleCtx {
            slots: SparseSegTable::new(),
            module,
            lazy,
            vm_ctx: std::ptr::null_mut(),
            jit_threshold,
            osr_entries: Mutex::new(HashMap::new()),
            osr_threshold,
            stack_limit: 0,
        }
    }

    // ── ids ──────────────────────────────────────────────────────────────────

    /// The function JIT id `id` names (see the struct doc), or `None` for an id
    /// nothing is registered under. Lock-free.
    /// SAFETY: `module` valid; `vm_ctx` valid or null.
    pub unsafe fn fn_of(&self, id: usize) -> Option<&Function> {
        let module = &*self.module;
        if let Some(f) = module.functions.get(id) {
            return Some(f);
        }
        if self.vm_ctx.is_null() { return None; }
        let funcs = (*self.vm_ctx).funcs();
        if !funcs.is_entry(module) { return None; }
        funcs.get(FnId(u32::try_from(id).ok()?))
    }

    /// The JIT id of `f`, if it has one: an entry-module function by address, a
    /// lazily loaded one by its `FnId` (only the function the table holds under
    /// that id). Lock-free, no hashing.
    /// SAFETY: see [`Self::fn_of`].
    pub unsafe fn id_of_func(&self, f: &Function) -> Option<usize> {
        let fns = &(*self.module).functions;
        let base = fns.as_ptr();
        let p = f as *const Function;
        if p >= base && p < base.add(fns.len()) {
            return Some(p.offset_from(base) as usize);
        }
        let id = f.id.get()?.0 as usize;
        std::ptr::eq(self.fn_of(id)?, f).then_some(id)
    }

    /// Cold path: the JIT id a call by `name` resolves to — this module's
    /// `func_index` first, then the lazily loaded functions (which may load the
    /// defining package). `None` when the name resolves nowhere or to a function
    /// without a JIT id.
    /// SAFETY: see [`Self::fn_of`].
    pub unsafe fn id_by_name(&self, name: &str) -> Option<usize> {
        let module = &*self.module;
        if let Some(&i) = module.func_index.get(name) {
            return Some(i);
        }
        if self.vm_ctx.is_null() { return None; }
        let vm = &*self.vm_ctx;
        if !vm.funcs().is_entry(module) { return None; }
        if let Some(id) = vm.funcs().lazy_id(name) {
            return Some(id.0 as usize);
        }
        let f = vm.try_lookup_function(name)?;
        self.id_of_func(&f)
    }

    // ── resolve ──────────────────────────────────────────────────────────────

    /// Resolve JIT id `id` to its compiled `FnEntry`, compiling it on first demand
    /// (no tier threshold — the entry point and `jit_to_str`). `None` when the
    /// function is not JIT-translatable or compilation failed (cached as
    /// Rejected) — the caller falls back to the interpreter.
    ///
    /// The compiled hot path is lock-free (`slots.get` + `OnceLock::get`); only an
    /// actual compile takes the compiler mutex, with a double-check so racing
    /// threads compile each function exactly once.
    ///
    /// SAFETY: `module` and `lazy` must be valid — they are for the lifetime of
    /// a `JitModule::run` (set at construction; `lazy` never null).
    #[inline]
    pub unsafe fn resolve_fn_by_id(&self, id: usize) -> Option<&FnEntry> {
        self.resolve_fn_by_id_thr(id, 0)
    }

    /// Tiered variant: count this call and compile only at `jit_threshold`; below
    /// it return `None` so the caller runs the function on the interpreter (cold
    /// tier). Used by every call path that has an interpreter fallback (`jit_call`,
    /// vcall, closures, ctors, and the interpreter's own per-site native routing).
    #[inline]
    pub unsafe fn resolve_fn_by_id_tiered(&self, id: usize) -> Option<&FnEntry> {
        self.resolve_fn_by_id_thr(id, self.jit_threshold)
    }

    /// [`Self::resolve_fn_by_id`] with an explicit threshold (`0` = compile now).
    #[inline]
    pub unsafe fn resolve_fn_by_id_thr(&self, id: usize, thr: u32) -> Option<&FnEntry> {
        if let Some(e) = self.slots.get(id).and_then(|s| s.entry.get()) {
            return Some(e);
        }
        self.resolve_slot_slow(id, thr)
    }

    /// Side-effect-free "is this function ALREADY compiled?" for the interpreter's
    /// central divert (`try_native_exec`): never counts, never compiles. The divert
    /// only routes already-hot functions to native; counting belongs to the
    /// primary call sites (a cold callee's interp fallback must not count it twice).
    #[inline]
    pub fn peek_fn_by_id(&self, id: usize) -> Option<&FnEntry> {
        self.slots.get(id)?.entry.get()
    }

    /// The not-yet-compiled half of [`Self::resolve_fn_by_id_thr`]: Rejected →
    /// `None` at once; otherwise count (tiered), then decide once — untranslatable
    /// or compile-failed ⇒ Rejected, else compile under the compiler lock.
    #[inline(never)]
    unsafe fn resolve_slot_slow(&self, id: usize, thr: u32) -> Option<&FnEntry> {
        // The function first (lock-free): an id nothing is registered under never
        // allocates a slot segment.
        let func = self.fn_of(id)?;
        let slot = self.slots.get_or_init(id)?;
        if let Some(e) = slot.entry.get() { return Some(e); }
        if slot.state.load(Ordering::Relaxed) == SLOT_REJECTED { return None; }
        if thr > 0 {
            let n = slot.count.fetch_add(1, Ordering::Relaxed) + 1;
            if n < thr { return None; }
        }
        // fix-jit-first-compile-unresolved-builtin: populate the function's token
        // table before translating — at `jit_threshold == 1` a function compiles
        // on its FIRST call, before the interpreter ever resolved it, and the JIT's
        // builtin-resolution fallback (static `BUILTINS` only) would panic on a
        // native-ext builtin. Idempotent (OnceLock-gated); `module` is the module
        // the callee executes against (identity invariant).
        if !self.vm_ctx.is_null() && func.resolved.get().is_none() {
            crate::metadata::resolver::resolve_function_tokens(func, &*self.module, &*self.vm_ctx);
        }
        if super::translate::jit_unsupported_reason(func).is_some() {
            slot.state.store(SLOT_REJECTED, Ordering::Relaxed);
            return None;
        }
        let mut guard = match (*self.lazy).lock() { Ok(g) => g, Err(p) => p.into_inner() };
        if slot.entry.get().is_none() {
            if slot.state.load(Ordering::Relaxed) == SLOT_REJECTED { return None; }
            let t0 = std::time::Instant::now();
            match guard.compile_fn(func) {
                Ok(entry) => { let _ = slot.entry.set(entry); self.bump_compile_counters(t0); }
                // Compilation is deterministic → never re-attempt.
                Err(_) => { slot.state.store(SLOT_REJECTED, Ordering::Relaxed); return None; }
            }
        }
        drop(guard);
        slot.entry.get()
    }

    /// Test probe: `(call count, rejected?)` of JIT id `id`'s slot (`(0, false)`
    /// when it was never touched).
    #[cfg(test)]
    pub(crate) fn slot_probe(&self, id: usize) -> (u32, bool) {
        self.slots.get(id).map_or((0, false), |s| {
            (s.count.load(Ordering::Relaxed), s.state.load(Ordering::Relaxed) == SLOT_REJECTED)
        })
    }

    /// Counters reflect what was ACTUALLY compiled. `vm_ctx` is set for the
    /// duration of `JitModule::run`.
    fn bump_compile_counters(&self, t0: std::time::Instant) {
        if !self.vm_ctx.is_null() {
            let c = unsafe { (*self.vm_ctx).counters() };
            c.jit_methods_compiled.fetch_add(1, Ordering::Relaxed);
            c.jit_compile_us_total
                .fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
        }
    }

    /// Name-keyed resolution (compile now): `id_by_name` → `resolve_fn_by_id`.
    /// SAFETY: see [`Self::resolve_fn_by_id`].
    pub unsafe fn resolve_fn_by_name(&self, name: &str) -> Option<&FnEntry> {
        self.resolve_fn_by_id(self.id_by_name(name)?)
    }

    /// Name-keyed tiered resolution (closures and constructors, which still carry
    /// names). SAFETY: see [`Self::resolve_fn_by_id`].
    pub unsafe fn resolve_fn_by_name_tiered(&self, name: &str) -> Option<&FnEntry> {
        self.resolve_fn_by_id_tiered(self.id_by_name(name)?)
    }

    /// Resolve (compiling on first sight) the **OSR entry** of JIT id `id` at
    /// loop-header block `k` — entry-module and lazily loaded functions alike.
    /// Returns an OWNED `FnEntry` clone (so the caller doesn't hold the cache lock
    /// across the native call), or `None` if the function is untranslatable or the
    /// compile failed. Cached per `(id, k)` — a second hot activation of the same
    /// loop reuses it. OSR is a rare event, so a plain `Mutex<HashMap>` is fine.
    pub unsafe fn resolve_osr_entry(&self, id: usize, k: usize) -> Option<FnEntry> {
        {
            let map = match self.osr_entries.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            if let Some(e) = map.get(&(id, k)) {
                return if e.is_rejected() { None } else { Some(e.clone()) };
            }
        }
        // Not cached — compile the OSR variant (translatable check first).
        let func = self.fn_of(id)?;
        let compiled: FnEntry = if super::translate::jit_unsupported_reason(func).is_some() {
            FnEntry::rejected()
        } else {
            let mut guard = match (*self.lazy).lock() { Ok(g) => g, Err(p) => p.into_inner() };
            match guard.compile_fn_osr(func, k) {
                Ok(e) => {
                    if !self.vm_ctx.is_null() {
                        (*self.vm_ctx).counters().jit_methods_compiled
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    e
                }
                Err(_) => FnEntry::rejected(),
            }
        };
        let mut map = match self.osr_entries.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        let e = map.entry((id, k)).or_insert(compiled);
        if e.is_rejected() { None } else { Some(e.clone()) }
    }
}

// SAFETY: raw pointer — caller ensures Module outlives ctx.
unsafe impl Send for JitModuleCtx {}
unsafe impl Sync for JitModuleCtx {}
