//! `JitRuntime` — the VM's one JIT, shared by every thread of the VM.
//!
//! **Context parameter.** Every compiled function and every JIT helper takes
//! the running thread's own [`VmContext`] as its context parameter
//! (`fn(frame: *mut JitFrame, vm: *const VmContext) -> u8`). Per-thread state —
//! frame stack, pending exception, register pool, arenas, safepoint throttle,
//! stack limit, counter shard — is therefore reached directly; generated code
//! reads the few hot fields at fixed offsets (`vm_context::JitHead`,
//! `VM_CONTEXT_*_OFFSET`). Shared JIT state lives here, on `VmCore::jit`, and a
//! helper reaches it as `vm.jit()` (one load: the pointer is cached in the
//! context's `JitHead`).
//!
//! **What is shared.** The per-function slot table (keyed by JIT id = `FnId`,
//! lock-free reads), the compiler (`Mutex<LazyCompiler>`, owning the cranelift
//! `JITModule` and so the code pages), the OSR entry cache, the tier thresholds
//! and the module the JIT ids are relative to. Installed once per VM by the
//! first [`install`] (`jit::run`) and dropped with the core, so compiled code
//! stays valid for as long as any thread holds the core.
//!
//! **Compilation** happens on the calling thread, under the compiler mutex: a
//! thread that reaches a function's tier threshold compiles it; another thread
//! wanting the same or another function waits for the lock (unparked — see
//! [`JitRuntime::lock_compiler`]). The result is published through the slot's
//! `OnceLock` (release / acquire), after cranelift has made the code executable
//! and flushed the instruction pipelines of every thread.
//!
//! **JIT ids.** Every function the JIT can run is named by one `usize` id, which
//! is the function's `FnId` (`VmCore.funcs`): entry-module functions are
//! `0..module.functions.len()` (= their `module.functions` index), lazily loaded
//! package functions the ids after that. Interp `Call` tokens
//! (`ResolvedTokens.method_tokens`) hold the same ids, so `jit_call` bakes and
//! caches them directly. When `module` is not the `FuncTable`'s entry module
//! (unit tests running a bare `VmContext::new()`), only `module.functions`
//! indices are ids and lazily loaded functions have none — they run on the
//! interpreter, exactly as the interp token rule (`FuncTable::is_entry`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use anyhow::Result;

use crate::vm::runtime::ids::func_table::FuncTable;
use crate::vm::runtime::ids::seg_vec::SparseSegTable;
use crate::vm::runtime::ids::tokens::FnId;
use crate::vm::runtime::{Function, Module};
use crate::vm_context::VmContext;

use super::forward::CompileLookup;
use super::frame::FnEntry;
use super::lazy::LazyCompiler;

// ── JitSlot ──────────────────────────────────────────────────────────────────

/// [`JitSlot::state`]: not decided yet (cold, or never reached the threshold).
const SLOT_UNTRIED: u8 = 0;
/// [`JitSlot::state`]: untranslatable or compile failed — stays on the interpreter.
/// Compilation is deterministic, so the verdict is final.
const SLOT_REJECTED: u8 = 1;

/// Per-function JIT state, one per JIT id (see [`JitRuntime::slots`]).
///
/// Lifecycle: `Untried` (counting calls) → `entry` filled (compiled) or
/// `state = Rejected`. Both outcomes are final; neither re-scans nor recompiles.
pub struct JitSlot {
    /// Compiled native entry. Set once, under the compiler lock; read lock-free
    /// by every thread (`OnceLock` publishes with release / acquire).
    entry: OnceLock<FnEntry>,
    /// Tier-up counter: tiered resolves count calls while the slot is untried and
    /// compile at `jit_threshold`. Frozen once the slot is decided. Threads count
    /// together (relaxed — a lost increment only delays the tier-up by one call).
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

// ── JitRuntime ───────────────────────────────────────────────────────────────

/// The VM's JIT (see the module docs).
pub struct JitRuntime {
    /// Per-function compile state indexed by JIT id. Lock-free read (a segment
    /// is allocated on first touch and never moves), so a compiled call is one
    /// `get` + one `OnceLock::get`. Slots live as long as the compiler whose
    /// code they point at (both are fields of this struct).
    slots:       SparseSegTable<JitSlot>,
    /// The bytecode module JIT ids are relative to (see the module docs). In
    /// production the VM's own `VmCore::module`; a unit test may install the JIT
    /// over a module it owns, which must then outlive the JIT's use.
    pub(crate) module: *const Module,
    /// The VM's function table (lazily loaded functions by `FnId`).
    funcs:       Arc<FuncTable>,
    /// `funcs` lists `module` as its entry module ⇒ ids past
    /// `module.functions` name lazily loaded functions.
    lazy_ids:    bool,
    /// Lazy per-function compiler (owns the cranelift `JITModule` + helper ids).
    /// Mutex-guarded so concurrent first-calls compile each function exactly
    /// once; see [`Self::lock_compiler`].
    lazy:        Mutex<LazyCompiler>,
    /// Tier-up threshold: compile a function on its `jit_threshold`-th tiered
    /// call (N=1 → compile-on-first-call). From the `jit-threshold` runtime knob
    /// (default 2; see `jit/mod.rs`), clamped ≥ 1.
    pub(crate) jit_threshold: u32,
    /// Compiled OSR entries keyed by `(JIT id, loop-header block K)`. Keyed by K
    /// too because a function with two loops can OSR at different headers — a K1
    /// entry must not be reused for a K2 hand-off. Filled under the compiler lock,
    /// at most once per key (OSR is rare); the `FnEntry` is cloned out (owned) so
    /// callers don't hold the lock across the native call. `rejected()` marks
    /// untranslatable / compile-failed.
    osr_entries: Mutex<HashMap<(usize, usize), FnEntry>>,
    /// OSR trigger threshold (loop back-edges in the interp).
    /// From `Z42_OSR_THRESHOLD`, clamped ≥ 1.
    pub(crate) osr_threshold: u32,
}

// SAFETY: `module` points at a module that outlives the JIT's use (the VM's own
// `Arc<Module>` in production) and is only read. Everything else is `Send + Sync`
// on its own (`LazyCompiler` is only touched under its mutex).
unsafe impl Send for JitRuntime {}
unsafe impl Sync for JitRuntime {}

/// Install the JIT on `vm`'s VM for `module`, or return the one already
/// installed. One JIT per VM: a second install for a different module fails.
///
/// The JIT is built without compiling any user function (compile-on-first-call).
/// Racing installers build one each; the first published wins and the other is
/// dropped unused.
pub fn install<'a>(vm: &'a VmContext, module: &Module) -> Result<&'a JitRuntime> {
    let rt = match vm.core.jit.get() {
        Some(rt) => rt,
        None => {
            let _ = vm.core.jit.set(JitRuntime::new(vm, module)?);
            vm.core.jit.get().expect("JIT runtime just installed")
        }
    };
    publish_to(vm, rt);
    anyhow::ensure!(
        std::ptr::eq(rt.module, module),
        "JIT: this VM already runs the JIT for module `{}`",
        // SAFETY: an installed runtime's module outlives it.
        unsafe { &*rt.module }.name
    );
    Ok(rt)
}

/// Test hook: install a JIT on `vm`'s VM after `tune` adjusted it (thresholds).
/// Panics when the VM already has one.
#[cfg(test)]
pub(crate) fn install_tuned<'a>(
    vm: &'a VmContext, module: &Module, tune: impl FnOnce(&mut JitRuntime),
) -> &'a JitRuntime {
    let mut rt = JitRuntime::new(vm, module).expect("JIT setup");
    tune(&mut rt);
    assert!(vm.core.jit.set(rt).is_ok(), "a JIT is already installed on this VM");
    let rt = vm.core.jit.get().expect("installed");
    publish_to(vm, rt);
    rt
}

/// Cache the runtime in the installing context's head (`VmContext::jit`). Other
/// contexts take it at construction or at their next bottom frame push.
fn publish_to(vm: &VmContext, rt: &JitRuntime) {
    vm.jit_head.jit.store(rt as *const JitRuntime as *mut JitRuntime, Ordering::Relaxed);
}

impl JitRuntime {
    fn new(vm: &VmContext, module: &Module) -> Result<Self> {
        let cfg = crate::config::runtime_config();
        Ok(JitRuntime {
            slots: SparseSegTable::new(),
            module: module as *const Module,
            funcs: Arc::clone(&vm.core.funcs),
            lazy_ids: vm.core.funcs.is_entry(module),
            lazy: Mutex::new(LazyCompiler::setup()?),
            // Tier-up threshold (`jit-threshold` knob, default 2): a function compiles on
            // its 2nd tiered call. 1 also compiled every run-once function (561 Cranelift
            // compiles ≈ 0.33 s of a 0.80 s hello-world build); larger values keep the
            // z42c pipeline — functions called a handful of times per build — on the
            // interpreter. OSR (`osr_threshold`) upgrades hot loops independently.
            jit_threshold: cfg.jit_threshold,
            osr_entries: Mutex::new(HashMap::new()),
            // Back-edge count that triggers OSR of the running interp activation.
            osr_threshold: cfg.osr_threshold,
        })
    }

    /// The module JIT ids are relative to.
    #[inline]
    pub(crate) fn module(&self) -> &Module {
        // SAFETY: see the field doc.
        unsafe { &*self.module }
    }

    // ── ids ──────────────────────────────────────────────────────────────────

    /// The function JIT id `id` names (see the module docs), or `None` for an id
    /// nothing is registered under. Lock-free.
    pub fn fn_of(&self, id: usize) -> Option<&Function> {
        if let Some(f) = self.module().functions.get(id) {
            return Some(f);
        }
        if !self.lazy_ids { return None; }
        self.funcs.get(FnId(u32::try_from(id).ok()?))
    }

    /// The JIT id of `f`, if it has one: an entry-module function by address, a
    /// lazily loaded one by its `FnId` (only the function the table holds under
    /// that id). Lock-free, no hashing.
    pub fn id_of_func(&self, f: &Function) -> Option<usize> {
        let fns = &self.module().functions;
        let base = fns.as_ptr();
        let p = f as *const Function;
        if p >= base && p < base.wrapping_add(fns.len()) {
            // SAFETY: `p` lies inside `fns`.
            return Some(unsafe { p.offset_from(base) } as usize);
        }
        let id = f.id.get()?.0 as usize;
        std::ptr::eq(self.fn_of(id)?, f).then_some(id)
    }

    /// Cold path: the JIT id a call by `name` resolves to — this module's
    /// `func_index` first, then the lazily loaded functions (which may load the
    /// defining package, through `vm`). `None` when the name resolves nowhere or
    /// to a function without a JIT id.
    pub fn id_by_name(&self, vm: &VmContext, name: &str) -> Option<usize> {
        if let Some(&i) = self.module().func_index.get(name) {
            return Some(i);
        }
        if !self.lazy_ids { return None; }
        if let Some(id) = self.funcs.lazy_id(name) {
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
    /// threads compile each function exactly once. `vm` is the calling thread's
    /// context: the compile runs on it (token resolution, counters).
    #[inline]
    pub fn resolve_fn_by_id(&self, vm: &VmContext, id: usize) -> Option<&FnEntry> {
        self.resolve_fn_by_id_thr(vm, id, 0)
    }

    /// Tiered variant: count this call and compile only at `jit_threshold`; below
    /// it return `None` so the caller runs the function on the interpreter (cold
    /// tier). Used by every call path that has an interpreter fallback (`jit_call`,
    /// vcall, closures, ctors, and the interpreter's own per-site native routing).
    #[inline]
    pub fn resolve_fn_by_id_tiered(&self, vm: &VmContext, id: usize) -> Option<&FnEntry> {
        self.resolve_fn_by_id_thr(vm, id, self.jit_threshold)
    }

    /// [`Self::resolve_fn_by_id`] with an explicit threshold (`0` = compile now).
    #[inline]
    pub fn resolve_fn_by_id_thr(&self, vm: &VmContext, id: usize, thr: u32) -> Option<&FnEntry> {
        if let Some(e) = self.slots.get(id).and_then(|s| s.entry.get()) {
            return Some(e);
        }
        self.resolve_slot_slow(vm, id, thr)
    }

    /// Side-effect-free "is this function ALREADY compiled?" for the interpreter's
    /// central divert (`try_native_exec`): never counts, never compiles. The divert
    /// only routes already-hot functions to native; counting belongs to the
    /// primary call sites (a cold callee's interp fallback must not count it twice).
    #[inline]
    pub fn peek_fn_by_id(&self, id: usize) -> Option<&FnEntry> {
        self.slots.get(id)?.entry.get()
    }

    /// The compiler, locked. A thread that finds it busy blocks without parking:
    /// callers may hold values reachable only from Rust locals across a resolve
    /// (`jit_obj_new`'s fresh instance before its ctor runs), which a collection
    /// running during a park would not see. The holder never reaches a safepoint
    /// or waits for a collection while it holds the lock — the translator only
    /// probes what is already loaded (`forward::CompileLookup`), and everything
    /// that may load a package or run z42 code (token resolution, the chained
    /// forward lookup) happens before the lock is taken — so the wait is bounded by
    /// one compile, the same footing as the VM's other internal locks.
    fn lock_compiler(&self) -> MutexGuard<'_, LazyCompiler> {
        match self.lazy.lock() { Ok(g) => g, Err(p) => p.into_inner() }
    }

    /// The not-yet-compiled half of [`Self::resolve_fn_by_id_thr`]: Rejected →
    /// `None` at once; otherwise count (tiered), then decide once — untranslatable
    /// or compile-failed ⇒ Rejected, else compile under the compiler lock.
    #[inline(never)]
    fn resolve_slot_slow(&self, vm: &VmContext, id: usize, thr: u32) -> Option<&FnEntry> {
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
        // the callee executes against (identity invariant). Outside the compiler
        // lock: resolution may load a package.
        if func.resolved.get().is_none() {
            crate::vm::runtime::resolve::resolve_function_tokens(func, self.module(), vm);
        }
        if super::translate::jit_unsupported_reason(func).is_some() {
            slot.state.store(SLOT_REJECTED, Ordering::Relaxed);
            return None;
        }
        // `[Native]` forwarder? (Shared with the interpreter and the cold call paths;
        // the probe never loads anything.)
        let forward = crate::interp::forward::forward_of(vm, self.module(), func);
        let mut guard = self.lock_compiler();
        if slot.entry.get().is_none() {
            if slot.state.load(Ordering::Relaxed) == SLOT_REJECTED { return None; }
            let t0 = std::time::Instant::now();
            let lookup = CompileLookup { rt: self, vm };
            match guard.compile_fn(func, &lookup) {
                Ok(mut entry) => {
                    entry.forward = forward;
                    let _ = slot.entry.set(entry);
                    bump_compile_counters(vm, t0);
                }
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

    /// Test hook: compile `func` with an explicit callee lookup, bypassing the slots.
    #[cfg(test)]
    pub(crate) fn compile_with(
        &self, func: &Function, callees: &dyn super::forward::ForwardLookup,
    ) -> Result<FnEntry> {
        self.lock_compiler().compile_fn(func, callees)
    }

    /// Name-keyed resolution (compile now): `id_by_name` → `resolve_fn_by_id`.
    pub fn resolve_fn_by_name(&self, vm: &VmContext, name: &str) -> Option<&FnEntry> {
        self.resolve_fn_by_id(vm, self.id_by_name(vm, name)?)
    }

    /// Resolve (compiling on first sight) the **OSR entry** of JIT id `id` at
    /// loop-header block `k` — entry-module and lazily loaded functions alike.
    /// Returns an OWNED `FnEntry` clone (so the caller doesn't hold the cache lock
    /// across the native call), or `None` if the function is untranslatable or the
    /// compile failed. Cached per `(id, k)` — a second hot activation of the same
    /// loop, on any thread, reuses it. The cache is checked again and filled under
    /// the compiler lock, so racing threads compile a variant once.
    pub fn resolve_osr_entry(&self, vm: &VmContext, id: usize, k: usize) -> Option<FnEntry> {
        let cached = |map: &HashMap<(usize, usize), FnEntry>| {
            map.get(&(id, k)).map(|e| if e.is_rejected() { None } else { Some(e.clone()) })
        };
        if let Some(hit) = cached(&self.lock_osr()) {
            return hit;
        }
        // Not cached — compile the OSR variant (translatable check first).
        let func = self.fn_of(id)?;
        let compiled: FnEntry = if super::translate::jit_unsupported_reason(func).is_some() {
            FnEntry::rejected()
        } else {
            let mut guard = self.lock_compiler();
            if let Some(hit) = cached(&self.lock_osr()) {
                return hit;
            }
            let lookup = CompileLookup { rt: self, vm };
            let e = match guard.compile_fn_osr(func, k, &lookup) {
                Ok(e) => {
                    vm.counters().jit_methods_compiled.fetch_add(1, Ordering::Relaxed);
                    e
                }
                Err(_) => FnEntry::rejected(),
            };
            // Published before the compiler lock drops (lock order: compiler → cache).
            let mut map = self.lock_osr();
            let e = map.entry((id, k)).or_insert(e);
            return if e.is_rejected() { None } else { Some(e.clone()) };
        };
        let mut map = self.lock_osr();
        let e = map.entry((id, k)).or_insert(compiled);
        if e.is_rejected() { None } else { Some(e.clone()) }
    }

    fn lock_osr(&self) -> MutexGuard<'_, HashMap<(usize, usize), FnEntry>> {
        match self.osr_entries.lock() { Ok(g) => g, Err(p) => p.into_inner() }
    }
}

/// Counters reflect what was ACTUALLY compiled, on the compiling thread's shard.
fn bump_compile_counters(vm: &VmContext, t0: std::time::Instant) {
    let c = vm.counters();
    c.jit_methods_compiled.fetch_add(1, Ordering::Relaxed);
    c.jit_compile_us_total.fetch_add(t0.elapsed().as_micros() as u64, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
