//! Lazy dependency loader (zpkg-based, C# assembly model).
//!
//! The VM eagerly loads only `z42.core` at startup. Other stdlib/third-party
//! zpkgs are loaded on demand when the interpreter encounters a `Call` or
//! `ObjNew` against an undefined function/type whose namespace matches a
//! declared-but-not-loaded zpkg.
//!
//! ## Triggering (Decision 1: strategy C + fallback B)
//!
//!   1. Extract namespace prefix from `func_name` / `class_name`
//!   2. Route to candidate zpkgs whose exported `namespaces` metadata
//!      contains that prefix (precise routing, like C# CLR AssemblyRef →
//!      TypeRef lookup)
//!   3. Fallback: if strategy C matches nothing, iterate every declared-but-
//!      -not-loaded zpkg until the target resolves or the set is exhausted
//!   4. Transitive `ZpkgDep`s are unfolded into the declared set on load
//!      (Decision 4 cycle-safe via pre-insert colouring)
//!
//! Multiple zpkgs may legitimately declare the same namespace — the lookup
//! visits them one by one and `first-wins` on function/type name collisions
//! (Decision 6).
//!
//! ## State ownership (consolidate-vm-state, 2026-04-28)
//!
//! Previously `LazyLoader` lived in a `thread_local!` slot. Now an instance
//! is owned by `VmContext::lazy_loader`; all `try_lookup_*` /
//! `declared_namespaces` calls go through `VmContext` methods which delegate
//! here. `LazyLoader` itself remains usable directly by tests / advanced
//! embedders.
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use super::bytecode::{Function, Instruction};
use super::loader::{load_artifact, load_artifact_from_bytes, LoadedArtifact};
use super::test_index::LoadedTestEntry;
use super::types::TypeDesc;
use super::namespace_index;

// ── Public API ────────────────────────────────────────────────────────────────

// `ZpkgCandidate` (the namespace→zpkg index entry: file path + exported
// namespaces) and its disk-parse constructors (`build` / `build_in_dirs`) live
// in the stateless `namespace_index` primitive (refactor-metadata-namespace-
// index, runtime_review #6 step 2). Re-exported here so existing
// `lazy_loader::ZpkgCandidate` paths (app.rs, vm_context, tests, and the
// `metadata::ZpkgCandidate` facade) are unaffected. The lazy loader is the
// *retaining* consumer: it keeps candidates in `declared_zpkgs` and owns the
// load/track/release lifecycle; parsing stays in the primitive.
pub use super::namespace_index::ZpkgCandidate;

// ── State ─────────────────────────────────────────────────────────────────────

/// Lazy-loaded dependency state. Owned by `VmContext::lazy_loader`.
pub struct LazyLoader {
    /// Directories searched (in order) to resolve a transitive dep zpkg by
    /// file name. Typically `[entry-zpkg-dir, stdlib-libs-dir]` so an apphost's
    /// colocated package deps resolve alongside the stdlib
    /// (support-colocated-zpkg-deps, 2026-06-20).
    search_dirs:    Vec<PathBuf>,
    /// Length of the main (user) module's string pool.
    /// ConstStr indices < `main_pool_len` resolve against the main module's
    /// pool; indices >= `main_pool_len` resolve against `string_pool` below
    /// at relative offset `idx - main_pool_len`.
    main_pool_len:  usize,
    /// Aggregated string pool from all lazy-loaded zpkgs.
    string_pool:    Vec<String>,

    /// zpkg file names that have been loaded (either eagerly at startup or
    /// by a previous lazy-load). Used for de-duplication and cycle-cutting
    /// (Decision 4: pre-inserted before load to break cycles).
    pub(crate) loaded_zpkgs:   FxHashSet<String>,
    /// Scratch buffer: zpkg file names newly inserted into `loaded_zpkgs` during
    /// the current resolve. Drained (`mem::take`) by `VmContext::try_lookup_*`
    /// to fire `ModuleLoaded` events — replaces cloning the whole `loaded_zpkgs`
    /// set on every lookup just to diff it afterwards (profiled: that per-call
    /// clone was the top interp alloc hotspot in cross-zpkg-heavy workloads like
    /// z42c self-compile). Common path (no new load) leaves it empty → zero alloc.
    pub(crate) newly_loaded:   Vec<String>,
    /// defer-class-initialization: `*.__static_init__` 名字，随包懒加载入队，由
    /// [`VmContext::run_pending_static_inits`] 在**放锁后**执行。取代旧的
    /// boot 期 `force_load_all_declared()` 全量加载枚举。
    pub(crate) pending_static_inits: Vec<String>,
    /// defer-class-initialization: 每个 `__static_init__` 的执行状态。
    /// `Running(tid)` = 某线程正在跑（同线程重入直接跳过 = CLR 循环类型初始化器语义；
    /// 他线程需等到 `Done` 再读该包静态字段）；`Done` = 已跑完。
    pub(crate) static_init_state: FxHashMap<String, InitState>,
    /// zpkg file names that are declared as dependencies (direct or
    /// transitive) but have not yet been loaded. Lookup candidates.
    pub(crate) declared_zpkgs: FxHashMap<String, ZpkgCandidate>,

    /// Functions loaded from lazily-resolved zpkgs, indexed by FQ name.
    /// ConstStr indices have been remapped to absolute indices.
    function_table: FxHashMap<String, Arc<Function>>,
    /// Type descriptors from lazily-resolved zpkgs.
    type_registry:  FxHashMap<String, Arc<TypeDesc>>,
    /// add-crosspkg-impl-reflection (unify P1-e): `target_fq → [trait_fq]`
    /// aggregated from every loaded zpkg's IMPL section (`impl Trait for Type`).
    /// Appended (not first-wins): distinct packages may each impl different
    /// traits for the same target. Backs `Type.GetInterfaces()` seeing
    /// cross-package traits; only loaded packages contribute (an unloaded
    /// package's impl methods aren't callable either — consistent).
    impls:          FxHashMap<String, Vec<String>>,

    /// **cache-failed-name-resolution**: names whose *full* resolve came back empty.
    ///
    /// Why this matters: a **ctor-less class** makes `new Foo()` look up
    /// `Foo..ctor$0`, which does not exist — both `interp::obj_new` and
    /// `jit_obj_new` therefore paid a full failed resolve (`format!` +
    /// `candidates_for_namespace` scan + `remaining_declared` scan + sort)
    /// **on every single allocation**, and `jit_obj_new` paid it twice.
    ///
    /// Lazily allocated (`None` until the first recorded miss) so a program that
    /// never misses pays nothing — same rule as per-`VmContext` caches.
    negative: Option<Box<NegativeResolveCache>>,

}

/// cache-failed-name-resolution: the negative resolve cache, held behind a `Box`
/// so `LazyLoader` itself stays the size it was. Valid only while
/// `LazyLoader::registry_fingerprint` is unchanged.
#[derive(Default)]
struct NegativeResolveCache {
    functions:   FxHashSet<String>,
    types:       FxHashSet<String>,
    fingerprint: (usize, usize, usize, usize),
}

impl LazyLoader {
    /// Seed the lazy loader's type_registry with TypeDescs from eagerly-loaded
    /// modules (e.g. the merged main module containing z42.core + user code).
    /// The cross-zpkg fixup pass (`try_fixup_inheritance`) consults this
    /// registry when a later-loaded subclass needs to find its base class
    /// living in an eagerly-loaded module. (fix-cross-pkg-subclass-fields,
    /// 2026-05-14)
    ///
    /// Arcs are shared with the source module — that bumps strong_count to 2
    /// for these entries. Normally that's fine: eagerly-loaded types are
    /// already fully merged (`build_type_registry` ran on the combined module
    /// and resolved their inheritance), so `needs_fixup` returns false and the
    /// fixup pass never touches them. Only later-arriving lazy-loaded TypeDescs
    /// (strong_count = 1) are the usual mutation targets.
    ///
    /// The exception (fix-projecthooks-vtable-fixup): an eagerly-loaded type can
    /// still be OWN-ONLY when its base lives in a zpkg that is only LAZILY loaded
    /// (e.g. a `ProjectHooks : BuildHooks` compiled into an app whose `z42.build`
    /// dep isn't statically linked). Such a type is seeded here (strong_count ≥ 2)
    /// AND `needs_fixup` is true. `try_fixup_inheritance` handles this with
    /// `Arc::make_mut` (clone-on-write): it gives THIS registry a private, merged
    /// copy so the lazy-lookup path resolves the full vtable/fields, while the
    /// eager source module keeps its own-only copy (unaffected).
    pub fn seed_types_for_lookup(&mut self, types: &FxHashMap<String, Arc<TypeDesc>>) {
        for (name, td) in types {
            if !self.type_registry.contains_key(name) {
                self.type_registry.insert(name.clone(), Arc::clone(td));
            }
        }
    }

    /// add-crosspkg-impl-reflection: merge `(target_fq, trait_fq)` pairs into
    /// the impls registry (from the eagerly-loaded main artifact or a
    /// lazily-loaded zpkg). Duplicate pairs are dropped; distinct traits for
    /// the same target accumulate.
    pub fn seed_impls(&mut self, pairs: &[(String, String)]) {
        for (target, tr) in pairs {
            let traits = self.impls.entry(target.clone()).or_default();
            if !traits.iter().any(|t| t == tr) {
                traits.push(tr.clone());
            }
        }
    }

    /// add-crosspkg-impl-reflection: traits added to `target_fq` via
    /// cross-package `impl Trait for Type`, from loaded packages only.
    pub fn impl_traits_for(&self, target_fq: &str) -> &[String] {
        self.impls.get(target_fq).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn new(
        search_dirs: Vec<PathBuf>,
        main_pool_len: usize,
        declared: Vec<(String, ZpkgCandidate)>,
        initially_loaded: Vec<String>,
    ) -> Self {
        let loaded_zpkgs: FxHashSet<String> = initially_loaded.into_iter().collect();
        let declared_zpkgs: FxHashMap<String, ZpkgCandidate> = declared
            .into_iter()
            .filter(|(k, _)| !loaded_zpkgs.contains(k))
            .collect();
        Self {
            search_dirs,
            main_pool_len,
            string_pool:    Vec::new(),
            loaded_zpkgs,
            newly_loaded:   Vec::new(),
            pending_static_inits: Vec::new(),
            static_init_state: FxHashMap::default(),
            declared_zpkgs,
            function_table: FxHashMap::default(),
            type_registry:  FxHashMap::default(),
            impls:          FxHashMap::default(),
            negative: None,
        }
    }

    /// Insert `name` into `loaded_zpkgs`; if it was genuinely new (not already
    /// resident), also record it in `newly_loaded` so a `try_lookup_*` caller can
    /// fire `ModuleLoaded` without cloning + diffing the whole set. Mirrors the
    /// old `loaded_zpkgs.difference(&before)` semantics (only genuinely-new keys).
    fn mark_zpkg_loaded(&mut self, name: String) {
        if self.loaded_zpkgs.insert(name.clone()) {
            self.newly_loaded.push(name);
        }
    }

    /// cache-ctorless-objnew: **the single funnel** for growing `function_table`.
    /// Every insert bumps the global registration mark, which is what makes
    /// "no function has been registered since" a sound guard for a cached negative
    /// answer. Returns `false` when the name was already present (first-wins).
    pub(crate) fn insert_function(&mut self, name: String, f: Arc<Function>) -> bool {
        if self.function_table.contains_key(&name) {
            return false;
        }
        self.function_table.insert(name, f);
        crate::metadata::resolver::note_fn_registration();
        true
    }

    /// cache-failed-name-resolution: cheap staleness key for the negative cache.
    /// `loaded_zpkgs` / `declared_zpkgs` / `function_table` / `type_registry` are
    /// **append-only** (grep: no `remove` / `clear` / `retain` on any of them), so
    /// an unchanged length tuple proves no name that previously failed to resolve
    /// could have become resolvable.
    #[inline]
    fn registry_fingerprint(&self) -> (usize, usize, usize, usize) {
        (self.loaded_zpkgs.len(), self.declared_zpkgs.len(),
         self.function_table.len(), self.type_registry.len())
    }

    /// The live negative cache, or `None` when nothing has ever been recorded
    /// (so a program that never misses pays no allocation and no scan).
    #[inline]
    fn negative_cache(&mut self) -> Option<&mut NegativeResolveCache> {
        let fp = self.registry_fingerprint();
        let neg = self.negative.as_mut()?;
        if neg.fingerprint != fp {
            neg.functions.clear();
            neg.types.clear();
            neg.fingerprint = fp;
        }
        Some(neg)
    }

    /// cache-failed-name-resolution: `true` when `name` is a known-unresolvable
    /// function under the *current* registry state.
    #[inline]
    fn is_known_unresolved_function(&mut self, name: &str) -> bool {
        self.negative_cache().is_some_and(|n| n.functions.contains(name))
    }

    /// cache-failed-name-resolution: same, for types.
    #[inline]
    fn is_known_unresolved_type(&mut self, name: &str) -> bool {
        self.negative_cache().is_some_and(|n| n.types.contains(name))
    }

    /// fix-negative-cache-under-read-lock：负缓存的**只读**版本。
    ///
    /// `is_known_unresolved_*` 要 `&mut` 只是因为 `negative_cache()` 顺手做惰性失效
    /// （fingerprint 变了就清空）。而「fingerprint 未变 **且** 名字在集合里」本身是纯读
    /// 判断 —— 于是可以在**读锁**下回答「这个名字在当前 registry 状态下必然解析不出来」。
    ///
    /// 保守且与写路径同义：fingerprint 一旦变过就返回 false，落回写锁走原路（那里会清缓存）。
    /// 清理仍然只发生在写路径，读侧不欠任何维护。
    ///
    /// 为什么值得：`probe_*` 在 registry miss 时返回 `None`，调用方就去拿**独占**锁，
    /// 而 `resolve_*` 的第一件事却是查这个负缓存、直接返回 `None` —— 一次纯记忆化的「否」
    /// 付了一次独占锁，把所有读者堵住。而这条路极热（`resolve_type` 注释：`obj_new` 每次
    /// 分配都要探一次类名；`resolve_function` 注释：无构造函数类合成的 `..ctor$0` 每次
    /// `new` 都查一次）。多线程下这是主要的读者阻塞源（`--jobs 16` 原生采样：
    /// `try_lookup_type` 独占等待 945 样本）。
    #[inline]
    pub(crate) fn known_unresolved_function_ro(&self, name: &str) -> bool {
        self.negative.as_ref().is_some_and(|n| {
            n.fingerprint == self.registry_fingerprint() && n.functions.contains(name)
        })
    }

    /// 同上，类型版。
    #[inline]
    pub(crate) fn known_unresolved_type_ro(&self, name: &str) -> bool {
        self.negative.as_ref().is_some_and(|n| {
            n.fingerprint == self.registry_fingerprint() && n.types.contains(name)
        })
    }

    /// Record a failed function resolve. The fingerprint is re-read here (not
    /// reused from the probe) because the resolve itself may have loaded zpkgs.
    fn note_unresolved_function(&mut self, name: &str) {
        let fp = self.registry_fingerprint();
        let neg = self.negative.get_or_insert_with(Default::default);
        if neg.fingerprint != fp { neg.functions.clear(); neg.types.clear(); neg.fingerprint = fp; }
        neg.functions.insert(name.to_string());
    }

    /// Record a failed type resolve.
    fn note_unresolved_type(&mut self, name: &str) {
        let fp = self.registry_fingerprint();
        let neg = self.negative.get_or_insert_with(Default::default);
        if neg.fingerprint != fp { neg.functions.clear(); neg.types.clear(); neg.fingerprint = fp; }
        neg.types.insert(name.to_string());
    }

    // 解析路径（`resolve.rs`）与注册/加载路径（`registry.rs`）是本类型的两个 impl 块。
}

mod registry;
mod resolve;
pub(crate) use resolve::{namespace_prefix, InitState};
#[cfg(test)]
use resolve::is_primitive_keyword_name;

impl LazyLoader {
    /// Resolve an "overflow" ConstStr index — one that falls past the main
    /// module's string pool. Returns the merged lazy-pool string if available.
    ///
    /// review.md C3 / Part 5 P3 Phase 1 (2026-06-03,
    /// add-string-literal-interning-phase1): returns `Arc<str>` instead of
    /// `String` so callers (interp / JIT ConstStr) can wrap directly into
    /// `Value::Str` without a second `.into::<Arc<str>>()` allocation. The
    /// underlying `String` is converted on each call; overflow-pool literals
    /// are (unlike main-pool literals) not per-context interned, so this
    /// re-allocates a fresh GC string each time (cold path).
    pub fn try_lookup_string(&self, absolute_idx: usize) -> Option<crate::metadata::vstr::Str> {
        let rel = absolute_idx.checked_sub(self.main_pool_len)?;
        self.string_pool.get(rel).map(|s| crate::metadata::vstr::Str::from(s.as_str()))
    }

    /// Returns all namespaces declared by lazy-loadable zpkgs (both already
    /// loaded and not-yet-loaded). Used by `run_with_static_init` to discover
    /// `<ns>.__static_init__` functions in imported stdlib modules.
    ///
    /// 2026-04-27 fix-static-field-access: 没这个 API 之前，VM 启动时只跑
    /// 主模块的 __static_init__，导入 zpkg（如 z42.math）的常量字段（PI / E /
    /// Tau）永远不被赋值 → `Math.PI` 返回 null。
    pub fn declared_namespaces(&self) -> Vec<String> {
        let mut all: Vec<String> = self.declared_zpkgs.values()
            .flat_map(|c| c.namespaces.iter().cloned())
            .collect();
        all.sort();
        all.dedup();
        all
    }

    /// Return zpkg file names from `declared_zpkgs` whose exported namespaces
    /// cover `ns` (exact match or a descendant like `Std.Collections.Generic`
    /// covering a query for `Std.Collections`).
    pub(crate) fn candidates_for_namespace(&self, ns: &str) -> Vec<String> {
        let ns_dot = format!("{ns}.");
        // common-pitfalls.md §1: `declared_zpkgs` is an FxHashMap (non-deterministic
        // iteration order); downstream consumers pick candidates first-wins, so sort
        // by file name for a stable, platform-independent result.
        let mut out: Vec<String> = self.declared_zpkgs
            .iter()
            .filter(|(file, cand)| {
                !self.loaded_zpkgs.contains(file.as_str())
                    && cand.namespaces.iter().any(|n| n == ns || n.starts_with(&ns_dot))
            })
            .map(|(file, _)| file.clone())
            .collect();
        out.sort();
        out
    }

    /// Force-load every not-yet-loaded declared package. One-time eager load used
    /// by reflection (`make_type_from_name`) when a dotless class name can't be
    /// resolved from already-loaded types — a constructed-generic field type_tag
    /// carries source spelling (`List<int>`), not an FQN, and there is no
    /// simple→FQN index without loading bodies. After this, `remaining_declared()`
    /// is empty so repeat calls are no-ops. add-collection-serde.
    pub fn force_load_all(&mut self) {
        for f in self.remaining_declared() {
            let _ = self.load_zpkg_file(&f);
        }
    }

    pub(crate) fn remaining_declared(&self) -> Vec<String> {
        // common-pitfalls.md §1: sort for deterministic force-load order across
        // platforms (FxHashMap iteration order is otherwise non-deterministic).
        let mut out: Vec<String> = self.declared_zpkgs
            .keys()
            .filter(|f| !self.loaded_zpkgs.contains(f.as_str()))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// Force-load every declared zpkg into `function_table` / `type_registry`.
    /// Used by `init_static_fields` to enumerate all `*.__static_init__`
    /// functions before running them. fix-multi-file-static-init (2026-05-15).
    ///
    /// fix-array-default-init (2026-05-19): loop until the declared set is
    /// stable. `load_zpkg_file` adds *transitive* deps to `declared_zpkgs`
    /// as it loads each artifact; a single snapshot misses zpkgs that only
    /// become declared mid-loop. The previous single-pass version skipped
    /// `__static_init__` for transitively-discovered zpkgs (e.g. z42.encoding
    /// pulled in by z42.crypto when the user module only imports Std.Crypto),
    /// leaving their static fields at `Null` (`Hex.ALPHA_LOWER` etc.) and
    /// crashing the first downstream `.CharAt` with `expected string, got Null`.
    pub fn force_load_all_declared(&mut self) {
        loop {
            let zpkgs: Vec<String> = self.remaining_declared();
            if zpkgs.is_empty() { break; }
            for zpkg in zpkgs {
                // Errors here surface later when a function lookup fails; keep
                // initialisation best-effort to mirror the previous lookup path.
                let _ = self.load_zpkg_file(&zpkg);
            }
        }
    }

    /// Iterator over all currently-loaded function names.
    pub fn iter_function_names(&self) -> impl Iterator<Item = &String> + '_ {
        self.function_table.keys()
    }

    /// Iterator over all currently-loaded type names (add-nested-types: used by
    /// `GetNestedTypes` to find `<outer>+<simple>` members among loaded types).
    pub fn iter_type_names(&self) -> impl Iterator<Item = &String> + '_ {
        self.type_registry.keys()
    }

}

#[cfg(test)]
#[path = "lazy_loader_tests.rs"]
mod lazy_loader_tests;
