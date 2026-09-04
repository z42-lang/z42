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

mod ic;
pub use ic::{
    field_ic_install, field_ic_lookup, vcall_ic_install, vcall_ic_lookup,
    FieldIC, FieldICEntry, VCallIC, VCallICEntry, IC_SLOTS,
};

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
                    // fix-jit-builtin-ext-fallback: a builtin that resolves to neither
                    // the static `BUILTINS[]` table nor the per-VM ext registry is left
                    // `UNRESOLVED` rather than panicking. This path can now run at JIT
                    // compile time (resolve-before-compile at `jit_threshold==1`), before
                    // an ext facade's native lib is needed/loaded in that VM; a hard panic
                    // there aborts the whole VM. Both consumers fall back to name-based
                    // `corelib::exec_builtin` at the actual call (interp `exec_call::builtin`
                    // / `jit_builtin`), which re-checks the ext registry then — mirroring
                    // interp's long-standing `None => exec_builtin(name)` back-compat path.
                    bid.map(|b| b.0).unwrap_or(crate::metadata::tokens::UNRESOLVED)
                }
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

        // defer-class-initialization (T3): 静态字段引用是「首次主动使用」的一种，
        // 必须触发所属类的初始化。热路径 `static_get_by_id` 无法区分「未初始化」与
        // 「值就是 null」（`Value::Null` 是合法值），故触发点前移到这里——名字在此可得，
        // 且每个名字每模块只走一次。所属类 = 字段 FQN 去掉最后一段。
        // 已在 type registry 里的类说明其所属包早已加载 + 初始化，无需入队。
        for name in &static_site_names {
            let Some((class_fq, _field)) = name.rsplit_once('.') else { continue };
            if ctx.has_loaded_type(class_fq) { continue; }
            ctx.enqueue_type_init(class_fq);
        }

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

        // defer-class-initialization (T3): 排空必须在**发布 `resolved` 之前**。
        // `resolved` 是 `OnceLock`——一旦发布，其它线程就跳过整条解析路径直奔函数体。
        // 若在发布之后才跑初始化器，另一个线程会在初始化完成前读到 Null
        // （cross-zpkg golden `static_init_concurrent` 抓到过：JIT 模式下两个工作线程
        // 同时首次触达同一包，一个读到 `Table` 是 Null）。放在发布前后，
        // 「看见 resolved 已发布」就蕴含「该函数引用的类都已初始化完毕」。
        ctx.run_pending_static_inits();

        // OnceLock idempotent set — Err means already set (race or repeat call).
        let _ = func.resolved.set(resolved);
    }
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod resolver_tests;
