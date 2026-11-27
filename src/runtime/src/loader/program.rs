//! Prepare loaded modules for VM assembly.
use anyhow::{Context, Result};
use std::{collections::HashSet, path::PathBuf};

use super::{
    decode::FUSION_INPUTS,
    availability::AvailabilityStats,
    build_block_indices,
    dependency_candidates::build_declared_candidates,
    eager::load_eager_dependencies,
    decode_program_bytes_cached,
    package::{build_func_index, prepare_module},
    prelude::{load_prelude, PRELUDE_FILE},
    DecodedProgram, ZpkgCandidate,
};
use crate::{metadata::Module, pal::fs::Vfs};

/// Package inputs collected before VM construction.
pub(crate) struct StartupPackages {
    pub(super) entry_identity: Option<super::package_identity::PackageIdentity>,
    pub(crate) search_dirs: Vec<std::path::PathBuf>,
    pub(crate) declared_candidates: Vec<(String, ZpkgCandidate)>,
    pub(crate) initially_loaded: Vec<String>,
    pub(crate) impl_pairs: Vec<(String, String)>,
    pub(crate) entry_dependencies: Vec<crate::zpkg::format::ZpkgDep>,
    pub(crate) programs: Vec<DecodedProgram>,
}

/// Fold against declared packages and rebuild execution indices when the module changes.
pub(crate) fn prepare_program_availability(
    fs: &dyn Vfs,
    module: &mut Module,
    packages: &StartupPackages,
) -> Option<AvailabilityStats> {
    let stats = super::availability::fold_availability_with_fusion(
        fs, module, &packages.declared_candidates, &packages.programs, FUSION_INPUTS,
    );
    if stats.is_noop() {
        return None;
    }

    build_block_indices(module);
    build_func_index(module);
    Some(stats)
}

/// Resolve a namespace name (as written in z42 source, e.g. `"Std.IO"`
/// or `"z42.core"`) to its zpkg bytes.
///
/// Return `None` to signal "this resolver doesn't know about that
/// namespace"; the runtime continues with the next fallback (typically
/// `search_paths`).
pub trait ZpkgResolver: Send + Sync {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>>;
}

/// Read a package's resolver keys: its nonempty package name, then NSPC namespaces.
pub fn read_zpkg_namespaces(bytes: &[u8]) -> anyhow::Result<Vec<String>> {
    let info = crate::zpkg::reader::read_zpkg_meta(bytes)?;
    let mut keys = Vec::with_capacity(info.namespaces.len() + 1);
    if !info.name.is_empty() {
        keys.push(info.name);
    }
    keys.extend(info.namespaces);
    Ok(keys)
}

pub(crate) struct PreparedProgram {
    pub(crate) module: Module,
    pub(crate) packages: StartupPackages,
    pub(crate) entry_hint: Option<String>,
    pub(crate) initial_events: Vec<(String, Option<u64>)>,
}

fn entry_zpkg_idempotency_keys(file: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(name) = std::path::Path::new(file)
        .file_name()
        .and_then(|n| n.to_str())
    {
        keys.push(name.to_string());
        if let Some(stem) = name.strip_suffix(".zpkg") {
            keys.push(format!("__loaded_path__{stem}"));
        }
    }
    keys
}

pub(crate) fn prepare_startup(
    entry_program: DecodedProgram,
    source_path: Option<&str>,
    logical_name: Option<&str>,
    fs: &dyn Vfs,
    search_dirs: Vec<PathBuf>,
    resolver: Option<&dyn ZpkgResolver>,
    file_resolver: Option<&dyn super::FileResolver>,
    eager_dependencies: bool,
    image_cache: Option<&super::ImageCache>,
) -> Result<PreparedProgram> {
    let file = logical_name.unwrap_or("<memory>");
    // Deps keep their whole decoded (M1 no-eager-merge registers them
    // per-package after construction); the merged path maps them to modules.
    let mut programs: Vec<crate::loader::DecodedProgram> = Vec::new();
    // Track canonical paths of loaded program files to prevent duplicate loading.
    let mut loaded_paths: HashSet<PathBuf> = HashSet::new();
    // Track zpkg file names loaded eagerly at startup (initially_loaded input
    // for the lazy loader — these are excluded from on-demand candidate set).
    // Seeded with the entry zpkg's two idempotency keys (see `entry_zpkg_idempotency_keys`).
    let mut initially_loaded_zpkgs = logical_name
        .map(entry_zpkg_idempotency_keys)
        .unwrap_or_default();
    // add-runtime-observer: collect (name, byte_size) for every module loaded
    // during boot, replay-emitted as `ModuleLoaded` events after the registry exists.
    let mut loaded_for_replay: Vec<(String, Option<u64>)> = Vec::new();
    // add-crosspkg-impl-reflection: collect (target_fq, trait_fq) impl pairs
    // from every eagerly-loaded decoded; seeded into the lazy loader below.
    let mut eager_impl_pairs: Vec<(String, String)> = Vec::new();

    let mut resolved_names = HashSet::new();
    if let Some(resolver) = file_resolver {
        let resolved = super::eager::load_resolved_dependencies(
            resolver,
            fs,
            &search_dirs,
            &entry_program,
            logical_name,
            image_cache,
            &mut initially_loaded_zpkgs,
        )?;
        for (name, program, byte_size) in resolved {
            loaded_for_replay.push((name, byte_size));
            eager_impl_pairs.extend(program.impl_pairs.iter().cloned());
            programs.push(program);
        }
    } else if let Some(resolver) = resolver {
        let mut queue = std::collections::VecDeque::from(["z42.core".to_owned()]);
        queue.extend(entry_program.import_namespaces.iter().cloned());
        queue.extend(
            entry_program
                .dependencies
                .iter()
                .flat_map(|d| d.namespaces.iter().cloned()),
        );
        let mut visited = HashSet::new();
        while let Some(namespace) = queue.pop_front() {
            if !visited.insert(namespace.clone()) {
                continue;
            }
            let Some(bytes) = resolver.resolve(&namespace) else {
                continue;
            };
            let decoded =
                decode_program_bytes_cached(&bytes, image_cache).with_context(|| {
                    format!("zpkg_resolver returned malformed bytes for namespace `{namespace}`")
                })?;
            initially_loaded_zpkgs.push(format!("{namespace}.zpkg"));
            if !resolved_names.insert(blake3::hash(&bytes)) {
                continue;
            }
            queue.extend(decoded.import_namespaces.iter().cloned());
            queue.extend(
                decoded
                    .dependencies
                    .iter()
                    .flat_map(|d| d.namespaces.iter().cloned()),
            );
            loaded_for_replay.push((namespace, Some(bytes.len() as u64)));
            eager_impl_pairs.extend(decoded.impl_pairs.iter().cloned());
            programs.push(decoded);
        }
    }
    let prelude_resolved = initially_loaded_zpkgs
        .iter()
        .any(|name| name == PRELUDE_FILE);
    if let Some(prelude) = load_prelude(fs, &search_dirs, image_cache, prelude_resolved)? {
        let byte_size = std::fs::metadata(&prelude.path).ok().map(|m| m.len());
        loaded_for_replay.push((PRELUDE_FILE.to_owned(), byte_size));
        eager_impl_pairs.extend(prelude.decoded.impl_pairs.iter().cloned());
        programs.insert(0, prelude.decoded);
        loaded_paths.insert(prelude.canonical_path);
        initially_loaded_zpkgs.push(PRELUDE_FILE.to_owned());
    }

    if let Some(path) = source_path {
        register_entry_as_loaded(path, &mut loaded_paths, &mut loaded_for_replay);
    }

    // 5.1d — dependency loading strategy: Interp / JIT → pure lazy (declared
    // candidates drive on-demand load); AOT → eager transitive BFS (whole
    // program compiled ahead of time, every callee merged up front).
    if file_resolver.is_none() && eager_dependencies && !search_dirs.is_empty() {
        for decoded in load_eager_dependencies(
            fs,
            &search_dirs,
            &entry_program,
            image_cache,
            resolver.is_some(),
            &mut loaded_paths,
            &mut initially_loaded_zpkgs,
        )? {
            eager_impl_pairs.extend(decoded.impl_pairs.iter().cloned());
            programs.push(decoded);
        }
    }

    // precise-pkg-refs: the entry program's DEPS symbol lists seed the loader's routing table.
    let eager_deps = entry_program.dependencies.clone();
    // Build declared-but-not-loaded zpkg candidate set for the lazy loader,
    // BEFORE moving `entry_program.module` into `modules` (partial-move).
    let declared_candidates = if file_resolver.is_none() {
        build_declared_candidates(fs, &entry_program, &search_dirs, &initially_loaded_zpkgs)
    } else {
        Vec::new()
    };

    let entry_hint = entry_program.entry_hint.clone();
    let entry_identity = super::package_identity::PackageIdentity::from_program(&entry_program);
    eager_impl_pairs.extend(entry_program.impl_pairs.iter().cloned());

    // Dependencies keep their own definitions and are registered after construction.
    let mut module = entry_program.module;
    prepare_module(&mut module, FUSION_INPUTS)
        .with_context(|| format!("constraint verification failed for `{file}`"))?;

    Ok(PreparedProgram {
        module,
        packages: StartupPackages {
            entry_identity,
            search_dirs,
            declared_candidates,
            initially_loaded: initially_loaded_zpkgs,
            impl_pairs: eager_impl_pairs,
            entry_dependencies: eager_deps,
            programs,
        },
        entry_hint,
        initial_events: loaded_for_replay,
    })
}

/// 入口 decoded 的三重登记（fix-entry-self-reload）：
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
