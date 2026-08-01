/// JIT frame and module context types.
///
/// `JitFrame` is the runtime stack frame passed (as a raw pointer) to every
/// JIT-compiled function.  `JitModuleCtx` is the read-only module-level context
/// that is shared across all calls within a single module execution.

use crate::metadata::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

// ── JitFrame ─────────────────────────────────────────────────────────────────

/// Runtime stack frame for a JIT-compiled function.
/// Pure register machine — all variables use integer register IDs, no named slots.
pub struct JitFrame {
    /// Register file indexed by SSA register number.
    pub regs: Vec<Value>,
    /// Return value written by `jit_set_ret` before the function returns.
    pub ret:  Option<Value>,
    /// 2026-05-02 impl-closure-l3-escape-stack: frame-local arena for
    /// non-escaping closure envs. `Value::StackClosure { env_idx }` indexes
    /// here. Released as part of `JitFrame::recycle` (envs hold normal Drop
    /// semantics — GcRef contents inside env Vec follow their own RC chains).
    pub env_arena: Vec<Vec<Value>>,
}

impl JitFrame {
    /// Allocate a new frame with `max_reg + 1` register slots.
    /// The first `args.len()` registers are initialised with the call arguments.
    pub fn new(max_reg: usize, args: &[Value]) -> Self {
        let size = max_reg + 1;
        let mut regs = take_pooled_regs(size);
        for (i, v) in args.iter().enumerate() {
            if i < size {
                regs[i] = v.clone();
            }
        }
        JitFrame { regs, ret: None, env_arena: Vec::new() }
    }

    /// Allocate a frame and fill its first registers directly from the caller's
    /// register file, indexed by `arg_indices`. Avoids the intermediate
    /// `Vec<Value>` collect + the resulting double-clone that `new(_, &args)`
    /// incurs on the hot `jit_call` path (perf: per-call malloc/free + one of
    /// two arg clones eliminated; reg Vec still pooled). Each argument is cloned
    /// exactly once (caller reg → callee reg).
    pub fn new_args_from(max_reg: usize, caller_regs: &[Value], arg_indices: &[u32]) -> Self {
        let size = max_reg + 1;
        let mut regs = take_pooled_regs(size);
        for (i, &r) in arg_indices.iter().enumerate() {
            if i < size {
                regs[i] = caller_regs[r as usize].clone();
            }
        }
        JitFrame { regs, ret: None, env_arena: Vec::new() }
    }

    /// Like `new_args_from`, but for a method call: register 0 is the receiver
    /// (`this`), and registers `1..` are the positional args read from the
    /// caller's register file via `arg_indices`. Avoids the
    /// `vec![obj]` + `append(extra_args)` two-Vec dance on the hot `jit_vcall`
    /// path. The receiver is moved in (already cloned by the caller).
    pub fn new_method_args_from(
        max_reg: usize, receiver: Value, caller_regs: &[Value], arg_indices: &[u32],
    ) -> Self {
        let size = max_reg + 1;
        let mut regs = take_pooled_regs(size);
        if size > 0 { regs[0] = receiver; }
        for (i, &r) in arg_indices.iter().enumerate() {
            let slot = i + 1;
            if slot < size {
                regs[slot] = caller_regs[r as usize].clone();
            }
        }
        JitFrame { regs, ret: None, env_arena: Vec::new() }
    }

    /// Return the register Vec to the pool for reuse.
    pub fn recycle(self) {
        return_pooled_regs(self.regs);
        // env_arena drops naturally with `self`; no explicit recycle (pool
        // dimension is reg vector only — env arenas are infrequent and
        // small enough to skip pooling for v1).
    }
}

// ── Frame pool ──────────────────────────────────────────────────────────────

const POOL_MAX: usize = 32;

thread_local! {
    static FRAME_POOL: RefCell<Vec<Vec<Value>>> = const { RefCell::new(Vec::new()) };
}

/// Take a Vec<Value> from the pool (or allocate a new one), sized to `size`.
///
/// INVARIANT: every Vec in the pool is already all-`Value::Null` (see
/// `return_pooled_regs`, which nulls before pooling; fresh Vecs start Null).
/// So we only `resize` to the requested length — no redundant per-element
/// reset on take (that reset already happened on the matching recycle, and
/// doing it twice is pure per-call overhead on every register slot).
fn take_pooled_regs(size: usize) -> Vec<Value> {
    FRAME_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if let Some(mut regs) = pool.pop() {
            // `regs` is all-Null by the pool invariant; resize keeps it Null
            // (truncated tail is Null → no-op drops; growth fills with Null).
            regs.resize(size, Value::Null);
            regs
        } else {
            vec![Value::Null; size]
        }
    })
}

/// Return a Vec<Value> to the pool for future reuse. Nulls every slot first to
/// release Arc/Rc references promptly AND uphold the all-Null pool invariant
/// relied on by `take_pooled_regs`.
fn return_pooled_regs(mut regs: Vec<Value>) {
    for v in regs.iter_mut() { *v = Value::Null; }
    FRAME_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < POOL_MAX {
            pool.push(regs);
        }
        // else: drop regs (pool is full)
    });
}

// ── FnEntry ──────────────────────────────────────────────────────────────────

/// A compiled native function entry inside the JIT module.
///
/// `Clone` (no longer `Copy`) since we now carry `Arc<str>` for name + file
/// to give `jit_call` / `jit_vcall` cheap access to the callee's stack-trace
/// metadata without reverse lookup into `module.functions`. Clone cost is
/// two `Arc::clone` (refcount bump) — negligible vs. the JIT call itself.
///
/// (2026-05-10 jit-stack-trace; was `Copy` since introduce-method-token
/// Phase 2.C / 2026-05-08.)
#[derive(Clone)]
pub struct FnEntry {
    /// Pointer to the native machine code of the function.
    pub ptr:     *const u8,
    /// Size of the register file needed by this function (`max_reg`).
    pub max_reg: usize,
    /// Fully-qualified function name (e.g. `"Demo.Inner"`), shared via Arc
    /// across all FnEntry copies. Used to push a `FrameInfo` onto
    /// `VmContext.call_stack` when the JIT invokes this function.
    pub name:    std::sync::Arc<str>,
    /// Source file path (from the function's first `LineEntry`). Empty
    /// `Arc<str>` if the line table omits file references.
    pub file:    std::sync::Arc<str>,
}

// Raw pointer — the JITModule that owns the code lives alongside this entry.
unsafe impl Send for FnEntry {}
unsafe impl Sync for FnEntry {}

impl FnEntry {
    /// Negative-cache marker (runtime-jit-tiering Phase 1): a null-`ptr` entry
    /// meaning "this function is not JIT-translatable (or compile failed) — run
    /// it on the interpreter". Filling the slot with this avoids re-running
    /// `jit_unsupported_reason` (a full instruction walk) on every subsequent
    /// call. `resolve_merged_slot` maps it back to `None` so callers fall through
    /// to `cross_zpkg_via_interp` exactly as they did for an empty slot.
    pub fn rejected() -> Self {
        FnEntry { ptr: std::ptr::null(), max_reg: 0, name: "".into(), file: "".into() }
    }
    #[inline]
    pub fn is_rejected(&self) -> bool { self.ptr.is_null() }
}

// ── Lazy slot table (make-vm-loading-lazy) ──────────────────────────────────
//
// Functions materialized by the lazy loader (`try_lookup_function`) are NOT in
// the merged `module.functions`, so they have no pre-sized `fn_entries_by_id`
// slot. They get a **synthetic id** `merged_len + i`; `resolve_fn_by_id` routes
// ids ≥ `merged_len` here. Boxed slots keep each entry's address stable across
// `Vec` growth (the pre-sized `fn_entries_by_id` can't grow; this can). Guarded by
// a `Mutex` — cold path only: the per-Call-site `call_jit_ic` caches the synthetic
// id so steady-state calls read the slot's lock-free `OnceLock` directly via
// `resolve_fn_by_id`.

struct LazySlot {
    /// FQ name — re-`try_lookup_function`'d to get the `Arc<Function>` to compile.
    name:  String,
    /// Compiled native entry, filled on first call to this slot (compile-once).
    entry: OnceLock<FnEntry>,
    /// runtime-jit-tiering Phase 1c: per-lazy-function call counter, the lazy-slot
    /// analogue of `JitModuleCtx.call_counts` (merged path). A lazily-loaded
    /// dep-zpkg function compiles only once its count reaches `jit_threshold`;
    /// below that it stays on the interpreter. Without this, `resolve_lazy_slot`
    /// compiled EVERY reached lazy function on first call, bypassing the threshold
    /// entirely — so one-shot dep `__static_init__` (force-loaded at startup) and
    /// any cold dep function always compiled (measured: ~73% of all compiles).
    count: AtomicU32,
}

/// Growable, address-stable table of lazily-loaded functions. `by_name` assigns a
/// stable index; `slots[i]` (boxed → stable address) holds the compiled entry.
#[derive(Default)]
pub struct LazyTable {
    by_name: HashMap<String, usize>,
    slots:   Vec<Box<LazySlot>>,
}

// ── JitModuleCtx ─────────────────────────────────────────────────────────────

/// Module-level context threaded through every JIT call.
///
/// lazy-per-function-jit (2026-07-23): the compiled-function table is now filled
/// **on first call** rather than eagerly at load. `fn_entries_by_id` holds a
/// per-function `OnceLock` slot (lock-free read on the hot path); a miss routes
/// through `resolve_fn_by_id`, which compiles the function under `lazy` (the
/// Mutex-guarded compiler) exactly once. The former by-name `fn_entries` HashMap
/// is gone — name lookups go through `module.func_index → resolve_fn_by_id`.
pub struct JitModuleCtx {
    /// Interned string constants (mirrors `Module::interned_strings`).
    /// review.md C3 Phase 1 (2026-06-03, add-string-literal-interning-phase1):
    /// pre-interned `Arc<str>` per pool slot; `jit_const_str` clones the
    /// Arc (atomic refcount inc, zero alloc) instead of the prior
    /// `String.clone() + .into::<Arc<str>>()` two-alloc path.
    pub string_pool: Vec<std::sync::Arc<str>>,
    /// Compiled function table — slot `i` corresponds to `module.functions[i]`
    /// (== `MethodId.0` == `module.func_index[name]`). Pre-sized once and never
    /// resized, so a slot's address is stable and `OnceLock::get()` hands out a
    /// `&FnEntry` valid for the whole run. An empty slot = "not yet compiled"
    /// (filled by `resolve_fn_by_id` on first call), and stays empty forever for
    /// functions that aren't JIT-translatable — those run on the interpreter.
    pub fn_entries_by_id: Vec<OnceLock<FnEntry>>,
    /// Back-pointer to the bytecode module for class descriptors, function
    /// bodies (lazy compile), `func_index`, etc.
    /// SAFETY: the Module must outlive this ctx.
    pub module:      *const crate::metadata::Module,
    /// Lazy per-function compiler (owns the cranelift `JITModule` + helper ids),
    /// Mutex-guarded so concurrent first-calls compile each function exactly
    /// once. SAFETY: the `Mutex<LazyCompiler>` is owned by the `JitModule` that
    /// outlives this ctx; never null once constructed.
    pub lazy:        *const Mutex<super::lazy::LazyCompiler>,
    /// `module.functions.len()` — the boundary between merged-module slot ids
    /// (`< merged_len` → `fn_entries_by_id`) and synthetic lazy ids
    /// (`≥ merged_len` → `lazy_table`). make-vm-loading-lazy.
    pub merged_len:  usize,
    /// Lazily-loaded functions' compiled entries (see `LazyTable`). Cold-path
    /// Mutex; steady state hits the per-Call-site `call_jit_ic` → lock-free slot.
    pub lazy_table:  Mutex<LazyTable>,
    /// Mutable VM state (static fields, pending exception, lazy loader).
    /// Set by `JitModule::run` for the duration of one entry-point invocation;
    /// reset to null on return. JIT helpers reach mutable VM state via this
    /// pointer — replaces the previous `thread_local!` slots in
    /// `jit/helpers.rs` (consolidate-vm-state, 2026-04-28).
    /// SAFETY: the VmContext must outlive `JitModule::run` and be unique
    /// (no concurrent JIT entry on the same JitModule).
    pub vm_ctx:      *mut crate::vm_context::VmContext,
    /// runtime-jit-tiering Phase 1: per-merged-function call counter, parallel to
    /// `fn_entries_by_id` (pre-sized `merged_len`, zero per-call heap alloc). A
    /// function compiles only once its count reaches `jit_threshold`; below that
    /// it runs on the interpreter (cold tier). Frozen once the slot is filled
    /// (Compiled/Rejected), so it never overflows in practice.
    pub call_counts: Vec<AtomicU32>,
    /// Tier-up threshold: compile a merged function on its `jit_threshold`-th call
    /// (N=1 → compile-on-first-call = pre-tiering behavior). From
    /// `Z42_JIT_THRESHOLD` (default 2), clamped ≥ 1.
    pub jit_threshold: u32,
}

impl JitModuleCtx {
    /// Resolve function slot `idx` to its compiled `FnEntry`, compiling it on
    /// first demand (compile-on-first-call). Returns `None` when the function
    /// is not JIT-translatable or compilation fails — the caller then falls back
    /// to the interpreter (or raises), exactly as it did when the eager table
    /// simply lacked the entry.
    ///
    /// The already-compiled hot path is lock-free (a single `OnceLock::get`);
    /// only the first compile of each slot takes the compiler mutex, with a
    /// double-check so racing threads compile it exactly once.
    ///
    /// SAFETY: `module` and `lazy` must be valid — they are for the lifetime of
    /// a `JitModule::run` (set at construction; `lazy` never null).
    pub unsafe fn resolve_fn_by_id(&self, id: usize) -> Option<&FnEntry> {
        if id < self.merged_len {
            self.resolve_merged_slot(id, false)
        } else {
            // make-vm-loading-lazy: synthetic id → lazily-loaded function.
            self.resolve_lazy_slot(id - self.merged_len, false)
        }
    }

    /// Tiered variant (runtime-jit-tiering Phase 1): apply the call-count threshold
    /// so only HOT functions compile; cold ones return `None` → caller runs them on
    /// the interpreter. Used ONLY by `jit_call` (static/free calls), whose
    /// `cross_zpkg_via_interp` cold-tier fallback is proven for arbitrary functions.
    /// The method/closure/ctor helpers keep `resolve_fn_by_id` (compile-on-first-call)
    /// — their `None`-fallbacks are not yet robust for arbitrary cold callees (Phase 1b).
    /// The tri-state negative cache applies in BOTH variants.
    pub unsafe fn resolve_fn_by_id_tiered(&self, id: usize) -> Option<&FnEntry> {
        if id < self.merged_len {
            self.resolve_merged_slot(id, true)
        } else {
            self.resolve_lazy_slot(id - self.merged_len, true)
        }
    }

    /// Merged-module path: slot `idx` in the pre-sized `fn_entries_by_id`. Compiles
    /// `module.functions[idx]` on first call (hot path = lock-free `OnceLock::get`).
    unsafe fn resolve_merged_slot(&self, idx: usize, tier: bool) -> Option<&FnEntry> {
        let slot = self.fn_entries_by_id.get(idx)?;
        if let Some(e) = slot.get() {
            // runtime-jit-tiering Phase 1 tri-state: filled slot is Compiled or
            // Rejected (negative cache). Rejected → interp, WITHOUT re-scanning.
            // Applies in BOTH tier modes.
            if e.is_rejected() { return None; }
            return Some(e);
        }
        // Tiered path only: count this call; below threshold → cold tier (interp via
        // jit_call's fallback), don't compile/scan yet. Non-tiered callers
        // (vcall/closure/ctor/entry) compile on first call as before.
        if tier {
            if let Some(cnt) = self.call_counts.get(idx) {
                let n = cnt.fetch_add(1, Ordering::Relaxed) + 1;
                if n < self.jit_threshold { return None; }
            }
        }
        // Threshold reached: scan translatability ONCE, then compile or cache
        // Rejected so future calls skip both the scan and the counter.
        let module = &*self.module;
        let func = module.functions.get(idx)?;
        if super::translate::jit_unsupported_reason(func).is_some() {
            let _ = slot.set(FnEntry::rejected()); // negative-cache the verdict
            return None;
        }
        let mtx = &*self.lazy;
        let mut guard = match mtx.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.get().is_none() {
            let t0 = std::time::Instant::now();
            match guard.compile_one(idx) {
                Ok(entry) => { let _ = slot.set(entry); self.bump_compile_counters(t0); }
                // Compile is deterministic → cache Rejected so we don't re-attempt.
                Err(_) => { let _ = slot.set(FnEntry::rejected()); return None; }
            }
        }
        drop(guard);
        slot.get().filter(|e| !e.is_rejected())
    }

    /// Lazy path (make-vm-loading-lazy): slot `i` in the growable `lazy_table`, for a
    /// function materialized by the lazy loader. Compiles it on first call. The slot's
    /// `OnceLock` (in a boxed `LazySlot` → stable address) hands out a `&FnEntry`
    /// valid for the run; the compile is deduped under the compiler lock.
    unsafe fn resolve_lazy_slot(&self, i: usize, tier: bool) -> Option<&FnEntry> {
        // Stable raw pointers into the boxed slot (survive `Vec` growth), so the
        // table lock is released before the (slow) compile.
        let (name_ptr, entry_ptr, count_ptr):
            (*const String, *const OnceLock<FnEntry>, *const AtomicU32) = {
            let table = match self.lazy_table.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            let slot = table.slots.get(i)?;
            (&slot.name as *const String,
             &slot.entry as *const OnceLock<FnEntry>,
             &slot.count as *const AtomicU32)
        };
        let entry_lock = &*entry_ptr;
        if let Some(e) = entry_lock.get() {
            return Some(e);
        }
        // runtime-jit-tiering Phase 1c: tiered callers count this call; below the
        // threshold → cold tier (return None → caller interprets via its lazy
        // `None`-fallback, the SAME arm already taken for untranslatable lazy
        // funcs). Non-tiered callers (entry / static-init resolve) compile on
        // first call as before. `resolve_id_by_name` already verified the function
        // is translatable before registering this slot, so a cold return here just
        // defers a definitely-compilable function, never hides an error.
        if tier {
            let n = (*count_ptr).fetch_add(1, Ordering::Relaxed) + 1;
            if n < self.jit_threshold { return None; }
        }
        if self.vm_ctx.is_null() { return None; }
        let func = (*self.vm_ctx).try_lookup_function(&*name_ptr)?;
        let mtx = &*self.lazy;
        let mut guard = match mtx.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        if entry_lock.get().is_none() {
            let t0 = std::time::Instant::now();
            match guard.compile_fn(&func) {
                Ok(entry) => { let _ = entry_lock.set(entry); self.bump_compile_counters(t0); }
                Err(_) => return None,
            }
        }
        drop(guard);
        entry_lock.get()
    }

    /// Counters reflect what was ACTUALLY compiled (vs the former eager whole-module
    /// count). `vm_ctx` is set for the duration of `JitModule::run`.
    fn bump_compile_counters(&self, t0: std::time::Instant) {
        if !self.vm_ctx.is_null() {
            let c = unsafe { (*self.vm_ctx).counters() };
            c.jit_methods_compiled.fetch_add(1, Ordering::Relaxed);
            c.jit_compile_us_total
                .fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
        }
    }

    /// Resolve a function NAME to a slot id — merged (`< merged_len`) or synthetic
    /// lazy (`≥ merged_len`). Registers a new lazy slot on first sight (materializing
    /// the function via the lazy loader to confirm it's JIT-translatable). The
    /// per-Call-site `call_jit_ic` caches this id so subsequent calls skip the hash.
    /// Returns None if the name resolves to nothing or an untranslatable function
    /// (caller then falls back to `cross_zpkg_via_interp`).
    /// SAFETY: see [`resolve_fn_by_id`].
    pub unsafe fn resolve_id_by_name(&self, name: &str) -> Option<u32> {
        if let Some(&idx) = (*self.module).func_index.get(name) {
            return Some(idx as u32);
        }
        {
            let table = match self.lazy_table.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            if let Some(&i) = table.by_name.get(name) {
                return Some((self.merged_len + i) as u32);
            }
        }
        // Materialize + verify translatable before assigning an id.
        if self.vm_ctx.is_null() { return None; }
        let func = (*self.vm_ctx).try_lookup_function(name)?;
        if super::translate::jit_unsupported_reason(&func).is_some() { return None; }
        let mut table = match self.lazy_table.lock() { Ok(g) => g, Err(p) => p.into_inner() };
        if let Some(&i) = table.by_name.get(name) {
            return Some((self.merged_len + i) as u32); // registered while we materialized
        }
        let i = table.slots.len();
        table.slots.push(Box::new(LazySlot {
            name: name.to_string(), entry: OnceLock::new(), count: AtomicU32::new(0),
        }));
        table.by_name.insert(name.to_string(), i);
        Some((self.merged_len + i) as u32)
    }

    /// Name-keyed `&FnEntry` resolution: `resolve_id_by_name` → `resolve_fn_by_id`.
    /// SAFETY: see [`resolve_fn_by_id`].
    pub unsafe fn resolve_fn_by_name(&self, name: &str) -> Option<&FnEntry> {
        let id = self.resolve_id_by_name(name)?;
        self.resolve_fn_by_id(id as usize)
    }

    /// Tiered by-name resolve (runtime-jit-tiering Phase 1b): applies the tier-up
    /// threshold. Used by `jit_vcall`'s vtable path, whose `None`-arm robustly
    /// interps the resolved method (receiver + args) for cold callees.
    pub unsafe fn resolve_fn_by_name_tiered(&self, name: &str) -> Option<&FnEntry> {
        let id = self.resolve_id_by_name(name)?;
        self.resolve_fn_by_id_tiered(id as usize)
    }

    /// runtime-jit-tiering Phase 1.5.2: **side-effect-free** "is this function
    /// ALREADY compiled?" check for the interp central divert (`try_native_exec`).
    /// Returns `Some(entry)` only when the slot is already filled with a compiled
    /// (non-rejected) entry — **never increments the tier counter, never compiles,
    /// never registers a lazy slot**. The divert's job is to ROUTE an already-hot
    /// function to native, NOT to tier it up (counting belongs to the primary call
    /// sites: `jit_call` / per-site interp hooks / `jit_obj_new` / vtable). Using
    /// the *tiered* resolve here double-counted a cold callee — once at `jit_call`,
    /// again at its interp fallback's `exec_function` — halving the effective
    /// threshold and prematurely compiling cold functions.
    pub unsafe fn resolve_fn_by_name_peek(&self, name: &str) -> Option<&FnEntry> {
        // Merged path: pre-sized OnceLock slot, stable address.
        if let Some(&idx) = (*self.module).func_index.get(name) {
            let e = self.fn_entries_by_id.get(idx)?.get()?;
            return if e.is_rejected() { None } else { Some(e) };
        }
        // Lazy path: peek an already-registered slot WITHOUT registering a new one.
        // Boxed slot → stable heap address, so the borrow outlives the table lock
        // (same invariant `resolve_lazy_slot` relies on).
        let entry_ptr: *const OnceLock<FnEntry> = {
            let table = match self.lazy_table.lock() { Ok(g) => g, Err(p) => p.into_inner() };
            let &i = table.by_name.get(name)?;
            &table.slots.get(i)?.entry as *const OnceLock<FnEntry>
        };
        let e = (*entry_ptr).get()?;
        if e.is_rejected() { None } else { Some(e) }
    }
}

// SAFETY: raw pointer — caller ensures Module outlives ctx.
unsafe impl Send for JitModuleCtx {}
unsafe impl Sync for JitModuleCtx {}
