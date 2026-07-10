/// Lazy dependency loader (zpkg-based, C# assembly model).
///
/// The VM eagerly loads only `z42.core` at startup. Other stdlib/third-party
/// zpkgs are loaded on demand when the interpreter encounters a `Call` or
/// `ObjNew` against an undefined function/type whose namespace matches a
/// declared-but-not-loaded zpkg.
///
/// ## Triggering (Decision 1: strategy C + fallback B)
///
///   1. Extract namespace prefix from `func_name` / `class_name`
///   2. Route to candidate zpkgs whose exported `namespaces` metadata
///      contains that prefix (precise routing, like C# CLR AssemblyRef →
///      TypeRef lookup)
///   3. Fallback: if strategy C matches nothing, iterate every declared-but-
///      -not-loaded zpkg until the target resolves or the set is exhausted
///   4. Transitive `ZpkgDep`s are unfolded into the declared set on load
///      (Decision 4 cycle-safe via pre-insert colouring)
///
/// Multiple zpkgs may legitimately declare the same namespace — the lookup
/// visits them one by one and `first-wins` on function/type name collisions
/// (Decision 6).
///
/// ## State ownership (consolidate-vm-state, 2026-04-28)
///
/// Previously `LazyLoader` lived in a `thread_local!` slot. Now an instance
/// is owned by `VmContext::lazy_loader`; all `try_lookup_*` /
/// `declared_namespaces` calls go through `VmContext` methods which delegate
/// here. `LazyLoader` itself remains usable directly by tests / advanced
/// embedders.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::bytecode::{Function, Instruction};
use super::loader::load_artifact;
use super::test_index::LoadedTestEntry;
use super::types::TypeDesc;
use super::zbc_reader::read_zpkg_meta;

// ── Public API ────────────────────────────────────────────────────────────────

/// A declared-but-not-yet-loaded zpkg. Records the information needed to
/// route a `Call` / `ObjNew` miss to the right zpkg for loading.
#[derive(Debug, Clone)]
pub struct ZpkgCandidate {
    /// Absolute path to the zpkg file.
    pub file_path: PathBuf,
    /// Namespaces exported by this zpkg (from its NSPC section).
    pub namespaces: Vec<String>,
}

impl ZpkgCandidate {
    /// Build a candidate by reading the zpkg metadata from disk.
    pub fn build(libs_dir: &Path, file_name: &str) -> Result<Self> {
        let file_path = libs_dir.join(file_name);
        let data = std::fs::read(&file_path)?;
        let meta = read_zpkg_meta(&data)?;
        Ok(Self {
            file_path,
            namespaces: meta.namespaces,
        })
    }

    /// Build a candidate by searching `dirs` in order for `file_name`, using
    /// the first directory that actually contains the file. Enables colocated
    /// dependency resolution (support-colocated-zpkg-deps, 2026-06-20): an
    /// apphost whose payload + its deps live together (e.g.
    /// `programs/z42c/z42c.driver.zpkg` next to `z42c.core.zpkg`) resolves
    /// those siblings even though they aren't in the stdlib `libs/` dir.
    pub fn build_in_dirs(dirs: &[PathBuf], file_name: &str) -> Result<Self> {
        for dir in dirs {
            let file_path = dir.join(file_name);
            if file_path.is_file() {
                return Self::build(dir, file_name);
            }
        }
        anyhow::bail!("zpkg `{file_name}` not found in any search dir ({} candidates)", dirs.len())
    }
}

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
    pub(crate) loaded_zpkgs:   HashSet<String>,
    /// zpkg file names that are declared as dependencies (direct or
    /// transitive) but have not yet been loaded. Lookup candidates.
    pub(crate) declared_zpkgs: HashMap<String, ZpkgCandidate>,

    /// Functions loaded from lazily-resolved zpkgs, indexed by FQ name.
    /// ConstStr indices have been remapped to absolute indices.
    function_table: HashMap<String, Arc<Function>>,
    /// Type descriptors from lazily-resolved zpkgs.
    type_registry:  HashMap<String, Arc<TypeDesc>>,
    /// add-crosspkg-impl-reflection (unify P1-e): `target_fq → [trait_fq]`
    /// aggregated from every loaded zpkg's IMPL section (`impl Trait for Type`).
    /// Appended (not first-wins): distinct packages may each impl different
    /// traits for the same target. Backs `Type.GetInterfaces()` seeing
    /// cross-package traits; only loaded packages contribute (an unloaded
    /// package's impl methods aren't callable either — consistent).
    impls:          HashMap<String, Vec<String>>,
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
    pub fn seed_types_for_lookup(&mut self, types: &HashMap<String, Arc<TypeDesc>>) {
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
        let loaded_zpkgs: HashSet<String> = initially_loaded.into_iter().collect();
        let declared_zpkgs: HashMap<String, ZpkgCandidate> = declared
            .into_iter()
            .filter(|(k, _)| !loaded_zpkgs.contains(k))
            .collect();
        Self {
            search_dirs,
            main_pool_len,
            string_pool:    Vec::new(),
            loaded_zpkgs,
            declared_zpkgs,
            function_table: HashMap::new(),
            type_registry:  HashMap::new(),
            impls:          HashMap::new(),
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
    /// underlying `String` is converted on each call; Phase 2 may add a
    /// parallel `interned_strings: Vec<Arc<str>>` to amortize this.
    pub fn try_lookup_string(&self, absolute_idx: usize) -> Option<std::sync::Arc<str>> {
        let rel = absolute_idx.checked_sub(self.main_pool_len)?;
        self.string_pool.get(rel).map(|s| std::sync::Arc::from(s.as_str()))
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
        self.declared_zpkgs
            .iter()
            .filter(|(file, cand)| {
                !self.loaded_zpkgs.contains(file.as_str())
                    && cand.namespaces.iter().any(|n| n == ns || n.starts_with(&ns_dot))
            })
            .map(|(file, _)| file.clone())
            .collect()
    }

    pub(crate) fn remaining_declared(&self) -> Vec<String> {
        self.declared_zpkgs
            .keys()
            .filter(|f| !self.loaded_zpkgs.contains(f.as_str()))
            .cloned()
            .collect()
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
        self.loaded_zpkgs.insert(file_name.to_string());

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
                    Err(e) => tracing::warn!(
                        "cannot read transitive dep zpkg meta `{}`: {e}", dep.file
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
        let mut artifact = load_artifact(path)?;
        let mod_key = format!("__loaded_path__{}", artifact.module.name);

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
            // `dependencies` list). Resolve each `using` namespace to the zpkg(s)
            // declaring it, across all search dirs.
            for ns in &artifact.import_namespaces {
                let zpkg_paths = match super::loader::resolve_namespace(ns, &[], &dirs) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                for zpkg_path in zpkg_paths {
                    let Some(file) = zpkg_path
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
                    if let Ok(cand) = ZpkgCandidate::build_in_dirs(&dirs, &file) {
                        self.declared_zpkgs.insert(file, cand);
                    }
                }
            }
        }

        // Idempotent: a re-load of the same module just returns its entries.
        if self.loaded_zpkgs.contains(&mod_key) {
            return Ok(entries);
        }
        self.loaded_zpkgs.insert(mod_key);

        let offset = self.main_pool_len + self.string_pool.len();
        self.string_pool.extend(artifact.module.string_pool.iter().cloned());

        for mut fn_ in artifact.module.functions {
            remap_const_str(&mut fn_, offset);
            let name = fn_.name.clone();
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

        Ok(entries)
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
