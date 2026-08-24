use super::*;

impl VmContext {
    // ── Lazy loader (delegates to LazyLoader struct) ─────────────────────

    /// Install with no declared dependencies — for tests / single-file
    /// scripts without stdlib references.
    pub fn install_lazy_loader(&self, libs_dir: Option<PathBuf>, main_pool_len: usize) {
        self.install_lazy_loader_with_deps(
            libs_dir.into_iter().collect(), main_pool_len, Vec::new(), Vec::new());
    }

    /// Install with declared deps (see `LazyLoader::new` for parameter docs).
    /// `search_dirs` are consulted in order to resolve transitive dep zpkgs by
    /// file name (typically `[entry-zpkg dir, stdlib libs dir]`).
    pub fn install_lazy_loader_with_deps(
        &self,
        search_dirs: Vec<PathBuf>,
        main_pool_len: usize,
        declared: Vec<(String, ZpkgCandidate)>,
        initially_loaded: Vec<String>,
    ) {
        *self.core.lazy_loader.lock() = Some(LazyLoader::new(
            search_dirs,
            main_pool_len,
            declared,
            initially_loaded,
        ));
    }

    /// Clear the lazy loader (used in tests).
    pub fn uninstall_lazy_loader(&self) {
        *self.core.lazy_loader.lock() = None;
    }

    /// fix-cross-pkg-subclass-fields (2026-05-14): seed the lazy loader's
    /// `type_registry` with TypeDescs from eagerly-loaded modules (e.g. the
    /// merged main module). Used by both the z42vm CLI and the in-process
    /// test runner immediately after `install_lazy_loader_with_deps` so the
    /// fixup pass can find eagerly-loaded base classes when lazy-loading a
    /// subclass.
    pub fn seed_lazy_loader_types(&self, types: &FxHashMap<String, Arc<TypeDesc>>) {
        let mut state = self.core.lazy_loader.lock();
        if let Some(loader) = state.as_mut() {
            loader.seed_types_for_lookup(types);
        }
    }

    /// add-crosspkg-impl-reflection: seed `(target_fq, trait_fq)` impl pairs
    /// from an eagerly-loaded artifact (main zpkg / eagerly-loaded deps) into
    /// the lazy loader's impls registry. Companion of `seed_lazy_loader_types`.
    pub fn seed_lazy_loader_impls(&self, pairs: &[(String, String)]) {
        let mut state = self.core.lazy_loader.lock();
        if let Some(loader) = state.as_mut() {
            loader.seed_impls(pairs);
        }
    }

    /// Look up a function by FQ name; triggers lazy load if needed.
    ///
    /// review.md D3 Phase 2 (2026-05-27): emits `RuntimeEvent::ModuleLoaded`
    /// for every zpkg the resolver pulled in transitively, so observers
    /// see lazy-load activity (not just boot-time eager loads).
    /// Load a compiled artifact at `path` into the live registries (functions +
    /// types become callable / reflectable) and return its TIDX test entries.
    /// Backs `Std.Test.ModuleLoader.Load` for the reflective test runner.
    /// Errors if no lazy loader is installed (a bare VM with no dep resolution).
    pub fn load_module_into_vm(
        &self, path: &str,
    ) -> anyhow::Result<Vec<crate::metadata::test_index::LoadedTestEntry>> {
        let mut state = self.core.lazy_loader.lock();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadModule: no lazy loader installed (cannot register loaded module)")
        })?;
        loader.load_module_from_path(path)
    }

    /// In-memory sibling of [`load_module_into_vm`]: load a compiled artifact
    /// from raw bytes (packed zpkg / bare zbc) into the live registries. Backs
    /// the `__load_bytecode_in_memory` builtin used by `z42.scripting` (REPL) so
    /// freshly compiled bytecode is invocable with zero disk I/O. (add-z42-repl)
    /// Returns the freshly-loaded module's own `*.__static_init__` function names
    /// so the caller runs only this round's static init (REPL carry-forward — a full
    /// clear+rerun would wipe prior rounds' mutated static state).
    pub fn load_module_bytes_into_vm(
        &self, raw: &[u8],
    ) -> anyhow::Result<Vec<String>> {
        let mut state = self.core.lazy_loader.lock();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadBytecodeInMemory: no lazy loader installed (cannot register loaded module)")
        })?;
        loader.load_module_from_bytes(raw)
    }

    pub fn try_lookup_function(&self, func_name: &str) -> Option<Arc<Function>> {
        let (result, newly_loaded) = {
            let mut state = self.core.lazy_loader.lock();
            let loader = state.as_mut()?;
            // reduce-lazy-lookup-alloc: drain the loader's `newly_loaded` scratch
            // buffer around the resolve, instead of cloning + diffing the whole
            // `loaded_zpkgs` set on every call (profiled: that per-call clone was
            // the top interp alloc hotspot in cross-zpkg-heavy workloads such as
            // z42c self-compile). Common path (no new zpkg loaded) → empty buffer
            // → `mem::take` of a zero-cap Vec → zero allocation.
            loader.newly_loaded.clear();
            let result = loader.resolve_function(func_name);
            let newly = std::mem::take(&mut loader.newly_loaded);
            (result, newly)
        };
        for name in newly_loaded {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
        result
    }

    /// Look up a class TypeDesc by FQ name; triggers lazy load if needed.
    /// Same `ModuleLoaded` emit semantics as `try_lookup_function`.
    pub fn try_lookup_type(&self, class_name: &str) -> Option<Arc<TypeDesc>> {
        let (result, newly_loaded) = {
            let mut state = self.core.lazy_loader.lock();
            let loader = state.as_mut()?;
            // reduce-lazy-lookup-alloc: drain scratch buffer (see try_lookup_function).
            loader.newly_loaded.clear();
            let result = loader.resolve_type(class_name);
            let newly = std::mem::take(&mut loader.newly_loaded);
            (result, newly)
        };
        for name in newly_loaded {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
        result
    }

    /// Force-load every not-yet-loaded declared package (one-time eager load).
    /// Used by reflection's `make_type_from_name` dotless fallback when a class-like
    /// simple name (e.g. a constructed-generic field type_tag base `List`) can't be
    /// resolved from already-loaded types. See `LazyLoader::force_load_all`.
    pub fn force_load_all_packages(&self) {
        let newly = {
            let mut state = self.core.lazy_loader.lock();
            match state.as_mut() {
                Some(loader) => {
                    loader.newly_loaded.clear();
                    loader.force_load_all();
                    std::mem::take(&mut loader.newly_loaded)
                }
                None => Vec::new(),
            }
        };
        for name in newly {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
    }

    /// add-crosspkg-impl-reflection (unify P1-e): traits added to `target_fq`
    /// via cross-package `impl Trait for Type`, aggregated from every loaded
    /// package's IMPL section (returns owned Vec — the registry lives behind
    /// the lazy-loader lock). Empty when no loader / no impls.
    pub fn impl_traits_for(&self, target_fq: &str) -> Vec<String> {
        let state = self.core.lazy_loader.lock();
        match state.as_ref() {
            Some(loader) => loader.impl_traits_for(target_fq).to_vec(),
            None => Vec::new(),
        }
    }

    /// All currently-loaded type names — entry module's `type_registry` plus the
    /// lazy loader's already-loaded types. add-nested-types: `GetNestedTypes`
    /// scans these for `<outer>+<simple>` members. A type's nested types live in
    /// the same package as their declaring type, so resolving the declaring type
    /// (its handle is in hand) has already force-loaded the package — no need to
    /// force-load everything. Deduped across both sources.
    pub fn loaded_type_names(&self) -> Vec<String> {
        let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(m) = self.module() {
            for name in m.type_registry.keys() {
                set.insert(name.clone());
            }
        }
        {
            let state = self.core.lazy_loader.lock();
            if let Some(loader) = state.as_ref() {
                for name in loader.iter_type_names() {
                    set.insert(name.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    /// **unify-gc-heap PR-4**: lazily intern `module`'s string-pool literal `idx`
    /// into a GC string, cached per-context. Returns `None` if `idx` is out of the
    /// main pool (caller falls back to [`try_lookup_string`](Self::try_lookup_string)
    /// for the lazy-overflow pool).
    ///
    /// The pool cannot be materialized at module *load* time (no heap exists then),
    /// so the first reference to each literal allocates a GC string from the live
    /// heap and caches it here keyed by `(module ptr, idx)`; the cache is a GC root
    /// (scanned by the external root scanner), so the interned string survives
    /// collection while this context lives, and subsequent hits copy the 8-byte
    /// handle (no re-alloc — the same amortization the old pre-interned `Vec<Str>`
    /// gave, but heap-safe).
    #[inline]
    pub fn intern_const_str(&self, module: &crate::metadata::Module, idx: usize) -> Option<crate::metadata::vstr::Str> {
        let key = (module as *const crate::metadata::Module as usize, idx as u32);
        // Fast path: already interned in this context.
        if let Some(s) = self.interned_cache.lock().get(&key) {
            return Some(*s);
        }
        // Slow path: allocate from the live heap + cache. The heap is the ambient
        // heap for this frame; a fresh block is immediately reachable (returned into
        // a register) and, once cached, kept alive as a root.
        let raw = module.string_pool.get(idx)?;
        let s = self.heap().alloc_str(raw);
        self.interned_cache.lock().insert(key, s);
        Some(s)
    }

    /// Resolve an "overflow" ConstStr index past the main module's pool.
    /// Returns `Arc<str>` (review.md C3 Phase 1, 2026-06-03) so callers can
    /// wrap directly into `Value::Str` without a second allocation.
    pub fn try_lookup_string(&self, absolute_idx: usize) -> Option<crate::metadata::vstr::Str> {
        let state = self.core.lazy_loader.lock();
        let loader = state.as_ref()?;
        loader.try_lookup_string(absolute_idx)
    }

    /// All namespaces declared by lazy-loadable zpkgs (for static-init scan).
    pub fn declared_namespaces(&self) -> Vec<String> {
        let state = self.core.lazy_loader.lock();
        match state.as_ref() {
            Some(loader) => loader.declared_namespaces(),
            None         => Vec::new(),
        }
    }

    /// Force-load every declared zpkg, then return a sorted list of all
    /// `*.__static_init__` function names across loaded zpkgs.
    ///
    /// fix-multi-file-static-init (2026-05-15): the compiler now emits
    /// `<ns>.<source-stem>.__static_init__` (one per CU). A single
    /// per-namespace lookup can't find them all, so the runtime force-loads
    /// each declared zpkg and enumerates the loader's function table for
    /// the suffix. Sorted for determinism.
    pub fn collect_lazy_static_init_names(&self) -> Vec<String> {
        let mut state = self.core.lazy_loader.lock();
        let Some(loader) = state.as_mut() else { return Vec::new(); };
        loader.force_load_all_declared();
        let mut names: Vec<String> = loader.iter_function_names()
            .filter(|n| n.ends_with(".__static_init__"))
            .cloned()
            .collect();
        names.sort();
        names
    }
}
