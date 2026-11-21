//! Shared artifact preparation for CLI and embedded hosts.
use std::{collections::HashSet, path::{Path, PathBuf}, sync::Arc};
use anyhow::{bail, Context, Result};
use crate::{zpkg::ExecMode, pal::fs::Vfs, loader::{LoadedArtifact, build_declared_candidates, version_mismatch_hint}};
use super::program::{LoadOptions, LoadedProgram};

fn vfs_exists(fs: &dyn Vfs, p: &Path) -> bool { p.to_str().is_some_and(|s| fs.exists(s)) }
/// Ordered prelude candidates — `<dir>/z42.core.zpkg` for every search dir that
/// actually has one, in `search_dirs` order (entry zpkg's own directory first,
/// then `libs/`).
///
/// The prelude used to be the ONE dependency that ignored that order and looked
/// only at `libs/`. See `run`'s 5.1b for what that cost.
pub(crate) fn prelude_candidates(fs: &dyn Vfs, search_dirs: &[PathBuf]) -> Vec<PathBuf> {
    search_dirs
        .iter()
        .map(|d| d.join("z42.core.zpkg"))
        .filter(|p| vfs_exists(fs, p))
        .collect()
}


fn entry_zpkg_idempotency_keys(file: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(name) = std::path::Path::new(file).file_name().and_then(|n| n.to_str()) {
        keys.push(name.to_string());
        if let Some(stem) = name.strip_suffix(".zpkg") {
            keys.push(format!("__loaded_path__{stem}"));
        }
    }
    keys
}

pub(super) fn prepare(user_artifact: LoadedArtifact, source_path: Option<&str>, options: &LoadOptions, components: &crate::vm::assembly::ComponentInputs, image_cache: Option<&crate::loader::ImageCache>) -> Result<LoadedProgram> {
    prepare_with_eager_mode(user_artifact, source_path, options, components, image_cache, EagerMerge::Default)
}

pub(super) fn prepare_with_eager_mode(user_artifact: LoadedArtifact, source_path: Option<&str>, options: &LoadOptions, components: &crate::vm::assembly::ComponentInputs, image_cache: Option<&crate::loader::ImageCache>, eager_mode: EagerMerge) -> Result<LoadedProgram> {
    let vfs = Arc::clone(&options.vfs);
    let fs = &*vfs;
    let search_dirs = options.search_dirs.clone();
    let file = source_path.unwrap_or("<memory>");
    // Deps keep their whole artifact (M1 no-eager-merge registers them
    // per-package after construction); the merged path maps them to modules.
    let mut artifacts: Vec<crate::loader::LoadedArtifact> = Vec::new();
    // Track canonical paths of loaded artifact files to prevent duplicate loading.
    let mut loaded_paths: HashSet<PathBuf> = HashSet::new();
    // Track zpkg file names loaded eagerly at startup (initially_loaded input
    // for the lazy loader — these are excluded from on-demand candidate set).
    // Seeded with the entry zpkg's two idempotency keys (see `entry_zpkg_idempotency_keys`).
    let mut initially_loaded_zpkgs = source_path.map(entry_zpkg_idempotency_keys).unwrap_or_default();
    // add-runtime-observer: collect (name, byte_size) for every module loaded
    // during boot, replay-emitted as `ModuleLoaded` events after the registry exists.
    let mut loaded_for_replay: Vec<(String, Option<u64>)> = Vec::new();
    // add-crosspkg-impl-reflection: collect (target_fq, trait_fq) impl pairs
    // from every eagerly-loaded artifact; seeded into the lazy loader below.
    let mut eager_impl_pairs: Vec<(String, String)> = Vec::new();

    let mut resolved_names = HashSet::new();
    if let Some(resolver) = &options.resolver {
        let mut queue = std::collections::VecDeque::from(["z42.core".to_owned()]);
        queue.extend(user_artifact.import_namespaces.iter().cloned());
        queue.extend(user_artifact.dependencies.iter().flat_map(|d| d.namespaces.iter().cloned()));
        let mut visited = HashSet::new();
        while let Some(namespace) = queue.pop_front() {
            if !visited.insert(namespace.clone()) { continue; }
            let Some(bytes) = resolver.resolve(&namespace) else { continue; };
            let artifact = load_bytes_cached(&bytes, image_cache)
                .with_context(|| format!("zpkg_resolver returned malformed bytes for namespace `{namespace}`"))?;
            initially_loaded_zpkgs.push(format!("{namespace}.zpkg"));
            if !resolved_names.insert(blake3::hash(&bytes)) { continue; }
            queue.extend(artifact.import_namespaces.iter().cloned());
            queue.extend(artifact.dependencies.iter().flat_map(|d| d.namespaces.iter().cloned()));
            loaded_for_replay.push((namespace, Some(bytes.len() as u64)));
            eager_impl_pairs.extend(artifact.impl_pairs.iter().cloned());
            artifacts.push(artifact);
        }
    }
    // 5.1b — unconditionally try to load z42.core.zpkg if present.
    //
    // fix-prelude-entry-dir-resolution: walk `search_dirs` (entry dir first, then
    // `libs/`), not `libs_dir` alone. The prelude used to be the ONE package that
    // ignored the colocated-deps order every other dependency follows — and it
    // `bail!`ed on the first version mismatch, so a `libs/` holding a
    // wrong-generation z42.core killed the process even when the entry zpkg's own
    // directory carried a perfectly readable one.
    //
    // That is exactly the two-gen bootstrap's gen2 step: old VM + entry dir of
    // matching-generation stdlib + `Z42_LIBS` pointing at the freshly written
    // NEW-generation flat view. `resolve_libs_dir` puts `$Z42_LIBS` first, so this
    // block opened the new z42.core and aborted — blocking every zbc/zpkg format
    // bump, however small. Trying candidates in order fixes it without weakening
    // the diagnostic: the hint still fires, but only when NO candidate loads.
    {
        let mut prelude_err: Option<String> = None;
        let mut seen_any = false;
        for core_path in prelude_candidates(fs, &search_dirs) {
            if initially_loaded_zpkgs.iter().any(|n| n == "z42.core.zpkg") { break; }
            seen_any = true;
            let core_canonical = core_path.canonicalize().unwrap_or(core_path.clone());
            let core_str = core_path.to_string_lossy().into_owned();
            match load_path_cached(fs, &core_str, image_cache) {
                Ok(a) => {
                    tracing::debug!("loaded stdlib z42.core from {core_str}");
                    let byte_size = std::fs::metadata(&core_path).ok().map(|m| m.len());
                    loaded_for_replay.push(("z42.core.zpkg".to_string(), byte_size));
                    eager_impl_pairs.extend(a.impl_pairs.iter().cloned());
                    artifacts.insert(0, a);
                    loaded_paths.insert(core_canonical);
                    initially_loaded_zpkgs.push("z42.core.zpkg".to_string());
                    prelude_err = None;
                    break;
                }
                Err(e) => {
                    // Keep the FIRST failure's message: it names the candidate the
                    // caller most likely meant. Only surfaced if nothing loads.
                    match version_mismatch_hint(&core_str, &e) {
                        Some(hint) => { if prelude_err.is_none() { prelude_err = Some(hint); } }
                        None => tracing::warn!("failed to load z42.core from {core_str}: {e:#}"),
                    }
                }
            }
        }
        // Unrecoverable: z42.core is the prelude — nothing runs without it. Only
        // reached when every candidate failed with a version mismatch.
        if let Some(hint) = prelude_err { bail!("{hint}"); }
        if !seen_any {
            tracing::debug!("z42.core.zpkg not found in any of {:?}", search_dirs);
        }
    }

    if let Some(path) = source_path { register_entry_as_loaded(path, &mut loaded_paths, &mut loaded_for_replay); }

    // 5.1d — dependency loading strategy: Interp / JIT → pure lazy (declared
    // candidates drive on-demand load); AOT → eager transitive BFS (whole
    // program compiled ahead of time, every callee merged up front).
    let is_eager = options.eager_dependencies || matches!(options.mode, ExecMode::Aot);
    if is_eager && !search_dirs.is_empty() {
        use std::collections::VecDeque;
        let libs_paths = search_dirs.clone();
        let mut file_queue: VecDeque<String> =
            user_artifact.dependencies.iter().map(|d| d.file.clone()).collect();
        let mut ns_queue: VecDeque<String> =
            user_artifact.import_namespaces.iter().cloned().collect();
        loop {
            while let Some(ns) = ns_queue.pop_front() {
                if visited_resolver_namespace(options, &initially_loaded_zpkgs, &ns) { continue; }
                let Ok(zpkg_paths) = crate::loader::resolve_namespace(fs, &ns, &libs_paths)
                else { continue };
                for zpkg_path in zpkg_paths {
                    if let Some(name) = zpkg_path.file_name().and_then(|n| n.to_str()) {
                        file_queue.push_back(name.to_string());
                    }
                }
            }
            let Some(dep_file) = file_queue.pop_front() else { break };
            let Some(dep_path) = search_dirs.iter()
                .map(|d| d.join(&dep_file))
                .find(|p| vfs_exists(fs, p))
            else { continue };
            let canonical = dep_path.canonicalize().unwrap_or_else(|_| dep_path.clone());
            if initially_loaded_zpkgs.iter().any(|n| n == &dep_file) || !loaded_paths.insert(canonical) { continue; }  // already merged
            let dep_str = dep_path.to_string_lossy().into_owned();
            let artifact = load_path_cached(fs, &dep_str, image_cache)
                .with_context(|| format!("loading dependency `{dep_str}`"))?;
            for dep in &artifact.dependencies { file_queue.push_back(dep.file.clone()); }
            for ns in &artifact.import_namespaces { ns_queue.push_back(ns.clone()); }
            eager_impl_pairs.extend(artifact.impl_pairs.iter().cloned());
            artifacts.push(artifact);
            initially_loaded_zpkgs.push(dep_file.clone());
        }
    }

    // precise-pkg-refs: the user artifact's DEPS symbol lists seed the loader's routing table.
    let eager_deps = user_artifact.dependencies.clone();
    // Build declared-but-not-loaded zpkg candidate set for the lazy loader,
    // BEFORE moving `user_artifact.module` into `modules` (partial-move).
    let declared_candidates = build_declared_candidates(
        fs,
        &user_artifact,
        &search_dirs,
        &initially_loaded_zpkgs,
    );

    // 5.1e — push user module last, then merge everything. Preserve the user
    // module's name so entry-point lookup resolves correctly.
    let entry_hint = user_artifact.entry_hint.clone();
    let user_module_name = user_artifact.module.name.clone();
    eager_impl_pairs.extend(user_artifact.impl_pairs.iter().cloned());

    // M1 dual track (merge-retirement design §4): the default path keeps the
    // flat merge byte for byte; the no-eager-merge path carries the entry
    // module alone and hands the decoded dep artifacts to the assembly, which
    // registers them per-package through the lazy loader's machinery.
    let no_eager_merge = match eager_mode {
        // M4 dry run (merge-retirement design §4): per-package registration is
        // now the DEFAULT. The escape hatch is Z42_EAGER_MERGE=1 (restores the
        // merged track) — the same transitional shape the old switch had,
        // inverted. Full-suite validation of the flipped default rides the CI
        // matrix of this PR; the bench gate measures the steady-state cost.
        EagerMerge::Default => std::env::var_os("Z42_EAGER_MERGE").is_none(),
        EagerMerge::Off => true,
    };
    let (final_module, eager_packages) = if no_eager_merge {
        let mut entry = user_artifact.module;
        entry.name = user_module_name;
        crate::metadata::types::build_type_registry(&mut entry);
        crate::metadata::types::verify_constraints(&entry)
            .with_context(|| format!("constraint verification failed for `{}`", file))?;
        crate::loader::build_block_indices(&mut entry);
        crate::loader::package::build_func_index(&mut entry);
        (entry, artifacts)
    } else {
        let mut modules: Vec<crate::metadata::Module> =
            artifacts.into_iter().map(|a| a.module).collect();
        modules.push(user_artifact.module);
        let merged = if modules.len() == 1 {
            modules.into_iter().next().ok_or_else(|| anyhow::anyhow!("no module to load"))?
        } else {
            let mut m = crate::loader::merge_modules(modules)
                .with_context(|| format!("merging modules for `{}`", file))?;
            m.name = user_module_name;
            crate::metadata::types::build_type_registry(&mut m);
            crate::metadata::types::verify_constraints(&m)
                .with_context(|| format!("constraint verification failed for `{}`", file))?;
            crate::loader::build_block_indices(&mut m);
            crate::loader::package::build_func_index(&mut m);
            m
        };
        (merged, Vec::new())
    };

    let jit_config = crate::config::runtime_config_snapshot().jit_config();
    let ctx = crate::vm::assembly::build(final_module, crate::vm::assembly::ProgramInputs {
        vfs: Arc::clone(&vfs),
        search_dirs: search_dirs.clone(),
        declared_candidates,
        initially_loaded: initially_loaded_zpkgs,
        eager_impl_pairs,
        eager_deps,
        eager_packages,
    }, crate::vm::assembly::AssemblyOptions {
        native: options.native.clone(), mode: options.mode, jit_config, components,
        freeze_extensions: crate::composition::freeze,
    }, loaded_for_replay)?;
    LoadedProgram::new(ctx, options.mode, entry_hint)
}

/// Which eager-loading track `prepare` takes. `Default` reads the
/// `Z42_EAGER_MERGE` escape hatch (per-package is the default since the M4
/// dry run); `Off` forces per-package registration (tests drive this
/// directly — the env var is process-global).
pub(super) enum EagerMerge {
    Default,
    Off,
}

fn visited_resolver_namespace(options: &LoadOptions, loaded: &[String], namespace: &str) -> bool {
    options.resolver.is_some() && loaded.iter().any(|n| n == &format!("{namespace}.zpkg"))
}
/// 入口 artifact 的三重登记（fix-entry-self-reload）：
/// · `loaded_for_replay` —— observer 的 `ModuleLoaded` 事件重放；
/// · `initially_loaded_zpkgs` —— **入口自己的文件名**也进惰性加载器的已载集。缺了它，入口仍是
///   惰性候选（入口目录正是第一个搜索目录），任何一次 provider / fallback 加载都会把**正在运行
///   的入口**从磁盘再读一遍：全部符号二次注册、逐类型记歧义，随后第一个入口类型的实例化当场拒用
///   （自举 driver dist 实测：`Z42.Driver.* is provided by more than one loaded package`）；
/// · `loaded_paths` —— AOT 急切 BFS 的 canonical-path 去重集，防依赖链绕回入口时重复合并。
/// 裸 `.zbc` 的 basename 不会与任何 zpkg 候选撞名，登记无副作用。
fn register_entry_as_loaded(
    file: &str,
    loaded_paths: &mut HashSet<PathBuf>,
    loaded_for_replay: &mut Vec<(String, Option<u64>)>,
) {
    let byte_size = std::fs::metadata(file).ok().map(|m| m.len());
    loaded_for_replay.push((file.to_string(), byte_size));
    // 两个幂等键（zpkg 文件名 + `__loaded_path__<module>`）已由 `entry_zpkg_idempotency_keys`
    // 在 vec 初始化处统一登记（#1393）；本函数只补 replay 与 AOT 去重集——#1393 与 #1399
    // 先后落 main 时各登记了一遍，此处去重。
    if let Ok(entry_canonical) = std::path::Path::new(file).canonicalize() {
        loaded_paths.insert(entry_canonical);
    }
}

/// Decode raw artifact bytes through the optional session cache: a hit hands
/// back a shallow `from_image` view sharing definition `Arc`s with every
/// earlier program of the session; a miss decodes and freezes into the cache.
fn load_bytes_cached(
    bytes: &[u8],
    cache: Option<&crate::loader::ImageCache>,
) -> Result<LoadedArtifact> {
    match cache {
        Some(cache) => cache.artifact_for(&crate::loader::content_id(bytes), || {
            crate::loader::load_artifact_from_bytes(bytes)
        }),
        None => crate::loader::load_artifact_from_bytes(bytes),
    }
}

/// Path variant: reads through the VFS only when a cache is attached (the
/// content hash needs the bytes); the uncached path keeps the original
/// `load_artifact` behaviour byte for byte.
fn load_path_cached(
    fs: &dyn crate::pal::fs::Vfs,
    path: &str,
    cache: Option<&crate::loader::ImageCache>,
) -> Result<LoadedArtifact> {
    match cache {
        Some(cache) => {
            let bytes = fs.read(path)?;
            load_bytes_cached(&bytes, Some(cache))
        }
        None => crate::loader::load_artifact(fs, path),
    }
}
