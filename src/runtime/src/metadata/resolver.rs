//! Load-time token resolution for the introduce-method-token spec
//! (Phase 1, 2026-05-08). Walks every IR instruction in a freshly built
//! `Module` and pre-fills per-function `ResolvedTokens` so the dispatch
//! hot path can index `Vec<Function>` / `Vec<Value>` directly without
//! per-call hashing.
//!
//! Only **load-time-knowable** references are resolved here:
//!
//!   • `Call.func`            → `FnId` (every callee already registered in the
//!                              VM's `FuncTable`; the rest filled on first dispatch)
//!   • `Builtin.name`         → `BuiltinId` (closed set — panic on miss)
//!   • `ObjNew` class + ctor  → `ObjNewSite` (class descriptor + ctor `FnId`; whatever is
//!                              already registered, the rest filled on first dispatch)
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

/// Per-function lazy-init cache populated by `resolve_module`. Stored on
/// `Function.resolved: OnceLock<ResolvedTokens>`.
///
/// Layout: each token-kind has its own `Vec` indexed by **per-kind site
/// index** (Call sites are numbered 0..N independently of Builtin sites).
/// `site_index[block_idx][instr_idx]` maps a (block, instruction) location
/// to the appropriate site index for that kind.
#[derive(Debug, Default)]
pub struct ResolvedTokens {
    /// `Call` sites: the callee's `FnId` (P1-2), or `UNRESOLVED` until the first
    /// dispatch binds it. When the function runs against the VM's entry module
    /// (`FuncTable::is_entry` — every production run) the token is an id in
    /// `VmCore.funcs`: entry functions are `0..n` (= their `Module.functions`
    /// index), callees in lazily loaded packages the ids after that. Against
    /// any other module (unit tests with a bare `VmContext::new()`), only that
    /// module's own indices are ever stored — the interp reads a token as
    /// `module.functions[t]` when `t` is in range, else `funcs.get(t)`, and the
    /// two readings agree whenever both exist. Filled here by name for every
    /// callee already registered (including calls inside a lazily loaded
    /// package into itself or into any package loaded so far); the rest are
    /// bound on first dispatch (`interp::exec_call::bind_callee`, which both
    /// backends use). The JIT shares the cells: the token is its slot id
    /// (`JitModuleCtx`), baked into a compiled call site when already bound and
    /// read from the cell at run time otherwise.
    pub method_tokens: Vec<AtomicU32>,
    /// `Builtin` sites: `BuiltinId` resolved at load (closed set —
    /// panic if a builtin name is unknown).
    pub builtin_tokens: Vec<u32>,
    /// `ObjNew` sites (P1-2 PR 5): the resolved class descriptor and ctor `FnId`, plus the
    /// cache-ctorless-objnew mark — see [`ObjNewSite`]. Used only when the function runs
    /// against the VM's entry module; pre-filled here from what is already registered
    /// (the module's type registry, `FuncTable::id_of`), the rest on first dispatch
    /// (`interp::obj_new_resolve`, which both backends use).
    pub obj_new: Vec<ObjNewSite>,
    /// `VCall` sites: polymorphic inline cache, `TypeId` → callee `FnId` (same id space
    /// as `method_tokens`; lazily loaded callees included, P1-2 PR 4).
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
    assert_field_ic_slot,
    ctorless_hit, ctorless_note, fn_registration_mark, note_fn_registration,
    field_ic_install, field_ic_lookup, vcall_ic_install, vcall_ic_lookup,
    FieldIC, FieldICEntry, ObjNewSite, VCallIC, VCallICEntry, IC_SLOTS,
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
/// entry). `method_tokens` are read against `module` (see the field doc) and `obj_new`
/// is only pre-filled for the entry module; resolving against a *different*
/// module would mint wrong targets. Callees in packages not loaded yet resolve
/// to `UNRESOLVED` here and are bound on first dispatch.
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
        // fix-call-arity-skew: parallel to `method_site_names` — the physical argument
        // count each `Call` site passes (receiver + args + sret slot, exactly as emitted).
        let mut method_site_argc:    Vec<usize>  = Vec::new();
        let mut builtin_site_names:  Vec<String> = Vec::new();
        // ObjNew sites: (class name, ctor name, physical argc incl. `this`).
        let mut obj_new_sites:       Vec<(&str, &str, usize)> = Vec::new();
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
                        method_site_argc.push(insn.args.len());
                        s
                    }
                    Instruction::Builtin(insn) => {
                        let s = builtin_site_names.len() as u32;
                        builtin_site_names.push(insn.name.clone());
                        s
                    }
                    Instruction::ObjNew(insn) => {
                        let s = obj_new_sites.len() as u32;
                        obj_new_sites.push((&insn.class_name, &insn.ctor_name, insn.args.len() + 1));
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
        // fix-call-arity-skew: a site whose argument count the bound function's signature
        // cannot take is **not** pre-filled. This is the merged-module path — and `z42.core`
        // is eagerly merged into the main module, so this is where a skew against the stdlib
        // (compiled against one version, running another) would otherwise be baked in
        // silently. Leaving it `UNRESOLVED` sends both backends (JIT tier 1 reads these
        // same tokens) to the cold path, which re-resolves and throws there. A load-time
        // pass over every site, once — the per-call hot path is untouched.
        //
        // P1-2: names resolve through the VM's `FuncTable` when `module` is its entry module —
        // entry module first, then every lazily loaded function registered so far (`id_of`, the
        // same precedence as the call path's cold resolve). That is what lets a lazily loaded
        // package's calls into itself start out bound. Same signature check as the call path's
        // first binding (`exec_call::call`): a callee that cannot take this site's argument count
        // stays `UNRESOLVED` so the call path raises there. Lookups only — nothing is loaded here.
        let funcs = ctx.funcs();
        let by_fn_id = funcs.is_entry(module);
        let method_tokens: Vec<AtomicU32> = method_site_names.iter().zip(method_site_argc.iter())
            .map(|(name, &argc)| {
                let bound = if by_fn_id {
                    funcs.id_of(name).and_then(|id| Some((id.0, funcs.get(id)?)))
                } else {
                    module.func_index.get(name)
                        .and_then(|&idx| Some((idx as u32, module.functions.get(idx)?)))
                };
                AtomicU32::new(match bound {
                    Some((tok, f)) if crate::vm_context::symres::call_arity(f).accepts(argc) => tok,
                    _ => UNRESOLVED,
                })
            })
            .collect();

        let builtin_tokens: Vec<u32> = builtin_site_names.iter()
            .map(|name| {
                // Static `BUILTINS[]` first, then per-VM ext registry (a miss loads
                // the providing library, `native::ext::ensure_lib_for`). add-z42-compression
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

        // P1-2 PR 5: ObjNew sites start out bound to whatever the first dispatch would pick
        // from what is registered now — entry module only (the cache is read only there).
        // Class: the module registry's descriptor unless its inheritance is still unmerged
        // (that one goes through the loader on first use). Ctor: `id_of` (entry first, then
        // lazily loaded) with the same arity check as the first binding. Nothing is loaded.
        let obj_new: Vec<ObjNewSite> = obj_new_sites.iter()
            .map(|&(class_name, ctor_name, argc)| {
                let site = ObjNewSite::default();
                if by_fn_id {
                    if let Some(td) = module.type_registry.get(class_name).filter(|td| !td.base_unmerged()) {
                        let _ = site.class.set(td.clone());
                    }
                    let ctor = funcs.id_of(ctor_name).filter(|&id| funcs.get(id)
                        .is_some_and(|f| crate::vm_context::symres::call_arity(f).accepts(argc)));
                    if let Some(id) = ctor {
                        site.ctor.store(id.0, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                site
            })
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
        // defer-class-initialization (T3): 静态字段引用是「首次主动使用」的一种，
        // 所属包必须先加载（类型才登记得上，cctor 屏障才有东西可查）。
        // 已在 type registry 里的类说明其所属包已加载，无需入队。
        //
        // ⚠️ unify-static-init-into-cctor（7.3）：这里**只负责"加载"，不负责"初始化"**。
        // 初始化时机归访问点的屏障（`static_get` 顶部的 `ensure_owner_type_init` 等），
        // 否则会在函数解析期就跑掉类型初始化器，破坏「首次使用前」语义。
        for name in &static_site_names {
            let Some((class_fq, _field)) = name.rsplit_once('.') else { continue };
            if ctx.has_loaded_type(class_fq) { continue; }
            ctx.enqueue_type_init(class_fq);
        }

        let resolved = ResolvedTokens {
            method_tokens,
            builtin_tokens,
            obj_new,
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
