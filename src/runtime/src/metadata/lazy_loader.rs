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
    /// for these entries, so `Arc::get_mut` will not succeed on them. That's
    /// the desired behavior: eagerly-loaded types are already fully merged
    /// (`build_type_registry` ran on the combined module and resolved their
    /// inheritance), so `needs_fixup` returns false and no mutation is
    /// attempted. Only later-arriving lazy-loaded TypeDescs (which have
    /// strong_count = 1 in this registry) are mutable targets.
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
            declared_zpkgs,
            function_table: FxHashMap::default(),
            type_registry:  FxHashMap::default(),
            impls:          FxHashMap::default(),
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

    /// Look up a function by FQ name; triggers lazy load if needed.
    pub fn resolve_function(&mut self, func_name: &str) -> Option<Arc<Function>> {
        if let Some(f) = self.function_table.get(func_name) {
            return Some(Arc::clone(f));
        }
        // Strategy C: precise routing by namespace prefix
        if let Some(ns) = namespace_prefix(func_name) {
            for zpkg_file in self.candidates_for_namespace(&ns) {
                let _ = self.load_zpkg_file(&zpkg_file);
                if let Some(f) = self.function_table.get(func_name) {
                    return Some(Arc::clone(f));
                }
            }
        }
        // Fallback B: try every remaining declared-but-not-loaded zpkg
        for zpkg_file in self.remaining_declared() {
            let _ = self.load_zpkg_file(&zpkg_file);
            if let Some(f) = self.function_table.get(func_name) {
                return Some(Arc::clone(f));
            }
        }
        None
    }

    /// Look up a class TypeDesc by FQ name; triggers lazy load if needed.
    /// L3-G4d: also triggers the zpkg load for the owning namespace so the
    /// first `new Stack<int>()` on an imported generic class resolves.
    pub fn resolve_type(&mut self, class_name: &str) -> Option<Arc<TypeDesc>> {
        if let Some(td) = self.type_registry.get(class_name) {
            return Some(Arc::clone(td));
        }
        // Strategy C: use the class's enclosing namespace (strip last segment)
        if let Some((ns, _)) = class_name.rsplit_once('.') {
            for zpkg_file in self.candidates_for_namespace(ns) {
                let _ = self.load_zpkg_file(&zpkg_file);
                if let Some(td) = self.type_registry.get(class_name) {
                    return Some(Arc::clone(td));
                }
            }
        }
        for zpkg_file in self.remaining_declared() {
            let _ = self.load_zpkg_file(&zpkg_file);
            if let Some(td) = self.type_registry.get(class_name) {
                return Some(Arc::clone(td));
            }
        }
        None
    }

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

    /// Load a zpkg file, merge its functions / types / strings, and expand
    /// its own `ZpkgDep` list into `declared_zpkgs` for future transitive
    /// lookups.
    ///
    /// Cycle-safe: inserts into `loaded_zpkgs` **before** loading the
    /// artifact, so a re-entrant call (A depends on B, B depends on A)
    /// returns immediately on the second visit.
    pub(crate) fn load_zpkg_file(&mut self, file_name: &str) -> Result<()> {
        if self.loaded_zpkgs.contains(file_name) {
            return Ok(());
        }
        let file_path = match self.declared_zpkgs.get(file_name) {
            Some(c) => c.file_path.clone(),
            None    => return Ok(()),
        };

        // Decision 4: pre-insert to break cycles before recursive dep expansion.
        self.mark_zpkg_loaded(file_name.to_string());

        let path_str = file_path.to_string_lossy().into_owned();
        let mut artifact = load_artifact(&path_str)?;

        // add-crosspkg-impl-reflection: register this package's
        // `impl Trait for Type` pairs (backs GetInterfaces cross-pkg traits).
        self.seed_impls(&artifact.impl_pairs);

        let offset = self.main_pool_len + self.string_pool.len();
        self.string_pool.extend(artifact.module.string_pool.iter().cloned());

        // Decision 6: first-wins on function / type name collisions.
        for mut fn_ in artifact.module.functions {
            remap_const_str(&mut fn_, offset);
            let name = fn_.name.clone();
            if self.function_table.contains_key(&name) {
                tracing::warn!(
                    "duplicate function `{name}` from zpkg `{file_name}`; keeping first-loaded"
                );
                continue;
            }
            self.function_table.insert(name, Arc::new(fn_));
        }
        // fix-cross-pkg-subclass-fields (2026-05-14): drop the by-id Vec of
        // TypeDescs BEFORE moving them into `self.type_registry` so each Arc
        // has strong_count = 1 in the lazy_loader's registry. Otherwise the
        // fixup pass below can't use `Arc::get_mut` to mutate inherited
        // field layouts in place.
        artifact.module.type_registry_vec.clear();
        for (name, desc) in std::mem::take(&mut artifact.module.type_registry) {
            if self.type_registry.contains_key(&name) {
                tracing::warn!(
                    "duplicate type `{name}` from zpkg `{file_name}`; keeping first-loaded"
                );
                continue;
            }
            self.type_registry.insert(name, desc);
        }

        // fix-cross-pkg-subclass-fields (2026-05-14): subclasses in this
        // zpkg whose base lives in an already-loaded dep zpkg need a fixup
        // pass to inherit the base's field/vtable layout. The fixed-point
        // loop also retries previously-deferred subclasses (a freshly-loaded
        // dep may unblock subclasses from earlier zpkgs).
        // Fixed-point: each round resolves one more deferred inheritance level,
        // so a well-formed registry converges in at most (max base-chain depth)
        // rounds, bounded by the type count. The `+ 8` slack absorbs interface
        // / multi-level chains. A correct `needs_fixup` (dedup-aware) makes this
        // cap unreachable; it's defense-in-depth so a future metadata defect can
        // never hang the loader at 100% CPU — bail loudly instead.
        let fixup_cap = self.type_registry.len() + 8;
        for round in 0.. {
            let n = crate::metadata::loader::try_fixup_inheritance(&mut self.type_registry);
            if n == 0 { break; }
            if round >= fixup_cap {
                tracing::error!(
                    "inheritance fixup did not converge after {fixup_cap} rounds while \
                     loading `{file_name}` (still fixing {n}/round); likely duplicate \
                     field names or a base-class cycle. Unconverged: {:?}",
                    crate::metadata::loader::unconverged_type_names(&self.type_registry)
                );
                break;
            }
        }

        // Transitively expand `ZpkgDep` list into the declared set. Each dep is
        // resolved across all `search_dirs` (entry-zpkg dir + stdlib libs), so a
        // colocated package dep is found even when it isn't in `libs/`.
        if !self.search_dirs.is_empty() {
            let dirs = self.search_dirs.clone();
            for dep in &artifact.dependencies {
                if self.loaded_zpkgs.contains(&dep.file)
                    || self.declared_zpkgs.contains_key(&dep.file)
                {
                    continue;
                }
                match ZpkgCandidate::build_in_dirs(&dirs, &dep.file) {
                    Ok(cand) => {
                        self.declared_zpkgs.insert(dep.file.clone(), cand);
                    }
                    // A missing transitive dep here is benign: this is an eager
                    // prefetch of the dep closure, not a hard requirement. If the
                    // dep is genuinely needed, the call into it surfaces a clear
                    // `undefined function` at runtime — the real gate. A preemptive
                    // miss can legitimately happen after a package is merged/renamed
                    // (e.g. z42.io.binary → z42.io, whose types still resolve via the
                    // merged package): the stale dep filename lingers in older zpkgs'
                    // dep lists but nothing loads it. Must stay off stdout/stderr —
                    // it otherwise pollutes golden-comparison test output for every
                    // module that transitively references the merged package.
                    Err(e) => tracing::debug!(
                        "transitive dep zpkg `{}` not prefetched (resolves lazily / merged away): {e}",
                        dep.file
                    ),
                }
            }
        }

        tracing::debug!("lazy-loaded zpkg `{file_name}` from {path_str}");
        Ok(())
    }

    /// Load a compiled artifact from an arbitrary **path** (not a declared dep)
    /// and merge its functions / types / strings into the live registries, then
    /// return its TIDX test entries (resolved FQN + kind + flags). Powers
    /// `Std.Test.ModuleLoader.Load` so a z42 test runner can load a compiled test
    /// module and discover + `Invoke` its `[Test]` methods. Mirrors
    /// `load_zpkg_file`'s merge (string-pool offset + `remap_const_str` + first-wins
    /// + inheritance fixup); idempotent per module name. (retire-test-runner)
    pub fn load_module_from_path(&mut self, path: &str) -> Result<Vec<LoadedTestEntry>> {
        // support-colocated-zpkg-deps for mid-run loads: a module loaded by an
        // absolute/foreign path (e.g. z42.scripting's REPL injector loading
        // z42c.pipeline.zpkg from the SDK's programs/z42c/) carries a transitive dep
        // closure (z42c.semantics + siblings) that sits NEXT TO it — outside the VM's
        // startup search_dirs (app entry dir + Z42_LIBS). Add the loaded artifact's
        // own directory to search_dirs so those siblings resolve, mirroring app::run's
        // "entry-zpkg dir first" rule. Appended (lowest priority) + deduped so it never
        // shadows the app dir / Z42_LIBS; harmless when path has no colocated deps.
        if let Some(parent) = std::path::Path::new(path).parent() {
            let dir = if parent.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                parent.to_path_buf()
            };
            let is_dir = dir.to_str()
                .map(|s| crate::corelib::fs_backend::active().is_dir(s))
                .unwrap_or(false);
            if is_dir && !self.search_dirs.iter().any(|d| d == &dir) {
                self.search_dirs.push(dir);
            }
        }
        let artifact = load_artifact(path)?;
        let (entries, _static_inits) = self.register_loaded_artifact(artifact)?;
        Ok(entries)
    }

    /// In-memory sibling of [`load_module_from_path`]: load a compiled artifact
    /// from raw bytes (packed zpkg or bare zbc, detected by magic) and merge it
    /// into the live registries. Backs the `__load_bytecode_in_memory` builtin
    /// used by `z42.scripting` (REPL): `PackageCompile` produces packed zpkg
    /// bytes in memory, which are loaded here with zero disk I/O so the freshly
    /// compiled `$Eval_N()` becomes reflectively invocable. (add-z42-repl)
    /// Returns the freshly-loaded module's own `*.__static_init__` function names,
    /// so the caller runs only THIS round's static init (not a full clear+rerun that
    /// would wipe prior rounds' mutated state — REPL carry-forward).
    pub fn load_module_from_bytes(&mut self, raw: &[u8]) -> Result<Vec<String>> {
        let artifact = load_artifact_from_bytes(raw)?;
        let (_entries, static_inits) = self.register_loaded_artifact(artifact)?;
        Ok(static_inits)
    }

    /// Shared registration body for [`load_module_from_path`] /
    /// [`load_module_from_bytes`]: merge functions / types / string pool,
    /// register dep + namespace candidates, force-load the declared closure,
    /// run inheritance fixup, and return the artifact's TIDX test entries.
    fn register_loaded_artifact(
        &mut self, mut artifact: LoadedArtifact,
    ) -> Result<(Vec<LoadedTestEntry>, Vec<String>)> {
        let mod_key = format!("__loaded_path__{}", artifact.module.name);
        // Names of THIS module's own `*.__static_init__` functions — returned so the
        // caller can run only the freshly-loaded module's static init (without the
        // full clear+rerun of `init_static_fields`, which would wipe already-loaded
        // modules' mutated static state). Backs REPL carry-forward. (add-z42-repl)
        let mut static_inits: Vec<String> = Vec::new();

        // Capture test entries (FQN resolved via functions[method_id]) before the
        // functions are moved into the table.
        let mut entries: Vec<LoadedTestEntry> = Vec::with_capacity(artifact.test_index.len());
        for e in &artifact.test_index {
            let qualified = artifact
                .module
                .functions
                .get(e.method_id as usize)
                .map(|f| f.name.clone())
                .unwrap_or_default();
            entries.push(LoadedTestEntry {
                qualified,
                kind: e.kind as u8,
                flags: e.flags.bits(),
                skip_reason: e.skip_reason.clone(),
                skip_platform: e.skip_platform.clone(),
                skip_feature: e.skip_feature.clone(),
                expected_throw: e.expected_throw_type.clone(),
            });
        }

        // Register the loaded module's dependencies as lazy-loader candidates so
        // cross-zpkg calls FROM the test module — including virtual (instance-
        // method) dispatch into a dep like z42.collections — resolve. A bare
        // `--emit-zbc` test unit carries NO `dependencies` list (only a packed
        // zpkg does), so we must ALSO resolve the test's `import_namespaces`
        // (its `using` clauses, e.g. `Std.Collections`) to candidate zpkgs.
        // Mirrors the dedicated test-runner bootstrap's `build_declared_candidates`
        // (which covered both); without the namespace path a test calling
        // `SortedSet.Add` (a dep's instance method) hits "VCall not found".
        if !self.search_dirs.is_empty() {
            let dirs = self.search_dirs.clone();
            for dep in &artifact.dependencies {
                if self.loaded_zpkgs.contains(&dep.file)
                    || self.declared_zpkgs.contains_key(&dep.file)
                {
                    continue;
                }
                match ZpkgCandidate::build_in_dirs(&dirs, &dep.file) {
                    Ok(cand) => {
                        self.declared_zpkgs.insert(dep.file.clone(), cand);
                    }
                    Err(e) => tracing::warn!(
                        "ModuleLoader: cannot read dep zpkg meta `{}`: {e}", dep.file
                    ),
                }
            }
            // Namespace-derived candidates (covers bare-zbc test units with no
            // `dependencies` list). Scan the search dirs once for zpkg candidates
            // via the stateless `namespace_index` primitive, then register those
            // whose exported namespaces exactly match a `using` clause. Replaces
            // the former reverse call into `loader::resolve_namespace` (which
            // re-scanned the dirs per namespace and then re-read each hit through
            // `build_in_dirs`): one scan, no cross-module dependency, matched
            // candidates retained directly. The stored path is now the dir where
            // the namespace actually matched (was first-dir-by-name) — identical
            // for real single-location zpkgs. (refactor-metadata-namespace-index)
            let scanned = namespace_index::scan_zpkg_candidates(&dirs);
            for ns in &artifact.import_namespaces {
                for cand in &scanned {
                    if !cand.namespaces.iter().any(|n| n == ns) {
                        continue;
                    }
                    let Some(file) = cand.file_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(str::to_owned)
                    else {
                        continue;
                    };
                    if self.loaded_zpkgs.contains(&file)
                        || self.declared_zpkgs.contains_key(&file)
                    {
                        continue;
                    }
                    self.declared_zpkgs.insert(file, cand.clone());
                }
            }
        }

        // Idempotent: a re-load of the same module just returns its entries.
        if self.loaded_zpkgs.contains(&mod_key) {
            return Ok((entries, static_inits));
        }
        self.mark_zpkg_loaded(mod_key);

        // Mark this package's canonical zpkg file name as resident. A later-loaded
        // dependent records deps by file name (`<pkg>.zpkg`), so without this a
        // cross-round REPL reference (R2 uses `Repl.R1.A`) makes the dep loop above
        // probe disk for `repl_r1.zpkg` — which never existed on disk (compiled to
        // bytes in memory) — and emit a spurious "cannot read dep zpkg meta" WARN.
        // Registering it here lets the dep loop short-circuit on `loaded_zpkgs`.
        // (fix-repl-inmemory-dep-warn)
        if let Some(pkg) = &artifact.package_name {
            self.mark_zpkg_loaded(format!("{pkg}.zpkg"));
        }

        let offset = self.main_pool_len + self.string_pool.len();
        self.string_pool.extend(artifact.module.string_pool.iter().cloned());

        for mut fn_ in artifact.module.functions {
            remap_const_str(&mut fn_, offset);
            let name = fn_.name.clone();
            if name.ends_with(".__static_init__") {
                static_inits.push(name.clone());
            }
            if self.function_table.contains_key(&name) {
                continue; // first-wins
            }
            self.function_table.insert(name, Arc::new(fn_));
        }
        artifact.module.type_registry_vec.clear();
        for (name, desc) in std::mem::take(&mut artifact.module.type_registry) {
            if self.type_registry.contains_key(&name) {
                continue;
            }
            self.type_registry.insert(name, desc);
        }

        // Eagerly load the test module's full declared-dependency closure
        // (functions + types + vtables). Lazy on-demand resolution suffices for
        // static calls, but virtual (instance-method) dispatch into a dep —
        // `set.Add(x)` on a `Std.Collections.SortedSet` — needs the dep's
        // TypeDesc *and* its method bodies present up front. Force-loading the
        // closure here matches the dedicated test-runner bootstrap's effect
        // (its candidates were resolved before any test ran).
        self.force_load_all_declared();

        // Inheritance fixup (subclasses whose base lives in an already-loaded module).
        let fixup_cap = self.type_registry.len() + 8;
        for round in 0.. {
            let n = crate::metadata::loader::try_fixup_inheritance(&mut self.type_registry);
            if n == 0 || round >= fixup_cap {
                break;
            }
        }

        Ok((entries, static_inits))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Rewrite all ConstStr `idx` values in a function's blocks by adding
/// `offset`, so the resulting indices point into the merged main+lazy pool.
fn remap_const_str(fn_: &mut Function, offset: usize) {
    for block in fn_.blocks.iter_mut() {
        for instr in block.instructions.iter_mut() {
            if let Instruction::ConstStr { idx, .. } = instr {
                *idx += offset as u32;
            }
        }
    }
}

/// Extract the namespace prefix from a fully-qualified function name.
/// E.g. "Std.IO.Console.WriteLine" → Some("Std.IO")
///      "Std.Assert.Equal"         → Some("Std")
///      "main"                     → None (no namespace)
pub(crate) fn namespace_prefix(func_name: &str) -> Option<String> {
    // A qualified function name has the form: <ns>.<Class>.<method>
    //                                         or <ns>.<func>
    // Strategy: strip the last two segments (Class.method), keep the rest.
    let dots: Vec<usize> = func_name.match_indices('.').map(|(i, _)| i).collect();
    if dots.len() < 2 {
        // "Class.method" — no explicit namespace. Use first segment as candidate.
        return dots.first().map(|&i| func_name[..i].to_string());
    }
    Some(func_name[..dots[dots.len() - 2]].to_string())
}

#[cfg(test)]
#[path = "lazy_loader_tests.rs"]
mod lazy_loader_tests;
