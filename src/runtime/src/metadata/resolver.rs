//! Load-time token resolution for the introduce-method-token spec
//! (Phase 1, 2026-05-08). Walks every IR instruction in a freshly built
//! `Module` and pre-fills per-function `ResolvedTokens` so the dispatch
//! hot path can index `Vec<Function>` / `Vec<Value>` directly without
//! per-call hashing.
//!
//! Only **load-time-knowable** references are resolved here:
//!
//!   • `Call.func`            → `MethodId` (intra-module hits; cross-zpkg
//!                              left UNRESOLVED, filled on first dispatch)
//!   • `Builtin.name`         → `BuiltinId` (closed set — panic on miss)
//!   • `ObjNew.class_name`    → `TypeId` (intra-module; cross-zpkg lazy)
//!   • `StaticGet/Set.field`  → `StaticFieldId` (lazy global ID via
//!                              `VmContext::resolve_static_field_id`)
//!
//! **Receiver-type-dependent** references (`VCall.method`, `FieldGet/Set
//! .field_name`) are *not* resolved here. They use per-site monomorphic
//! inline caches (`VCallIC` / `FieldIC`) populated on first dispatch.
//!
//! Population timing: called from `Vm::run` after `merge_modules` /
//! `build_type_registry` are done (so all intra-module lookups succeed)
//! and before any dispatch happens (so hot paths see fully-populated
//! `ResolvedTokens`).

use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::Function;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, OnceLock};

/// Per-function lazy-init cache populated by `resolve_module`. Stored on
/// `Function.resolved: OnceLock<ResolvedTokens>` (`#[serde(skip)]`).
///
/// Layout: each token-kind has its own `Vec` indexed by **per-kind site
/// index** (Call sites are numbered 0..N independently of Builtin sites).
/// `site_index[block_idx][instr_idx]` maps a (block, instruction) location
/// to the appropriate site index for that kind.
#[derive(Debug, Default)]
pub struct ResolvedTokens {
    /// `Call` sites: cached `MethodId` (UNRESOLVED until first dispatch
    /// resolves it via `module.func_index` or lazy loader).
    pub method_tokens: Vec<AtomicU32>,
    /// `Call` sites: cached **cross-zpkg** target (review.md C7,
    /// cache-cross-zpkg-call-target). Parallel to `method_tokens` (same site
    /// index). A cross-zpkg target lives in the lazy loader's `function_table`,
    /// not `module.functions`, so a `u32` index can't reach it — the resolved
    /// `Arc<Function>` is cached here on first dispatch and borrowed thereafter
    /// (`OnceLock::get`), eliminating the per-call `try_lookup_function` hash.
    /// Empty cell for intra-module-only sites. `OnceLock` (write-once) because
    /// FQ-name → target is stable within a run; `Sync`-safe for future MT.
    pub cross_module_targets: Vec<OnceLock<Arc<Function>>>,
    /// `Call` sites, JIT only (make-vm-loading-lazy): per-site inline cache of the
    /// resolved **function id** for the JIT `resolve_fn_by_id` fast path. Parallel
    /// to `method_tokens`. A merged-module target's id is baked as the `Call`'s
    /// `method_id` constant at translate time, so this cell stays `UNRESOLVED` for
    /// those; it caches only **lazily-loaded** targets (absent from `module.functions`
    /// at translate time → baked `method_id = UNRESOLVED`). On first dispatch
    /// `jit_call` resolves the name to a synthetic lazy-slot id, compiles the
    /// function once, and stores the id here so subsequent calls skip the name hash
    /// (mirrors `cross_module_targets` for interp, but caches the id the JIT
    /// `resolve_fn_by_id` consumes). `u32` (not the jit `FnEntry`) to avoid a
    /// metadata→jit dependency cycle; the id maps to a per-run compiled entry in
    /// `JitModuleCtx`'s lazy slot table.
    pub call_jit_ic: Vec<AtomicU32>,
    /// `Builtin` sites: `BuiltinId` resolved at load (closed set —
    /// panic if a builtin name is unknown).
    pub builtin_tokens: Vec<u32>,
    /// `ObjNew` sites: cached `TypeId` (similar lifecycle to method_tokens).
    pub type_tokens: Vec<AtomicU32>,
    /// `VCall` sites: monomorphic inline cache (TypeId, vtable slot, MethodId).
    pub vcall_ic: Vec<VCallIC>,
    /// `FieldGet` / `FieldSet` sites: monomorphic inline cache (TypeId, field slot).
    pub field_ic: Vec<FieldIC>,
    /// `StaticGet` / `StaticSet` sites: cached `StaticFieldId`.
    pub static_field_tokens: Vec<AtomicU32>,
    /// `(block_idx, instr_idx) → site_idx` mapping. Outer Vec indexed by
    /// `block_idx`, inner Vec by `instr_idx`. Stores the appropriate
    /// per-kind site index for the instruction at that location, or
    /// `UNRESOLVED` for non-token-bearing instructions.
    pub site_index: Vec<Vec<u32>>,
}

/// review.md C4 P2 + C5 P2 (jit-polymorphic-ic, 2026-05-28): 4-slot
/// polymorphic inline cache. `IC_SLOTS` is the lookup window per site.
/// Sites that observe ≤4 receiver types fast-path through linear scan
/// with early exit on `UNRESOLVED`. Sites with > 4 types victimize
/// via round-robin counter — the per-IC `round_robin` AtomicU32 ticks
/// on each install, modulo 4 picks the slot to overwrite.
pub const IC_SLOTS: usize = 4;

/// Single VCall PIC entry — (TypeId, vtable slot, target MethodId).
#[derive(Debug)]
pub struct VCallICEntry {
    pub type_id: AtomicU32,
    pub slot:    AtomicU32,
    pub fn_idx:  AtomicU32,
}

impl Default for VCallICEntry {
    fn default() -> Self {
        Self {
            type_id: AtomicU32::new(UNRESOLVED),
            slot:    AtomicU32::new(UNRESOLVED),
            fn_idx:  AtomicU32::new(UNRESOLVED),
        }
    }
}

/// Polymorphic inline cache for `VCall` sites. Linear scan through up to
/// `IC_SLOTS` (TypeId, slot, fn_idx) entries; first matching `type_id`
/// returns the cached dispatch. Sites that see < `IC_SLOTS` types skip
/// remaining slots via the `UNRESOLVED` early-exit sentinel.
///
/// Pre-2026-05-28 this was a single (type_id, slot, fn_idx) tuple —
/// mono only; polymorphic sites bounced between the cache and
/// `vtable_index` HashMap. PIC closes that gap for sites with ≤4
/// observed receiver types. Mono fast-path is preserved at zero cost
/// (the scan hits slot 0 in one compare).
///
/// Eviction: on miss past all `IC_SLOTS` filled entries, overwrite the
/// slot indexed by `round_robin.fetch_add(1, Relaxed) % IC_SLOTS`.
/// Round-robin sacrifices recency for the simplicity of a single atomic
/// increment per miss + zero overhead per hit.
#[derive(Debug)]
pub struct VCallIC {
    pub entries:     [VCallICEntry; IC_SLOTS],
    pub round_robin: AtomicU32,
}

impl Default for VCallIC {
    fn default() -> Self {
        Self {
            entries:     std::array::from_fn(|_| VCallICEntry::default()),
            round_robin: AtomicU32::new(0),
        }
    }
}

/// Single Field PIC entry — (TypeId, field slot).
#[derive(Debug)]
pub struct FieldICEntry {
    pub type_id: AtomicU32,
    pub slot:    AtomicU32,
}

impl Default for FieldICEntry {
    fn default() -> Self {
        Self {
            type_id: AtomicU32::new(UNRESOLVED),
            slot:    AtomicU32::new(UNRESOLVED),
        }
    }
}

/// Polymorphic inline cache for `FieldGet` / `FieldSet` sites. Mirrors
/// `VCallIC` PIC layout but caches just (TypeId, field slot). See
/// [`VCallIC`] for the linear-scan + round-robin-eviction protocol.
#[derive(Debug)]
pub struct FieldIC {
    pub entries:     [FieldICEntry; IC_SLOTS],
    pub round_robin: AtomicU32,
}

impl Default for FieldIC {
    fn default() -> Self {
        Self {
            entries:     std::array::from_fn(|_| FieldICEntry::default()),
            round_robin: AtomicU32::new(0),
        }
    }
}

// ── PIC lookup + install helpers (shared interp + JIT) ──────────────────────
//
// Inline lookup helpers used by both the interp dispatch (exec_object.rs +
// exec_vcall.rs) and the JIT helper bodies (helpers/object.rs, vcall.rs).
//
// SAFETY / atomic ordering: all loads / stores use `Ordering::Relaxed` —
// the type_id check gates payload use, so a torn read (type_id of slot A,
// payload of slot B) is bounded to "got wrong cached entry for a type that
// IS currently transitioning"; subsequent reads converge to a valid state.
// Same hazard the pre-PIC mono IC accepted.

/// PIC lookup for `FieldIC`. Returns `Some(slot)` on hit; `None` on miss
/// (caller must do `field_index.get(name)` fallback + `field_ic_install`).
#[inline]
pub fn field_ic_lookup(ic: &FieldIC, recv_type: u32) -> Option<u32> {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return None; }
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == recv_type { return Some(entry.slot.load(Relaxed)); }
        if tid == UNRESOLVED { return None; }  // early exit: rest are empty
    }
    None
}

/// PIC install for `FieldIC`. Finds the first `UNRESOLVED` slot; if all
/// filled, picks the round-robin victim. Idempotent for the same
/// `(recv_type, slot)` pair (just re-writes the same data).
#[inline]
pub fn field_ic_install(ic: &FieldIC, recv_type: u32, slot: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return; }
    // First-empty-slot install.
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == UNRESOLVED || tid == recv_type {
            entry.slot.store(slot, Relaxed);
            entry.type_id.store(recv_type, Relaxed);  // write type_id LAST
            return;
        }
    }
    // All filled — round-robin victim.
    let victim = (ic.round_robin.fetch_add(1, Relaxed) as usize) % IC_SLOTS;
    let entry = &ic.entries[victim];
    entry.slot.store(slot, Relaxed);
    entry.type_id.store(recv_type, Relaxed);
}

/// PIC lookup for `VCallIC`. Returns `Some((slot, fn_idx))` on hit.
#[inline]
pub fn vcall_ic_lookup(ic: &VCallIC, recv_type: u32) -> Option<(u32, u32)> {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return None; }
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == recv_type {
            return Some((entry.slot.load(Relaxed), entry.fn_idx.load(Relaxed)));
        }
        if tid == UNRESOLVED { return None; }
    }
    None
}

/// PIC install for `VCallIC`. Same protocol as `field_ic_install` but
/// writes a (slot, fn_idx) pair before publishing type_id.
#[inline]
pub fn vcall_ic_install(ic: &VCallIC, recv_type: u32, slot: u32, fn_idx: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    if recv_type == UNRESOLVED { return; }
    for entry in &ic.entries {
        let tid = entry.type_id.load(Relaxed);
        if tid == UNRESOLVED || tid == recv_type {
            entry.slot.store(slot, Relaxed);
            entry.fn_idx.store(fn_idx, Relaxed);
            entry.type_id.store(recv_type, Relaxed);
            return;
        }
    }
    let victim = (ic.round_robin.fetch_add(1, Relaxed) as usize) % IC_SLOTS;
    let entry = &ic.entries[victim];
    entry.slot.store(slot, Relaxed);
    entry.fn_idx.store(fn_idx, Relaxed);
    entry.type_id.store(recv_type, Relaxed);
}

/// Walk every Function in `module` and populate its `resolved`
/// `OnceLock<ResolvedTokens>`. Idempotent: once `OnceLock` is
/// initialised on a function, subsequent calls are no-ops (the
/// `let _ = ...` ignores the duplicate-set error).
///
/// `ctx` is needed for `StaticGet/Set` resolution: static field IDs are
/// allocated lazily through `VmContext::resolve_static_field_id` so
/// cross-zpkg static fields can be encountered in any load order.
pub fn resolve_module(module: &crate::metadata::Module, ctx: &crate::vm_context::VmContext) {
    for func in &module.functions {
        resolve_function_tokens(func, module, ctx);
    }
}

/// Populate one `Function`'s `ResolvedTokens` against `module` + `ctx`.
///
/// Extracted from `resolve_module` (perf-lazy-resolve-tokens, 2026-08-18) so
/// **lazily-loaded** functions — whose owning zpkg is never passed through
/// `resolve_module` (only the entry module is, in `Vm::run`) — can populate
/// their per-site caches too. Before this, every function in a lazily-loaded
/// package (e.g. all of z42c.semantics / z42c.syntax during a self-compile)
/// ran with `resolved == None`, so its VCall PIC / FieldIC / builtin-id /
/// static-field-id / call-token caches were all dead and every dispatch fell
/// back to string-keyed hashing.
///
/// **Module identity invariant**: `module` MUST be the same `Module` the
/// function will execute against at runtime (always the entry module — lazy
/// callees are invoked with the caller's `module`, which threads down from the
/// entry). `method_tokens` / `type_tokens` are indices into `module.functions`
/// / `module.type_registry`; resolving them against a *different* module would
/// mint wrong indices. Cross-module targets (absent from the entry module)
/// correctly resolve to `UNRESOLVED` here and are cached per-site on first
/// dispatch via `cross_module_targets`.
///
/// Idempotent: `OnceLock::set` no-ops if another path (or a concurrent thread)
/// already populated this function.
pub fn resolve_function_tokens(
    func: &Function,
    module: &crate::metadata::Module,
    ctx: &crate::vm_context::VmContext,
) {
    {
        // Skip if already populated (idempotent).
        if func.resolved.get().is_some() {
            return;
        }

        // ─── Pass 1: enumerate token-bearing sites ────────────────────────
        // Per-kind site lists. Each entry: the source-string at that site,
        // captured for pass-2 resolution. site_index[block][instr] = the
        // appropriate per-kind site_idx (or UNRESOLVED for non-token instructions).
        let mut method_site_names:   Vec<String> = Vec::new();
        let mut builtin_site_names:  Vec<String> = Vec::new();
        let mut type_site_names:     Vec<String> = Vec::new();
        let mut static_site_names:   Vec<String> = Vec::new();
        let mut vcall_site_count:    u32 = 0;
        let mut field_site_count:    u32 = 0;

        let mut site_index: Vec<Vec<u32>> = Vec::with_capacity(func.blocks.len());

        for block in &func.blocks {
            let mut block_sites = vec![UNRESOLVED; block.instructions.len()];
            for (instr_idx, instr) in block.instructions.iter().enumerate() {
                use crate::metadata::Instruction;
                let site_idx = match instr {
                    Instruction::Call(insn) => {
                        let s = method_site_names.len() as u32;
                        method_site_names.push(insn.func.clone());
                        s
                    }
                    Instruction::Builtin(insn) => {
                        let s = builtin_site_names.len() as u32;
                        builtin_site_names.push(insn.name.clone());
                        s
                    }
                    Instruction::ObjNew(insn) => {
                        let s = type_site_names.len() as u32;
                        type_site_names.push(insn.class_name.clone());
                        s
                    }
                    Instruction::VCall(_) => {
                        let s = vcall_site_count;
                        vcall_site_count += 1;
                        s
                    }
                    Instruction::FieldGet(_) | Instruction::FieldSet(_) => {
                        let s = field_site_count;
                        field_site_count += 1;
                        s
                    }
                    Instruction::StaticGet(insn) => {
                        let s = static_site_names.len() as u32;
                        static_site_names.push(insn.field.clone());
                        s
                    }
                    Instruction::StaticSet(insn) => {
                        let s = static_site_names.len() as u32;
                        static_site_names.push(insn.field.clone());
                        s
                    }
                    _ => UNRESOLVED, // non-token-bearing instruction
                };
                block_sites[instr_idx] = site_idx;
            }
            site_index.push(block_sites);
        }

        // ─── Pass 2: resolve names → tokens ───────────────────────────────
        let method_tokens: Vec<AtomicU32> = method_site_names.iter()
            .map(|name| AtomicU32::new(
                module.func_index.get(name).copied()
                    .map(|idx| idx as u32)
                    .unwrap_or(UNRESOLVED)
            ))
            .collect();

        // Parallel cross-zpkg target cache: one empty cell per Call site,
        // filled on first cross-zpkg dispatch (review.md C7). Intra-module
        // sites resolve via `method_tokens` and leave their cell untouched.
        let cross_module_targets: Vec<OnceLock<Arc<Function>>> =
            method_site_names.iter().map(|_| OnceLock::new()).collect();
        // make-vm-loading-lazy: per-Call-site JIT id cache (see field docs).
        let call_jit_ic: Vec<AtomicU32> =
            method_site_names.iter().map(|_| AtomicU32::new(UNRESOLVED)).collect();

        let builtin_tokens: Vec<u32> = builtin_site_names.iter()
            .map(|name| {
                // Static `BUILTINS[]` first, then per-VM ext registry (populated by
                // `native::ext::load_all` at VM startup). add-z42-compression
                // (2026-05-22): facade `[Native(lib="z42_compression", entry=...)]`
                // names resolve through the ext path.
                {
                    let bid = crate::corelib::builtin_id_of(name);
                    #[cfg(feature = "native-interop")]
                    let bid = bid.or_else(|| crate::corelib::ext_builtin_id_of(ctx, name));
                    bid.unwrap_or_else(|| panic!(
                        "unknown builtin `{}` (typo? not in BUILTINS table or any \
                         dlopened native extension?)",
                        name
                    ))
                }.0
            })
            .collect();

        let type_tokens: Vec<AtomicU32> = type_site_names.iter()
            .map(|name| AtomicU32::new(
                module.type_registry.get(name)
                    .map(|td| td.id.0)
                    .unwrap_or(UNRESOLVED)
            ))
            .collect();

        let vcall_ic: Vec<VCallIC> = (0..vcall_site_count).map(|_| VCallIC::default()).collect();
        let field_ic: Vec<FieldIC> = (0..field_site_count).map(|_| FieldIC::default()).collect();

        // Static fields: lazy allocate through the VmContext so cross-zpkg
        // ordering doesn't matter. Resolution is "always succeed" — if the
        // name was first seen in this module, this is the allocation site.
        let static_field_tokens: Vec<AtomicU32> = static_site_names.iter()
            .map(|name| AtomicU32::new(ctx.resolve_static_field_id(name).0))
            .collect();

        let resolved = ResolvedTokens {
            method_tokens,
            cross_module_targets,
            call_jit_ic,
            builtin_tokens,
            type_tokens,
            vcall_ic,
            field_ic,
            static_field_tokens,
            site_index,
        };

        // OnceLock idempotent set — Err means already set (race or repeat call).
        let _ = func.resolved.set(resolved);
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;

    #[test]
    fn resolved_tokens_default_is_empty() {
        let r = ResolvedTokens::default();
        assert!(r.method_tokens.is_empty());
        assert!(r.cross_module_targets.is_empty());
        assert!(r.builtin_tokens.is_empty());
        assert!(r.type_tokens.is_empty());
        assert!(r.vcall_ic.is_empty());
        assert!(r.field_ic.is_empty());
        assert!(r.static_field_tokens.is_empty());
        assert!(r.site_index.is_empty());
    }

    /// review.md C7 / cache-cross-zpkg-call-target: the per-site cross-zpkg
    /// cell contract that `exec_call::call` relies on — empty until first
    /// dispatch, write-once fill, borrow-after returns the same `Arc`, and a
    /// concurrent/repeat `set` is ignored (so a winner's target stays stable).
    #[test]
    fn cross_module_target_cell_fill_once_then_borrow() {
        use crate::metadata::bytecode::{BasicBlock, Terminator};
        use crate::metadata::types::ExecMode;

        let mk = |name: &str| {
            Arc::new(Function {
                name: name.to_string(),
                param_count: 0,
                ret_type: "void".to_string(),
                exec_mode: ExecMode::Interp,
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    instructions: Vec::new(),
                    terminator: Terminator::Ret { reg: None },
                }],
                is_static: false,
                visibility: 0,
                method_flags: 0, min_arg: 0, params_from: 0xFF,
                max_reg: 0,
                cold: None,
                reg_types: Box::new([]),
                block_index: std::collections::HashMap::new(),
                branch_targets: Vec::new(),
                fused_tails: Vec::new(),
                frame_meta: None,
                resolved: OnceLock::new(),
            })
        };

        let cell: OnceLock<Arc<Function>> = OnceLock::new();
        assert!(cell.get().is_none(), "fresh cell must be empty (forces first-dispatch resolve)");

        let first = mk("Other.zpkg.fn");
        assert!(cell.set(Arc::clone(&first)).is_ok(), "first fill succeeds");
        assert!(Arc::ptr_eq(cell.get().unwrap(), &first), "borrow returns the cached Arc, no re-resolve");

        // A second resolve (e.g. concurrent double-fill) must not replace the
        // cached target — set() returns Err and the original Arc stays.
        let second = mk("Other.zpkg.fn");
        assert!(cell.set(Arc::clone(&second)).is_err(), "repeat fill is rejected (write-once)");
        assert!(Arc::ptr_eq(cell.get().unwrap(), &first), "cached target unchanged after rejected fill");
    }

    #[test]
    fn vcall_ic_default_all_slots_unresolved() {
        let ic = VCallIC::default();
        use std::sync::atomic::Ordering;
        for entry in &ic.entries {
            assert_eq!(entry.type_id.load(Ordering::Relaxed), UNRESOLVED);
            assert_eq!(entry.slot.load(Ordering::Relaxed), UNRESOLVED);
            assert_eq!(entry.fn_idx.load(Ordering::Relaxed), UNRESOLVED);
        }
        assert_eq!(ic.round_robin.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn field_ic_default_all_slots_unresolved() {
        let ic = FieldIC::default();
        use std::sync::atomic::Ordering;
        for entry in &ic.entries {
            assert_eq!(entry.type_id.load(Ordering::Relaxed), UNRESOLVED);
            assert_eq!(entry.slot.load(Ordering::Relaxed), UNRESOLVED);
        }
        assert_eq!(ic.round_robin.load(Ordering::Relaxed), 0);
    }

    // ── PIC lookup + install (review.md C4 P2 + C5 P2) ─────────────────

    #[test]
    fn field_ic_mono_hit() {
        let ic = FieldIC::default();
        field_ic_install(&ic, 1, 7);
        assert_eq!(field_ic_lookup(&ic, 1), Some(7));
    }

    #[test]
    fn field_ic_poly_two_types_both_hit() {
        let ic = FieldIC::default();
        field_ic_install(&ic, 1, 7);
        field_ic_install(&ic, 2, 9);
        assert_eq!(field_ic_lookup(&ic, 1), Some(7));
        assert_eq!(field_ic_lookup(&ic, 2), Some(9));
    }

    #[test]
    fn field_ic_poly_four_types_all_hit() {
        let ic = FieldIC::default();
        for t in 1..=4 { field_ic_install(&ic, t, t * 10); }
        for t in 1..=4 { assert_eq!(field_ic_lookup(&ic, t), Some(t * 10)); }
    }

    #[test]
    fn field_ic_megamorphic_evicts_via_round_robin() {
        let ic = FieldIC::default();
        for t in 1..=4 { field_ic_install(&ic, t, t * 10); }
        // 5th type triggers round-robin eviction (victim = slot 0).
        field_ic_install(&ic, 5, 50);
        assert_eq!(field_ic_lookup(&ic, 5), Some(50));
        // Three of the original four still present; one (slot 0) replaced.
        let remaining_hits: usize = (1..=4)
            .filter(|t| field_ic_lookup(&ic, *t).is_some())
            .count();
        assert_eq!(remaining_hits, 3, "round-robin victimizes exactly one slot");
    }

    #[test]
    fn field_ic_unresolved_recv_type_returns_none() {
        let ic = FieldIC::default();
        field_ic_install(&ic, 1, 7);
        assert_eq!(field_ic_lookup(&ic, UNRESOLVED), None);
    }

    #[test]
    fn field_ic_install_unresolved_is_noop() {
        let ic = FieldIC::default();
        field_ic_install(&ic, UNRESOLVED, 7);
        // Should not poison the first slot — subsequent install must still hit it.
        field_ic_install(&ic, 1, 9);
        assert_eq!(field_ic_lookup(&ic, 1), Some(9));
    }

    #[test]
    fn vcall_ic_mono_hit() {
        let ic = VCallIC::default();
        vcall_ic_install(&ic, 1, 2, 100);
        assert_eq!(vcall_ic_lookup(&ic, 1), Some((2, 100)));
    }

    #[test]
    fn vcall_ic_poly_four_types() {
        let ic = VCallIC::default();
        for t in 1..=4 { vcall_ic_install(&ic, t, t, t * 100); }
        for t in 1..=4 { assert_eq!(vcall_ic_lookup(&ic, t), Some((t, t * 100))); }
    }

    #[test]
    fn ic_reinstall_same_type_updates_slot_in_place() {
        let ic = FieldIC::default();
        field_ic_install(&ic, 1, 7);
        // Reinstall with the same type should not consume another slot.
        field_ic_install(&ic, 1, 99);
        assert_eq!(field_ic_lookup(&ic, 1), Some(99));
        // And the remaining 3 slots should still be UNRESOLVED.
        use std::sync::atomic::Ordering;
        for entry in &ic.entries[1..] {
            assert_eq!(entry.type_id.load(Ordering::Relaxed), UNRESOLVED);
        }
    }
}
