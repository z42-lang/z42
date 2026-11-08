//! Shared artifact preparation for CLI and embedded hosts.
use std::{collections::HashSet, path::{Path, PathBuf}, sync::Arc};
use anyhow::{bail, Context, Result};
use crate::{zpkg::ExecMode, pal::fs::Vfs, vm::loader::{LoadedArtifact, lazy::ZpkgCandidate}};
use super::runtime::{LoadOptions, LoadedProgram};

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


// fix-version-mismatch-diagnosis : a .zpkg whose format version is
// not this runtime's is **not** a "skip it and carry on" condition. The strict
// pin (see format/versions.rs) means z42vm reads exactly one generation, so
// a mismatched package can never be substituted — warning and continuing boots a
// world with no stdlib and the real failure surfaces far away as a bogus
// `undefined function Std.IO.Environment.GetCommandLineArgs$0` (and on runtimes
// older than the package, as an outright hang). This is exactly the trap a stale
// `.z42` seed + a freshly built tree walks into after a format bump.
//
// So: version mismatches get named at the point of detection, with the command
// that fixes them. Every other read failure keeps its warn-and-continue
// behavior — those really can be optional / unrelated artifacts.
fn version_mismatch_hint(file: &str, e: &anyhow::Error) -> Option<String> {
    let m = crate::zpkg::format::as_version_mismatch(e)?;
    Some(format!(
        "`{file}` was built for a different z42 format version than this runtime: {m}.\n\
         z42vm and every .zpkg it loads must be the same generation — there is no \
         cross-version compatibility (strict-pin policy).\n\
         Fix by rebuilding the libraries against this runtime (`{remedy}`), or by \
         running the z42vm that produced them (`Z42_PORTABLE_VM=<matching z42vm>`).",
        remedy = m.remedy()
    ))
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

pub(super) fn prepare(user_artifact: LoadedArtifact, source_path: Option<&str>, options: &LoadOptions) -> Result<LoadedProgram> {
    let vfs = Arc::clone(&options.vfs);
    let fs = &*vfs;
    let search_dirs = options.search_dirs.clone();
    let file = source_path.unwrap_or("<memory>");
    let mut modules: Vec<crate::vm::runtime::Module> = Vec::new();
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
            let artifact = crate::vm::loader::load_artifact_from_bytes(&bytes)
                .with_context(|| format!("zpkg_resolver returned malformed bytes for namespace `{namespace}`"))?;
            initially_loaded_zpkgs.push(format!("{namespace}.zpkg"));
            if !resolved_names.insert(blake3::hash(&bytes)) { continue; }
            queue.extend(artifact.import_namespaces.iter().cloned());
            queue.extend(artifact.dependencies.iter().flat_map(|d| d.namespaces.iter().cloned()));
            loaded_for_replay.push((namespace, Some(bytes.len() as u64)));
            eager_impl_pairs.extend(artifact.impl_pairs.iter().cloned());
            modules.push(artifact.module);
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
            match crate::vm::loader::load_artifact(fs, &core_str) {
                Ok(a) => {
                    tracing::debug!("loaded stdlib z42.core from {core_str}");
                    let byte_size = std::fs::metadata(&core_path).ok().map(|m| m.len());
                    loaded_for_replay.push(("z42.core.zpkg".to_string(), byte_size));
                    eager_impl_pairs.extend(a.impl_pairs.iter().cloned());
                    modules.insert(0, a.module);
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
                let Ok(zpkg_paths) = crate::vm::loader::resolve_namespace(fs, &ns, &libs_paths)
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
            let artifact = crate::vm::loader::load_artifact(fs, &dep_str)
                .with_context(|| format!("loading dependency `{dep_str}`"))?;
            for dep in &artifact.dependencies { file_queue.push_back(dep.file.clone()); }
            for ns in &artifact.import_namespaces { ns_queue.push_back(ns.clone()); }
            eager_impl_pairs.extend(artifact.impl_pairs.iter().cloned());
            modules.push(artifact.module);
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
    modules.push(user_artifact.module);

    let final_module = if modules.len() == 1 {
        modules.into_iter().next().ok_or_else(|| anyhow::anyhow!("no module to load"))?
    } else {
        let mut m = crate::vm::loader::merge_modules(modules)
            .with_context(|| format!("merging modules for `{}`", file))?;
        m.name = user_module_name;
        crate::vm::runtime::types::build_type_registry(&mut m);
        crate::vm::runtime::types::verify_constraints(&m)
            .with_context(|| format!("constraint verification failed for `{}`", file))?;
        crate::vm::loader::package::build_block_indices(&mut m);
        crate::vm::loader::package::build_func_index(&mut m);
        m
    };

    // fix-host-static-init: availability folding → ctx → cctor registration → lazy loader
    // live in `boot::boot_context`, shared with the embedding host path.
    let ctx = crate::host::boot::boot_context(final_module, crate::host::boot::BootPlan {
        vfs: Arc::clone(&vfs),
        search_dirs: search_dirs.clone(),
        declared_candidates,
        initially_loaded: initially_loaded_zpkgs,
        eager_impl_pairs,
        eager_deps,
    });
    for (name, byte_size) in loaded_for_replay {
        ctx.fire_runtime_event(&crate::vm::diagnostics::observer::RuntimeEvent::ModuleLoaded { name, byte_size });
    }
    if let Some(module) = ctx.module() { crate::host::boot::prepare_execution(&ctx, module); }
    LoadedProgram::new(ctx, options.mode, entry_hint)
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

/// Build the declared-but-not-loaded zpkg candidate set for the lazy loader.
///
/// Sources (in order, deduped by zpkg file name):
///   1. `.zpkg` artifact's `dependencies` (DEPS section)
///   2. `.zbc`  artifact's `import_namespaces` — reverse-lookup into search dirs
///      for zpkgs declaring each namespace
/// Entries already in `initially_loaded` (e.g. eager-loaded z42.core, or AOT-merged
/// deps) are excluded.
fn build_declared_candidates(
    fs: &dyn Vfs,
    user_artifact: &LoadedArtifact,
    search_dirs: &[PathBuf],
    initially_loaded: &[String],
) -> Vec<(String, ZpkgCandidate)> {
    let mut declared: Vec<(String, ZpkgCandidate)> = Vec::new();
    if search_dirs.is_empty() { return declared; }

    let loaded_has = |name: &str| initially_loaded.iter().any(|f| f == name);
    let declared_has = |d: &[(String, _)], name: &str| d.iter().any(|(f, _)| f == name);

    let libs_paths = search_dirs.to_vec();

    // .zpkg dependencies (DEPS): file field authoritative; fall back to the
    // sibling `namespaces` field if the literal filename does not resolve.
    for dep in &user_artifact.dependencies {
        if loaded_has(&dep.file) || declared_has(&declared, &dep.file) { continue; }
        if let Ok(cand) = ZpkgCandidate::build_in_dirs(fs, search_dirs, &dep.file) {
            declared.push((dep.file.clone(), cand));
            continue;
        }
        for ns in &dep.namespaces {
            let Ok(zpkg_paths) = crate::vm::loader::resolve_namespace(fs, ns, &libs_paths) else {
                continue;
            };
            for zpkg_path in zpkg_paths {
                let Some(file_name) = zpkg_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
                else { continue };
                if loaded_has(&file_name) || declared_has(&declared, &file_name) { continue; }
                match ZpkgCandidate::build_in_dirs(fs, search_dirs, &file_name) {
                    Ok(cand) => declared.push((file_name, cand)),
                    // Left as a warning on purpose: a namespace can resolve to several
                    // candidates, so one unreadable file is not yet fatal. The hint
                    // still names the remedy when it is a version mismatch rather
                    // than a corrupt / unrelated file.
                    Err(e)   => match version_mismatch_hint(&file_name, &e) {
                        Some(hint) => tracing::warn!("{hint}"),
                        None => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
                    },
                }
            }
        }
    }

    // .zbc import_namespaces — reverse lookup
    for ns in &user_artifact.import_namespaces {
        let Ok(zpkg_paths) = crate::vm::loader::resolve_namespace(fs, ns, &libs_paths) else {
            continue;
        };
        for zpkg_path in zpkg_paths {
            let Some(file_name) = zpkg_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
            else { continue };
            if loaded_has(&file_name) { continue; }
            if declared_has(&declared, &file_name) { continue; }
            match ZpkgCandidate::build_in_dirs(fs, search_dirs, &file_name) {
                Ok(cand) => declared.push((file_name, cand)),
                // Left as a warning on purpose: a namespace can resolve to several
                // candidates, so one unreadable file is not yet fatal. The hint
                // still names the remedy when it is a version mismatch rather
                // than a corrupt / unrelated file.
                Err(e)   => match version_mismatch_hint(&file_name, &e) {
                    Some(hint) => tracing::warn!("{hint}"),
                    None => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
                },
            }
        }
    }

    declared
}
