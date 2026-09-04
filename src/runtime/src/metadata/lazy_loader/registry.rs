//! 注册与加载：把一个 zpkg / 模块的函数、类型、字符串池、impl 对并入 loader 的注册表，
//! 并做跨包继承 fixup（含 `try_fixup_inheritance` 的不动点循环）。解析路径在 `super::resolve`。

use super::*;

impl LazyLoader {
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
            // defer-class-initialization: 入队，锁外由 VmContext 排空执行。
            if name.ends_with(".__static_init__") {
                self.pending_static_inits.push(name.clone());
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
        let (entries, static_inits) = self.register_loaded_artifact(artifact)?;
        // defer-class-initialization: 本路径（宿主 / z42b 显式按路径加载模块）把收集到的
        // `__static_init__` 名字**丢弃**了——变更前无所谓，因为随后的 `init_static_fields`
        // 会 force-load 全部包再逐个枚举执行；现在没有那一步，必须入队交给
        // `VmContext::run_pending_static_inits` 执行。
        // （`load_module_from_bytes` 不在此列：它把名字返回给 REPL，由调用方按轮次自己跑。）
        self.pending_static_inits.extend(static_inits);
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
    pub(super) fn register_loaded_artifact(
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
