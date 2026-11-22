//! Package installation and explicit reload invalidate thread lookup caches.
use super::*;
use crate::loader::contexts::{AssemblyId, ContextId, ContextLoadError, ContextState};
impl RuntimeLoader<'_> {
    /// Seed the entry package before registering dependencies, so dependency
    /// registration can find entry definitions and their package ownership.
    pub(crate) fn install_startup_packages(
        &self,
        main_pool_len: usize,
        packages: crate::loader::StartupPackages,
    ) -> anyhow::Result<()> {
        self.install_lazy_loader_with_deps(
            packages.search_dirs,
            main_pool_len,
            packages.declared_candidates,
            packages.initially_loaded,
        );
        if let Some(entry) = self.metadata.module.as_deref() {
            let func_names: Vec<_> = entry.functions.iter().map(|f| f.name.clone()).collect();
            self.seed_lazy_loader_types(&entry.type_registry, &func_names);
        }
        for artifact in packages.artifacts {
            self.register_eager_artifact(artifact)?;
        }
        self.seed_lazy_loader_impls(&packages.impl_pairs);
        self.seed_lazy_loader_symbol_owners(&packages.entry_dependencies);
        Ok(())
    }

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
        let mut loader = LazyLoader::with_fusion_inputs(
            search_dirs, main_pool_len, declared, initially_loaded, self.state.fusion_inputs,
        );
        // fix-crosspkg-static-call-cctor：加载器在类型入表时登记 cctor，须持有同一份 registry。
        loader.set_cctor_registry(Arc::clone(&self.metadata.cctors));
        // 惰性包的函数登记在 VmCore 的 FuncTable 里（分配 FnId；它也是加载器的函数名表）。
        loader.set_func_table(Arc::clone(&self.metadata.funcs));
        // 惰性包的类型描述符发布到 VmCore 的 TypeTable（TypeId → 描述符）。
        loader.set_type_table(Arc::clone(&self.metadata.types));
        // 惰性包的字符串池追加到 VmCore 的 StrTable（ConstStr 操作数即其中的 id，永不复用）。
        loader.set_str_table(Arc::clone(&self.metadata.strings));
        // 惰性加载按需读包，走本 VM 的文件系统。
        loader.set_vfs(Arc::clone(&self.state.vfs));
        let mut state = self.state.lazy_loader.write();
        // 再次安装（只有测试会）= 新的名字空间：旧加载器登记的名字不再可见，与两张表合并前一致。
        // 旧槽位与 id 保留（id 永不复用），已缓存的 id 仍指向旧函数。
        self.metadata.funcs.reset_lazy_names();
        *state = Some(loader);
        drop(state);
        // cache-ctorless-objnew: a loader swap can make an absent ctor present.
        crate::metadata::dispatch::ic::note_fn_registration();
    }

    /// M1 no-eager-merge track: register an already-decoded dependency
    /// artifact through the lazy loader (the per-package machinery the lazy
    /// path uses for on-demand loads). Requires the loader to be installed.
    pub fn register_eager_artifact(&self, artifact: crate::loader::LoadedArtifact) -> anyhow::Result<()> {
        let mut state = self.state.lazy_loader.write();
        let loader = state
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no-eager-merge registration before the lazy loader is installed"))?;
        loader.register_decoded_artifact(artifact)
    }

    /// Clear the lazy loader (used in tests).
    pub fn uninstall_lazy_loader(&self) {
        let mut state = self.state.lazy_loader.write();
        self.metadata.funcs.reset_lazy_names();
        *state = None;
        drop(state);
        crate::metadata::dispatch::ic::note_fn_registration();
    }

    /// fix-cross-pkg-subclass-fields : seed the lazy loader's
    /// `type_registry` with TypeDescs from eagerly-loaded modules (e.g. the
    /// merged main module). Used by both the z42vm CLI and the in-process
    /// test runner immediately after `install_lazy_loader_with_deps` so the
    /// fixup pass can find eagerly-loaded base classes when lazy-loading a
    /// subclass.
    /// `func_names`：同包函数 FQN —— 包初始化失败后的归属判定要它（主包的自由函数既不在
    /// `types` 里也不是类型成员）。见 `LazyLoader::seed_types_for_lookup`。
    pub fn seed_lazy_loader_types(
        &self,
        types: &FxHashMap<String, Arc<TypeDesc>>,
        func_names: &[String],
    ) {
        let mut state = self.state.lazy_loader.write();
        if let Some(loader) = state.as_mut() {
            loader.seed_types_for_lookup(types, func_names);
        }
    }

    /// add-crosspkg-impl-reflection: seed `(target_fq, trait_fq)` impl pairs
    /// from an eagerly-loaded artifact (main zpkg / eagerly-loaded deps) into
    /// the lazy loader's impls registry. Companion of `seed_lazy_loader_types`.
    pub fn seed_lazy_loader_impls(&self, pairs: &[(String, String)]) {
        let mut state = self.state.lazy_loader.write();
        if let Some(loader) = state.as_mut() {
            loader.seed_impls(pairs);
        }
    }

    /// precise-pkg-refs: seed the loader's "which package defines this name" table from the
    /// eagerly merged artifacts' DEPS symbol lists. Companion of `seed_lazy_loader_impls`.
    pub fn seed_lazy_loader_symbol_owners(&self, deps: &[crate::zpkg::format::ZpkgDep]) {
        if deps.is_empty() { return; }
        let mut state = self.state.lazy_loader.write();
        if let Some(loader) = state.as_mut() {
            loader.note_symbol_owners(deps);
        }
    }

    /// precise-pkg-refs: the unique referenced full name for a simple type name, if any
    /// (reflection's dotless lookups — see `make_type_from_name`).
    pub fn referenced_full_name(&self, short: &str) -> Option<String> {
        self.state.lazy_loader.read().as_ref().and_then(|l| l.full_name_for_short(short))
    }

    /// Load into the context's private arena for reflection.
    pub(crate) fn load_context_artifact(
        &self,
        context: ContextId,
        path: &str,
    ) -> Result<AssemblyId, ContextLoadError> {
        if self.state.context_registry.lock().context_state(context) != Some(ContextState::Active) {
            return Err(ContextLoadError::Inactive);
        }
        let artifact = crate::loader::package::load_artifact_with_fusion(
            self.state.vfs.as_ref(),
            path,
            self.state.fusion_inputs,
        )
        .map_err(ContextLoadError::Artifact)?;
        let name = std::path::Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        Ok(self.state.context_registry.lock().load_into(context, name, artifact.module))
    }

    /// Load a compiled artifact at `path` into the live registries.
    /// Errors if the VM has no lazy loader.
    pub fn load_module_into_vm(&self, path: &str) -> anyhow::Result<()> {
        self.reload_module("LoadModule", |loader| loader.load_module_from_path(path))
    }

    /// Load packed zpkg or bare zbc bytes into the live registries for the REPL
    /// and other in-memory callers. Existing static values remain available.
    pub fn load_module_bytes_into_vm(&self, raw: &[u8]) -> anyhow::Result<()> {
        self.reload_module("LoadBytecodeInMemory", |loader| loader.load_module_from_bytes(raw))
    }

    fn reload_module(
        &self,
        operation: &str,
        load: impl FnOnce(&mut LazyLoader) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        // Explicit reloads can redefine types. Invalidate their cache entries
        // before attempting the load, including when the artifact is rejected.
        self.cache.subclass_memo.lock().clear();
        self.cache.isa_cache.clear();
        self.cache.type_lookup_cache.lock().clear();
        self.cache.fn_lookup_cache.lock().clear();
        crate::metadata::statics::StaticFields::new(&self.metadata.statics, &self.metadata.cctors).forget_owners();
        let result = {
            let mut state = self.state.lazy_loader.write();
            let loader = state.as_mut().ok_or_else(|| {
                anyhow::anyhow!("{operation}: no lazy loader installed (cannot register loaded module)")
            })?;
            load(loader)
        };
        // Pending type-owner loads can take the loader lock again.
        self.drain_pending_type_loads();
        result
    }
}
