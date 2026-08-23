//! Shared app-run core (add-embedded-app-run, 2026-08-02).
//!
//! `run()` is the single implementation of "load a z42 app (.zbc / .zpkg) and
//! execute its entry", extracted from the z42vm binary's `main.rs` so it is
//! shared by:
//!   - the `z42vm` binary (`main.rs` builds [`RunOpts`] from its CLI and calls this),
//!   - embedding (`z42-host::run_app` → `z42-abi::z42_run_app`) — desktop
//!     self-contained + wasm / iOS / Android all run apps through the same core,
//!   - any user cross-platform app packaged by the workload system.
//!
//! Behavior is a faithful move of main.rs's former load→lazy-loader→merge→
//! `vm.run` sequence; the whole golden suite is its regression net. Mode
//! resolution + CLI/process concerns stay in the caller; `run` takes a resolved
//! [`RunOpts`].

use crate::corelib::fs_backend;
use crate::metadata::lazy_loader::ZpkgCandidate;
use crate::metadata::{ExecMode, LoadedArtifact};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// add-wasm-testhost (G6): dir/exists probes go through the platform fs backend
// so `run` works on wasm (in-memory VFS) as well as native (std::fs). Native is
// byte-identical. `metadata`/`canonicalize` below stay on std::fs — they only
// feed replay byte-sizes / symlink dedup and degrade gracefully (None / raw
// path) when absent, so wasm needs no change there.
fn be_is_dir(p: &Path) -> bool {
    p.to_str().map(|s| fs_backend::active().is_dir(s)).unwrap_or(false)
}
fn be_exists(p: &Path) -> bool {
    p.to_str().map(|s| fs_backend::active().exists(s)).unwrap_or(false)
}

/// Build-default execution mode: JIT when compiled in (make-jit-default),
/// else Interp (jit-less builds — wasm / `--features interp-only`). Callers
/// without an explicit mode (embedding one-shot) use this.
pub fn default_mode() -> ExecMode {
    #[cfg(feature = "jit")]
    { ExecMode::Jit }
    #[cfg(not(feature = "jit"))]
    { ExecMode::Interp }
}

/// Resolved options for [`run`]. The caller resolves execution mode, the stdlib
/// `libs` dir, program args, and stat-printing; `run` performs the load + execute.
pub struct RunOpts {
    /// Effective execution mode (already resolved from CLI / config / build default).
    pub mode: ExecMode,
    /// Stdlib `libs/` directory (where `z42.core.zpkg` and deps live), if found.
    pub libs_dir: Option<PathBuf>,
    /// Program args forwarded to `GetCommandLineArgs()` (the app's `-- <args>`).
    pub program_args: Vec<String>,
    /// Print counter stats to stderr after the run (z42vm `--print-stats-on-exit`).
    pub print_stats: bool,
    /// When `print_stats`, emit a single-line JSON object instead of the human
    /// text block (z42vm `--stats-format=json`; `xtask profile` scrapes it).
    pub stats_json: bool,
}

/// Load the app at `file` (.zbc or .zpkg) and run its entry (`entry` overrides
/// the artifact's baked-in entry hint). Returns the program's result.
pub fn run(file: &str, entry: Option<&str>, opts: RunOpts) -> Result<()> {
    let libs_dir = opts.libs_dir;

    // Dependency search dirs (support-colocated-zpkg-deps): resolve a dep zpkg
    // from the ENTRY zpkg's own directory first, then the stdlib `libs/`. Fixed
    // order (entry dir, then libs) for deterministic resolution; de-duped.
    let search_dirs: Vec<PathBuf> = {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(entry_dir) = std::path::Path::new(file).parent() {
            let entry_dir = if entry_dir.as_os_str().is_empty() {
                PathBuf::from(".")
            } else {
                entry_dir.to_path_buf()
            };
            if be_is_dir(&entry_dir) {
                dirs.push(entry_dir);
            }
        }
        if let Some(libs) = &libs_dir {
            if !dirs.iter().any(|d| d == libs) {
                dirs.push(libs.clone());
            }
        }
        dirs
    };

    let mut modules: Vec<crate::metadata::Module> = Vec::new();
    // Track canonical paths of loaded artifact files to prevent duplicate loading.
    let mut loaded_paths: HashSet<PathBuf> = HashSet::new();
    // Track zpkg file names loaded eagerly at startup (initially_loaded input
    // for the lazy loader — these are excluded from on-demand candidate set).
    let mut initially_loaded_zpkgs: Vec<String> = Vec::new();
    // add-runtime-observer: collect (name, byte_size) for every module loaded
    // during boot, replay-emitted as `ModuleLoaded` events after the registry exists.
    let mut loaded_for_replay: Vec<(String, Option<u64>)> = Vec::new();
    // add-crosspkg-impl-reflection: collect (target_fq, trait_fq) impl pairs
    // from every eagerly-loaded artifact; seeded into the lazy loader below.
    let mut eager_impl_pairs: Vec<(String, String)> = Vec::new();

    // 5.1b — unconditionally try to load z42.core.zpkg if present.
    if let Some(ref dir) = libs_dir {
        let core_path = dir.join("z42.core.zpkg");
        if be_exists(&core_path) {
            let core_canonical = core_path.canonicalize().unwrap_or(core_path.clone());
            let core_str = core_path.to_string_lossy().into_owned();
            match crate::metadata::load_artifact(&core_str) {
                Ok(a) => {
                    tracing::debug!("loaded stdlib z42.core from {core_str}");
                    let byte_size = std::fs::metadata(&core_path).ok().map(|m| m.len());
                    loaded_for_replay.push(("z42.core.zpkg".to_string(), byte_size));
                    eager_impl_pairs.extend(a.impl_pairs.iter().cloned());
                    modules.push(a.module);
                    loaded_paths.insert(core_canonical);
                    initially_loaded_zpkgs.push("z42.core.zpkg".to_string());
                }
                Err(e) => tracing::warn!("failed to load z42.core: {e:#}"),
            }
        } else {
            tracing::debug!("z42.core.zpkg not found in {}", dir.display());
        }
    }

    // 5.1c — load the user artifact.
    let user_artifact = crate::metadata::load_artifact(file)?;
    {
        let byte_size = std::fs::metadata(file).ok().map(|m| m.len());
        loaded_for_replay.push((file.to_string(), byte_size));
    }

    // 5.1d — dependency loading strategy: Interp / JIT → pure lazy (declared
    // candidates drive on-demand load); AOT → eager transitive BFS (whole
    // program compiled ahead of time, every callee merged up front).
    let is_eager = matches!(opts.mode, ExecMode::Aot);
    if is_eager && !search_dirs.is_empty() {
        use std::collections::VecDeque;
        let libs_paths = search_dirs.clone();
        let mut file_queue: VecDeque<String> =
            user_artifact.dependencies.iter().map(|d| d.file.clone()).collect();
        let mut ns_queue: VecDeque<String> =
            user_artifact.import_namespaces.iter().cloned().collect();
        loop {
            while let Some(ns) = ns_queue.pop_front() {
                let Ok(zpkg_paths) = crate::metadata::resolve_namespace(&ns, &[], &libs_paths)
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
                .find(|p| p.exists())
            else { continue };
            let canonical = dep_path.canonicalize().unwrap_or_else(|_| dep_path.clone());
            if !loaded_paths.insert(canonical) { continue; }  // already merged
            let dep_str = dep_path.to_string_lossy().into_owned();
            if let Ok(a) = crate::metadata::load_artifact(&dep_str) {
                for d in &a.dependencies { file_queue.push_back(d.file.clone()); }
                for ns in &a.import_namespaces { ns_queue.push_back(ns.clone()); }
                eager_impl_pairs.extend(a.impl_pairs.iter().cloned());
                modules.push(a.module);
                initially_loaded_zpkgs.push(dep_file.clone());
            }
        }
    }

    // Build declared-but-not-loaded zpkg candidate set for the lazy loader,
    // BEFORE moving `user_artifact.module` into `modules` (partial-move).
    let declared_candidates = build_declared_candidates(
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
        modules.into_iter().next().unwrap()
    } else {
        let mut m = crate::metadata::merge_modules(modules)
            .with_context(|| format!("merging modules for `{}`", file))?;
        m.name = user_module_name;
        crate::metadata::loader::build_type_registry(&mut m);
        crate::metadata::loader::verify_constraints(&m)
            .with_context(|| format!("constraint verification failed for `{}`", file))?;
        crate::metadata::loader::build_block_indices(&mut m);
        crate::metadata::loader::build_func_index(&mut m);
        m
    };

    // Construct the VmContext (owns static-fields / pending-exception / lazy_loader).
    let string_pool_len = final_module.string_pool.len();
    let ctx = crate::vm_context::VmContext::with_module(final_module);
    // Forward `-- <args>` to the program's GetCommandLineArgs() before vm.run.
    ctx.set_program_args(opts.program_args.clone());
    ctx.install_lazy_loader_with_deps(
        search_dirs.clone(),
        string_pool_len,
        declared_candidates,
        initially_loaded_zpkgs,
    );
    // Seed lazy loader with merged module's TypeDescs (cross-zpkg base classes)
    // and eagerly-loaded artifacts' impl pairs.
    let type_registry = ctx.module().unwrap().type_registry.clone();
    ctx.seed_lazy_loader_types(&type_registry);
    ctx.seed_lazy_loader_impls(&eager_impl_pairs);

    // Replay-emit ModuleLoaded for every module loaded during boot.
    for (name, byte_size) in loaded_for_replay.drain(..) {
        ctx.fire_runtime_event(&crate::observer::RuntimeEvent::ModuleLoaded { name, byte_size });
    }

    let vm = crate::vm::Vm::new(opts.mode);
    // Caller-supplied `entry` overrides any artifact-supplied entry hint.
    let effective_entry = entry.or(entry_hint.as_deref());
    let result = vm.run(&*ctx, effective_entry);

    // --print-stats-on-exit: snapshot counters AFTER vm.run (even on error).
    // JSON form (--stats-format=json) is one line for `xtask profile` to scrape;
    // both go to stderr so program stdout stays the script's own output.
    // Merge the RuntimeCounters snapshot with heap-owned numbers (allocations +
    // GC minor/major/reclaimed) into one ProfileSnapshot so the profile picture
    // is complete in a single line/block.
    if opts.print_stats {
        let counters = ctx.counters().snapshot();
        let h = ctx.heap().stats();
        // add-concurrency-probes (P1b): safepoint-park distribution (always-on)
        // + user-lock contention counters (0 unless the `profile-contention`
        // feature is built in).
        let (park_count, park_us_total, park_max_us) = {
            let ph = ctx.core.park_histogram.lock();
            (ph.count, ph.total_us, if ph.count == 0 { 0 } else { ph.max_us })
        };
        use std::sync::atomic::Ordering::Relaxed;
        let snap = crate::counters::ProfileSnapshot::new(
            counters, h.allocations,
            h.minor_collections, h.major_collections, h.reclaimed_bytes,
        )
        .with_concurrency(
            park_count, park_us_total, park_max_us,
            ctx.core.lock_contentions.load(Relaxed),
            ctx.core.lock_wait_us.load(Relaxed),
        );
        if opts.stats_json {
            eprintln!("{}", snap.to_json());
        } else {
            eprintln!("{snap}");
        }
    }

    result
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
        if let Ok(cand) = ZpkgCandidate::build_in_dirs(search_dirs, &dep.file) {
            declared.push((dep.file.clone(), cand));
            continue;
        }
        for ns in &dep.namespaces {
            let Ok(zpkg_paths) = crate::metadata::resolve_namespace(ns, &[], &libs_paths) else {
                continue;
            };
            for zpkg_path in zpkg_paths {
                let Some(file_name) = zpkg_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
                else { continue };
                if loaded_has(&file_name) || declared_has(&declared, &file_name) { continue; }
                match ZpkgCandidate::build_in_dirs(search_dirs, &file_name) {
                    Ok(cand) => declared.push((file_name, cand)),
                    Err(e)   => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
                }
            }
        }
    }

    // .zbc import_namespaces — reverse lookup
    for ns in &user_artifact.import_namespaces {
        let Ok(zpkg_paths) = crate::metadata::resolve_namespace(ns, &[], &libs_paths) else {
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
            match ZpkgCandidate::build_in_dirs(search_dirs, &file_name) {
                Ok(cand) => declared.push((file_name, cand)),
                Err(e)   => tracing::warn!("cannot read zpkg meta `{}`: {e}", file_name),
            }
        }
    }

    declared
}
