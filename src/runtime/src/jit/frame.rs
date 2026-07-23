/// JIT frame and module context types.
///
/// `JitFrame` is the runtime stack frame passed (as a raw pointer) to every
/// JIT-compiled function.  `JitModuleCtx` is the read-only module-level context
/// that is shared across all calls within a single module execution.

use crate::metadata::Value;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
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
    /// Mutable VM state (static fields, pending exception, lazy loader).
    /// Set by `JitModule::run` for the duration of one entry-point invocation;
    /// reset to null on return. JIT helpers reach mutable VM state via this
    /// pointer — replaces the previous `thread_local!` slots in
    /// `jit/helpers.rs` (consolidate-vm-state, 2026-04-28).
    /// SAFETY: the VmContext must outlive `JitModule::run` and be unique
    /// (no concurrent JIT entry on the same JitModule).
    pub vm_ctx:      *mut crate::vm_context::VmContext,
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
    pub unsafe fn resolve_fn_by_id(&self, idx: usize) -> Option<&FnEntry> {
        let slot = self.fn_entries_by_id.get(idx)?;
        if let Some(e) = slot.get() {
            return Some(e);
        }
        // Miss: compile on demand iff the function is JIT-translatable.
        let module = &*self.module;
        let func = module.functions.get(idx)?;
        if super::translate::jit_unsupported_reason(func).is_some() {
            return None; // interp fallback — mirrors eager "absent from table"
        }
        // Serialize compilation; re-check the slot after taking the lock in case
        // another thread compiled it while we waited.
        let mtx = &*self.lazy;
        let mut guard = match mtx.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if slot.get().is_none() {
            let t0 = std::time::Instant::now();
            match guard.compile_one(idx) {
                Ok(entry) => {
                    let _ = slot.set(entry);
                    // Counters reflect what was ACTUALLY compiled (vs the former
                    // eager whole-module count). vm_ctx is set for the duration
                    // of JitModule::run, i.e. whenever a lazy compile can happen.
                    if !self.vm_ctx.is_null() {
                        let c = (*self.vm_ctx).counters();
                        c.jit_methods_compiled.fetch_add(1, Ordering::Relaxed);
                        c.jit_compile_us_total
                            .fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
                    }
                }
                Err(_) => return None, // compile failed → interp fallback
            }
        }
        drop(guard);
        slot.get()
    }

    /// Name-keyed resolution: `module.func_index[name] → idx → resolve_fn_by_id`.
    /// Replaces the former by-name `fn_entries` HashMap.
    /// SAFETY: see [`resolve_fn_by_id`].
    pub unsafe fn resolve_fn_by_name(&self, name: &str) -> Option<&FnEntry> {
        let module = &*self.module;
        let idx = *module.func_index.get(name)?;
        self.resolve_fn_by_id(idx)
    }
}

// SAFETY: raw pointer — caller ensures Module outlives ctx.
unsafe impl Send for JitModuleCtx {}
unsafe impl Sync for JitModuleCtx {}
