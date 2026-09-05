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
        *self.core.lazy_loader.write() = Some(LazyLoader::new(
            search_dirs,
            main_pool_len,
            declared,
            initially_loaded,
        ));
        // cache-ctorless-objnew: a loader swap can make an absent ctor present.
        crate::metadata::resolver::note_fn_registration();
    }

    /// Clear the lazy loader (used in tests).
    pub fn uninstall_lazy_loader(&self) {
        *self.core.lazy_loader.write() = None;
        crate::metadata::resolver::note_fn_registration();
    }

    /// cache-ctorless-objnew: the current function-registration count. An `ObjNew`
    /// site that proved "this class has no constructor" at value `n` may reuse that
    /// answer while this still reads `n` — `function_table` only ever grows, and a
    /// ctor can only appear by being inserted there.
    #[inline]
    pub fn fn_registration_mark(&self) -> usize {
        crate::metadata::resolver::fn_registration_mark()
    }

    /// fix-cross-pkg-subclass-fields (2026-05-14): seed the lazy loader's
    /// `type_registry` with TypeDescs from eagerly-loaded modules (e.g. the
    /// merged main module). Used by both the z42vm CLI and the in-process
    /// test runner immediately after `install_lazy_loader_with_deps` so the
    /// fixup pass can find eagerly-loaded base classes when lazy-loading a
    /// subclass.
    pub fn seed_lazy_loader_types(&self, types: &FxHashMap<String, Arc<TypeDesc>>) {
        let mut state = self.core.lazy_loader.write();
        if let Some(loader) = state.as_mut() {
            loader.seed_types_for_lookup(types);
        }
    }

    /// add-crosspkg-impl-reflection: seed `(target_fq, trait_fq)` impl pairs
    /// from an eagerly-loaded artifact (main zpkg / eagerly-loaded deps) into
    /// the lazy loader's impls registry. Companion of `seed_lazy_loader_types`.
    pub fn seed_lazy_loader_impls(&self, pairs: &[(String, String)]) {
        let mut state = self.core.lazy_loader.write();
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
        // optimize-subclass-check: an explicit (re)load can redefine a type (REPL), which
        // could invalidate a cached subclass answer. Lazy-load ADDs (monotonic) are safe and
        // don't clear; only these explicit loads do.
        self.subclass_memo.lock().clear();
        self.isa_cache.clear();
        let mut state = self.core.lazy_loader.write();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadModule: no lazy loader installed (cannot register loaded module)")
        })?;
        let r = loader.load_module_from_path(path);
        drop(state);
        // defer-class-initialization: 显式加载路径（z42b / 宿主 LoadModule）内部会
        // `force_load_all_declared()` 拉进依赖闭包，那些包的 `__static_init__` 同样入队，
        // 必须在这里排空——否则「类已注册但初始化器没跑」，静态字段永远读到 null
        // （实测：z42b 的 `DepScanCache._count` 是 Null，崩在 DepScan.ScanDirs）。
        self.run_pending_static_inits();
        r
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
        self.subclass_memo.lock().clear();   // optimize-subclass-check: REPL redefinition safety
        self.isa_cache.clear();
        let mut state = self.core.lazy_loader.write();
        let loader = state.as_mut().ok_or_else(|| {
            anyhow::anyhow!("LoadBytecodeInMemory: no lazy loader installed (cannot register loaded module)")
        })?;
        let r = loader.load_module_from_bytes(raw);
        drop(state);
        // 同上（REPL 每轮的字节码加载路径）。
        self.run_pending_static_inits();
        r
    }

    pub fn try_lookup_function(&self, func_name: &str) -> Option<Arc<Function>> {
        // fix-lazy-lookup-contention：稳态命中走读锁 —— 没有加载任何 zpkg，所以
        // `newly_loaded` 恒空；静态初始化的排空判定仍照旧（`loader_quiet` 同样在锁内读）。
        let fast = {
            let state = self.core.lazy_loader.read();
            match state.as_ref() {
                Some(loader) => loader
                    .probe_function(func_name)
                    .map(|f| (f, loader.pending_static_inits.is_empty())),
                None => return None,
            }
        };
        if let Some((f, loader_quiet)) = fast {
            if !self.static_init_drain_is_noop(&[], loader_quiet) {
                self.run_pending_static_inits();
            }
            return Some(f);
        }
        let (result, newly_loaded, loader_quiet) = {
            let mut state = self.core.lazy_loader.write();
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
            let quiet = loader.pending_static_inits.is_empty();
            (result, newly, quiet)
        };
        if self.static_init_drain_is_noop(&newly_loaded, loader_quiet) {
            return result;
        }
        // defer-class-initialization (T1): 锁已释放，跑刚拉进来的包的初始化器。
        self.run_pending_static_inits();
        for name in newly_loaded {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
        result
    }

    /// Look up a class TypeDesc by FQ name; triggers lazy load if needed.
    /// Same `ModuleLoaded` emit semantics as `try_lookup_function`.
    pub fn try_lookup_type(&self, class_name: &str) -> Option<Arc<TypeDesc>> {
        // fix-lazy-lookup-contention：同 try_lookup_function —— registry 命中且基类链完整
        // 时是纯读，走读锁。
        let fast = {
            let state = self.core.lazy_loader.read();
            match state.as_ref() {
                Some(loader) => loader
                    .probe_type(class_name)
                    .map(|td| (td, loader.pending_static_inits.is_empty())),
                None => return None,
            }
        };
        if let Some((td, loader_quiet)) = fast {
            if !self.static_init_drain_is_noop(&[], loader_quiet) {
                self.run_pending_static_inits();
            }
            return Some(td);
        }
        let (result, newly_loaded, loader_quiet) = {
            let mut state = self.core.lazy_loader.write();
            let loader = state.as_mut()?;
            // reduce-lazy-lookup-alloc: drain scratch buffer (see try_lookup_function).
            loader.newly_loaded.clear();
            let result = loader.resolve_type(class_name);
            let newly = std::mem::take(&mut loader.newly_loaded);
            let quiet = loader.pending_static_inits.is_empty();
            (result, newly, quiet)
        };
        if self.static_init_drain_is_noop(&newly_loaded, loader_quiet) {
            return result;
        }
        // defer-class-initialization (T2): 同上。
        self.run_pending_static_inits();
        for name in newly_loaded {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
        result
    }

    /// cache-failed-name-resolution: `true` when `run_pending_static_inits` provably
    /// has nothing to do, so a lookup can skip it and its two extra mutex round-trips
    /// plus the `static_init_state` scan. Not a semantic change: every `false` falls
    /// through to the real drain, and the barrier `static_init_concurrent` depends on
    /// is preserved because "another thread is inside an initializer" is exactly
    /// `running_static_inits != 0`.
    ///
    /// `newly_loaded` / `loader_quiet` come out of the loader lock the caller already
    /// held; the other two reads are plain atomics.
    #[inline]
    fn static_init_drain_is_noop(&self, newly_loaded: &[String], loader_quiet: bool) -> bool {
        use std::sync::atomic::Ordering;
        newly_loaded.is_empty()
            && loader_quiet
            && self.core.pending_type_init_count.load(Ordering::Relaxed) == 0
            && self.core.running_static_inits.load(Ordering::Acquire) == 0
            && self.core.init_batch_inflight.load(Ordering::Acquire) == 0
    }

    /// defer-class-initialization: 排空「待初始化类」与「待跑 `__static_init__`」两个队列。
    ///
    /// **必须在 loader 锁释放后调用**——初始化器自身会再进 `try_lookup_*` 抢同一把锁。
    /// 排空是循环的：一个初始化器可能拉进新的包，新的包又带来新的初始化器。
    ///
    /// 重入（初始化器内部再次触发查找）由线程本地 `DRAINING` 标志挡掉：嵌套调用直接返回，
    /// 由最外层的循环继续消费新入队的项。
    pub fn run_pending_static_inits(&self) {
        thread_local! {
            static DRAINING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) { DRAINING.with(|d| d.set(false)); }
        }
        if DRAINING.with(|d| d.replace(true)) {
            return; // 已在排空中（嵌套触发）——外层循环会处理新入队项
        }
        let _guard = Guard;

        loop {
            // 取队列**之前**先宣告在飞：从这一刻起到本批处理完，别的线程都不会把
            // 「三个队列/计数都是零」误读成「初始化已静止」（见 `init_batch_inflight`）。
            self.core.init_batch_inflight.fetch_add(1, std::sync::atomic::Ordering::Release);
            // ① 待初始化的所属类（静态字段引用触发点 T3）：解析类型会触发所属包加载，
            //    进而把该包的 `__static_init__` 压进 pending_static_inits。
            let types: Vec<String> = {
                let mut q = self.core.pending_type_inits.lock();
                self.core.pending_type_init_count
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                std::mem::take(&mut *q)
            };
            for class_fq in &types {
                let _ = self.try_lookup_type(class_fq);
            }

            // ② 待跑的初始化器。**取出与认领必须在同一把锁里完成** —— 只 take 不认领的话，
            //    从队列清空到 `run_one_static_init` 写 `Running` 之间有一个窗口，别的线程会看到
            //    「队列空 + 无 Running + 计数 0」而误判「初始化已静止」，接着读到未赋值的静态
            //    字段。取走即写 `Claimed(me)`（见 `InitState::Claimed`）。
            let names: Vec<String> = {
                let mut state = self.core.lazy_loader.write();
                match state.as_mut() {
                    Some(loader) => {
                        let taken = std::mem::take(&mut loader.pending_static_inits);
                        for n in &taken {
                            if !loader.static_init_state.contains_key(n) {
                                loader.static_init_state.insert(
                                    n.clone(),
                                    crate::metadata::lazy_loader::InitState::Claimed(
                                        std::thread::current().id(),
                                    ),
                                );
                                // 与 `Claimed` 同锁自增：计数从此刻起就非零，窗口关闭。
                                // 对应的自减在 `Done` 落定处（Claimed → Running 只是换状态，
                                // 不动计数）。
                                self.core.running_static_inits
                                    .fetch_add(1, std::sync::atomic::Ordering::Release);
                            }
                        }
                        taken
                    }
                    None => Vec::new(),
                }
            };
            if names.is_empty() && types.is_empty() {
                // 本批没活 —— 先撤销自己的在飞声明，再去等别人静止（否则会等自己）。
                self.core.init_batch_inflight.fetch_sub(1, std::sync::atomic::Ordering::Release);
                self.await_init_quiescence();
                return;
            }
            for name in names {
                self.run_one_static_init(&name);
            }
            self.core.init_batch_inflight.fetch_sub(1, std::sync::atomic::Ordering::Release);
        }
    }

    /// 等到**没有其它线程**正在跑 `__static_init__` 才返回。
    ///
    /// 光靠「队列里有没有我的活」不够：两个线程同时进 `resolve_function_tokens` 时，
    /// 先到的把待跑队列 `mem::take` 走，后到的看到空队列就直接返回、接着去读那个
    /// **还在初始化中**的类的静态字段 → 读到 Null（cross-zpkg golden
    /// `static_init_concurrent` 稳定复现，interp / JIT 两种模式都会）。
    /// 因此排空的收尾必须是「初始化静止」而不是「我的队列空了」。
    ///
    /// 不会互相死等：本方法只等**别人**的 `Running`，而调用它时自己名下的初始化
    /// 都已跑完（`run_one_static_init` 返回即 `Done`），所以不存在 A 等 B、B 等 A 的环。
    fn await_init_quiescence(&self) {
        let me = std::thread::current().id();
        // 上限兜底：初始化器都是纯表构造（实测 31 个合计 78 µs），正常等待是微秒级。
        // 真等到这个上限说明持有线程已经死了，继续空转没有意义——放行并留下告警。
        for spin in 0..2_000_000u64 {
            // Lock-free pre-check: no `Running` entry anywhere → nothing to wait for.
            if self.core.running_static_inits.load(std::sync::atomic::Ordering::Acquire) == 0
                && self.core.init_batch_inflight.load(std::sync::atomic::Ordering::Acquire) == 0
            {
                return;
            }
            if self.core.init_batch_inflight.load(std::sync::atomic::Ordering::Acquire) != 0 {
                std::thread::yield_now();
                continue;   // 有线程正在处理一批 —— 它可能还会产出新的初始化器
            }
            let busy = {
                let state = self.core.lazy_loader.write();
                match state.as_ref() {
                    Some(loader) => loader.static_init_state.values().any(|st| {
                        // `Claimed` 与 `Running` 一样算「他线程手上还有活」。
                        matches!(
                            st,
                            crate::metadata::lazy_loader::InitState::Running(t)
                                | crate::metadata::lazy_loader::InitState::Claimed(t)
                            if *t != me
                        )
                    }),
                    None => false,
                }
            };
            if !busy { return; }
            std::thread::yield_now();
            if spin == 1_999_999 {
                tracing::warn!("static-init quiescence wait timed out; proceeding");
            }
        }
    }

    /// 执行单个 `__static_init__`，按 `InitState` 保证「每个最多跑一次」。
    ///
    /// - 同线程已在跑（循环初始化器）→ 直接返回，允许观察部分初始化状态（CLR 语义）。
    /// - 他线程正在跑 → 自旋让出，等到 `Done` 再返回，保证读到完整初始化的静态字段。
    fn run_one_static_init(&self, name: &str) {
        use crate::metadata::lazy_loader::InitState;
        let me = std::thread::current().id();
        loop {
            let claimed = {
                let mut state = self.core.lazy_loader.write();
                let Some(loader) = state.as_mut() else { return };
                match loader.static_init_state.get(name) {
                    Some(InitState::Done) => return,
                    Some(InitState::Running(tid)) if *tid == me => return, // 重入
                    // 本线程在 ② 里取走时已认领 —— 换成 Running 开跑（计数早已算上，不再自增）。
                    Some(InitState::Claimed(tid)) if *tid == me => {
                        loader.static_init_state.insert(name.to_string(), InitState::Running(me));
                        true
                    }
                    Some(InitState::Running(_)) | Some(InitState::Claimed(_)) => false, // 他线程持有 → 等
                    None => {
                        loader.static_init_state.insert(name.to_string(), InitState::Running(me));
                        // Bumped under the loader lock, alongside the `Running` entry
                        // it mirrors, so `== 0` ⇒ no `Running` entry exists.
                        self.core.running_static_inits
                            .fetch_add(1, std::sync::atomic::Ordering::Release);
                        true
                    }
                }
            };
            if claimed { break; }
            std::thread::yield_now();
        }

        tracing::debug!("running lazy static init `{name}`");
        let outcome = match self.module() {
            Some(module) => {
                let f = self.try_lookup_function(name);
                match f {
                    Some(f) => match crate::interp::exec_function(self, module, f.as_ref(), &[]) {
                        Ok(crate::interp::ExecOutcome::Returned(_)) => None,
                        Ok(crate::interp::ExecOutcome::Thrown(v)) => Some(format!(
                            "uncaught exception in static init `{name}`: {}",
                            crate::interp::value_to_str(&v)
                        )),
                        Err(e) => Some(format!("static init `{name}` failed: {e:#}")),
                    },
                    None => Some(format!("static init `{name}` disappeared from the loader")),
                }
            }
            None => Some(format!("static init `{name}`: no module installed")),
        };

        {
            let mut state = self.core.lazy_loader.write();
            if let Some(loader) = state.as_mut() {
                loader.static_init_state.insert(name.to_string(), InitState::Done);
            }
            self.core.running_static_inits
                .fetch_sub(1, std::sync::atomic::Ordering::Release);
        }
        if let Some(msg) = outcome {
            tracing::error!("{msg}");
            let mut slot = self.core.static_init_error.lock();
            if slot.is_none() { *slot = Some(msg); }
        }
    }

    /// defer-class-initialization: 该类是否已在 registry 中（即所属包已加载 + 初始化过）。
    /// 只读，不触发任何加载——供 T3 入队前的快速过滤。
    pub fn has_loaded_type(&self, class_fq: &str) -> bool {
        let state = self.core.lazy_loader.write();
        match state.as_ref() {
            Some(loader) => loader.has_type(class_fq),
            None => false,
        }
    }

    /// defer-class-initialization: 入队一个「静态字段所属类」，等待 `run_pending_static_inits`
    /// 触发其所属包的加载 + 初始化。由 `metadata::resolver` 在解析静态字段名时调用。
    pub fn enqueue_type_init(&self, class_fq: &str) {
        let mut q = self.core.pending_type_inits.lock();
        if !q.iter().any(|c| c == class_fq) {
            q.push(class_fq.to_string());
            self.core.pending_type_init_count
                .store(q.len(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// defer-class-initialization: 取走首个懒初始化失败（若有）。`Vm::run` 在入口返回后检查。
    pub fn take_static_init_error(&self) -> Option<String> {
        self.core.static_init_error.lock().take()
    }

    /// Force-load every not-yet-loaded declared package (one-time eager load).
    /// Used by reflection's `make_type_from_name` dotless fallback when a class-like
    /// simple name (e.g. a constructed-generic field type_tag base `List`) can't be
    /// resolved from already-loaded types. See `LazyLoader::force_load_all`.
    pub fn force_load_all_packages(&self) {
        let newly = {
            let mut state = self.core.lazy_loader.write();
            match state.as_mut() {
                Some(loader) => {
                    loader.newly_loaded.clear();
                    loader.force_load_all();
                    std::mem::take(&mut loader.newly_loaded)
                }
                None => Vec::new(),
            }
        };
        // defer-class-initialization: 反射的 dotless 兜底会一次性拉进所有包，
        // 它们的 `__static_init__` 同样入队，必须排空（所有会触发加载的入口都要排空，
        // 否则「类已注册但初始化器没跑」的窗口会重新出现）。
        self.run_pending_static_inits();
        for name in newly {
            self.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size: None });
        }
    }

    /// add-crosspkg-impl-reflection (unify P1-e): traits added to `target_fq`
    /// via cross-package `impl Trait for Type`, aggregated from every loaded
    /// package's IMPL section (returns owned Vec — the registry lives behind
    /// the lazy-loader lock). Empty when no loader / no impls.
    pub fn impl_traits_for(&self, target_fq: &str) -> Vec<String> {
        let state = self.core.lazy_loader.write();
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
            let state = self.core.lazy_loader.write();
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
        // fix-lazy-lookup-contention：`LazyLoader::try_lookup_string` 只读 string_pool，
        // 不碰任何可变状态 —— 读锁即可，多线程可并发。
        let state = self.core.lazy_loader.read();
        let loader = state.as_ref()?;
        loader.try_lookup_string(absolute_idx)
    }

    /// All namespaces declared by lazy-loadable zpkgs (for static-init scan).
    pub fn declared_namespaces(&self) -> Vec<String> {
        let state = self.core.lazy_loader.write();
        match state.as_ref() {
            Some(loader) => loader.declared_namespaces(),
            None         => Vec::new(),
        }
    }

}
