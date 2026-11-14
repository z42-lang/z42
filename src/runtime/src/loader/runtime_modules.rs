//! Package installation and explicit reload invalidate thread lookup caches.
use super::*;
impl RuntimeLoader<'_> {
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

    /// Look up a function by FQ name; triggers lazy load if needed.
    ///
    /// review.md D3 Phase 2 : emits `RuntimeEvent::ModuleLoaded`
    /// for every zpkg the resolver pulled in transitively, so observers
    /// see lazy-load activity (not just boot-time eager loads).
    /// Load a compiled artifact at `path` into the live registries (functions +
    /// types become callable / reflectable).
    /// Backs `Std.Runtime.ModuleLoader.Load` (`__load_module`).
    /// Errors if no lazy loader is installed (a bare VM with no dep resolution).
    pub fn load_module_into_vm(
        &self, path: &str,
    ) -> anyhow::Result<()> {
        // optimize-subclass-check: an explicit (re)load can redefine a type (REPL), which
        // could invalidate a cached subclass answer. Lazy-load ADDs (monotonic) are safe and
        // don't clear; only these explicit loads do.
        self.cache.subclass_memo.lock().clear();
        self.cache.isa_cache.clear();
        self.cache.type_lookup_cache.lock().clear();
        self.cache.fn_lookup_cache.lock().clear();
        crate::metadata::statics::StaticFields::new(&self.metadata.statics, &self.metadata.cctors).forget_owners();
        let mut state = self.state.lazy_loader.write();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadModule: no lazy loader installed (cannot register loaded module)")
        })?;
        let r = loader.load_module_from_path(path);
        drop(state);
        // Complete queued type-owner loading outside the loader lock. Execution
        // barriers remain responsible for managed package/type initialization.
        self.drain_pending_type_loads();
        r
    }

    /// In-memory sibling of [`load_module_into_vm`]: load a compiled artifact
    /// from raw bytes (packed zpkg / bare zbc) into the live registries. Backs
    /// the `__load_bytecode_in_memory` builtin used by `z42.scripting` (REPL) so
    /// freshly compiled bytecode is invocable with zero disk I/O. (add-z42-repl)
    /// Preserves existing static storage; new initialization facts are registered
    /// for execution barriers without rerunning or clearing earlier REPL rounds.
    pub fn load_module_bytes_into_vm(
        &self, raw: &[u8],
    ) -> anyhow::Result<()> {
        self.cache.subclass_memo.lock().clear();   // optimize-subclass-check: REPL redefinition safety
        self.cache.isa_cache.clear();
        self.cache.type_lookup_cache.lock().clear();
        self.cache.fn_lookup_cache.lock().clear();
        crate::metadata::statics::StaticFields::new(&self.metadata.statics, &self.metadata.cctors).forget_owners();
        let mut state = self.state.lazy_loader.write();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadBytecodeInMemory: no lazy loader installed (cannot register loaded module)")
        })?;
        let r = loader.load_module_from_bytes(raw);
        drop(state);
        // 同上（REPL 每轮的字节码加载路径）。
        self.drain_pending_type_loads();
        r
    }

}
