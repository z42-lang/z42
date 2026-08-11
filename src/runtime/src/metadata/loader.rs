/// Artifact loader: detects the format of a compiler output file by extension,
/// deserialises it (binary only), and returns a merged `Module` plus metadata.
///
/// Supported formats:
///   `.zbc`  — ZbcFile binary (single source file, full mode)
///   `.zpkg` — ZpkgFile binary (project package; packed mode only)
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

// add-wasm-testhost (G6): artifact reads route through the platform fs backend
// (`native` = std::fs, byte-identical; `memory` = in-memory VFS on wasm) so a
// wasm host that mounts test zbcs / stdlib zpkgs can load them at runtime. On
// native this is exactly std::fs — no behaviour change.
use crate::corelib::fs_backend;

use super::bytecode::Module;
use super::formats::{ZpkgDep, ZBC_MAGIC, ZPKG_MAGIC};
use super::merge::merge_modules;
use super::test_index::TestEntry;
use super::name_index::NameIndex;
use super::types::{FieldSlot, TypeDesc};
use super::zbc_reader::{
    parse_zbc_sidecar, parse_zpkg_sidecar, read_build_id, read_directory_pub,
    read_test_index_resolved, read_zbc, read_zpkg_file_entries, read_zpkg_meta,
    read_zpkg_modules, read_zpkg_namespaces,
};

/// Result of loading a compiler artifact.
pub struct LoadedArtifact {
    /// The merged, flat IR module ready for the VM.
    pub module: Module,
    /// Entry-point function name from the artifact's metadata, if present.
    pub entry_hint: Option<String>,
    /// Resolved dependency list from the zpkg manifest (empty for .zbc).
    pub dependencies: Vec<ZpkgDep>,
    /// Namespace prefixes extracted from the import table (populated by load_zbc).
    /// Used by main.rs to load the corresponding zpkgs.
    pub import_namespaces: Vec<String>,
    /// Spec R1 (add-test-metadata-section) — compile-time test metadata
    /// extracted from the zbc TIDX section. Empty when the artifact has no
    /// `[Test]`/`[Benchmark]`/etc.-decorated functions or the section is absent
    /// (older artifacts). Consumed by R3 z42-test-runner.
    pub test_index: Vec<TestEntry>,
    /// add-crosspkg-impl-reflection (unify P1-e): `(target_fq, trait_fq)`
    /// pairs from the zpkg IMPL section (`impl Trait for Type` declared in
    /// this package). Empty for .zbc. Merged into the lazy loader's impls
    /// registry so `Type.GetInterfaces()` sees cross-package traits.
    pub impl_pairs: Vec<(String, String)>,
    /// zpkg package name (`ZpkgFile.name`, e.g. `"repl_r1"`); `None` for a bare
    /// `.zbc`. On load the lazy loader marks `<package_name>.zpkg` as resident so a
    /// later-loaded dependent's dep-resolution loop recognises an already-loaded
    /// package instead of probing disk. Fixes the spurious "cannot read dep zpkg
    /// meta `repl_rN.zpkg`" WARN for REPL rounds compiled to bytes in memory and
    /// never written to disk. (fix-repl-inmemory-dep-warn)
    pub package_name: Option<String>,
}

/// Load a compiler output artifact from `path`, returning a `LoadedArtifact`.
///
/// Format is determined by file extension (case-insensitive):
/// - `.zbc`  → binary zbc (full mode)
/// - `.zpkg` → binary zpkg (packed mode)
pub fn load_artifact(path: &str) -> Result<LoadedArtifact> {
    match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("zbc")  => load_zbc(path),
        Some("zpkg") => load_zpkg(path),
        ext => bail!(
            "unrecognised artifact extension {:?} in `{}`; expected .zbc or .zpkg",
            ext, path
        ),
    }
}

/// Load a compiler output artifact from in-memory bytes, returning a
/// `LoadedArtifact`. Format is detected by magic bytes.
///
/// Used by the embedding API (`z42_host_load_zbc`) where the host hands
/// the runtime raw bytes rather than a filesystem path. Behaviour
/// mirrors [`load_artifact`] modulo the source of the byte stream;
/// the same registry / verification / index passes run.
///
/// Spec: docs/design/runtime/embedding.md §4.4 (z42_host_load_zbc),
///       docs/spec/archive/2026-05-10-add-embedding-api/.
pub fn load_artifact_from_bytes(raw: &[u8]) -> Result<LoadedArtifact> {
    if raw.len() < 4 {
        bail!("artifact byte buffer is too short ({} bytes); expected at least 4 magic bytes", raw.len());
    }
    let magic = &raw[0..4];
    if magic == ZBC_MAGIC {
        load_zbc_bytes(raw)
    } else if magic == ZPKG_MAGIC {
        load_zpkg_bytes(raw)
    } else {
        bail!(
            "unrecognised artifact magic {:02x?}; expected ZBC ({:02x?}) or ZPKG ({:02x?})",
            magic,
            ZBC_MAGIC,
            ZPKG_MAGIC
        );
    }
}

// ── Format-specific loaders ───────────────────────────────────────────────────

fn load_zbc(path: &str) -> Result<LoadedArtifact> {
    let raw = fs_backend::active().read(path).with_context(|| format!("cannot read `{path}`"))?;
    let mut artifact = load_zbc_bytes(&raw).with_context(|| format!("cannot parse binary zbc `{path}`"))?;

    // 1.2 split-debug-symbols: probe `<basename>.zsym` adjacent to the main file
    // and merge debug info when build_id matches.
    let sidecar_path = Path::new(path).with_extension("zsym");
    if let Some(sp) = sidecar_path.to_str() {
        if let Ok(sym_raw) = fs_backend::active().read(sp) {
            apply_zbc_sidecar(&mut artifact.module, &raw, &sym_raw, &sidecar_path);
        }
    }

    Ok(artifact)
}

fn apply_zbc_sidecar(
    module: &mut Module,
    main: &[u8],
    sym: &[u8],
    sym_path: &Path,
) {
    let main_blid = match read_build_id(main) {
        Some(b) => b,
        None => {
            tracing::warn!(
                "found {} but main zbc has no BLID section; ignoring sidecar",
                sym_path.display()
            );
            return;
        }
    };
    let sidecar = match parse_zbc_sidecar(sym) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ignoring corrupt zbc sidecar {}: {e}", sym_path.display());
            return;
        }
    };
    if sidecar.build_id != main_blid {
        tracing::warn!(
            "{} build_id mismatch: main={} sidecar={}; ignored",
            sym_path.display(),
            super::build_id::short_hex(&main_blid),
            super::build_id::short_hex(&sidecar.build_id),
        );
        return;
    }
    if sidecar.functions.len() != module.functions.len() {
        tracing::warn!(
            "{} function count mismatch: main has {} sidecar has {}; ignored",
            sym_path.display(),
            module.functions.len(),
            sidecar.functions.len(),
        );
        return;
    }
    for (i, fb) in sidecar.functions.into_iter().enumerate() {
        if !fb.line_table.is_empty() {
            module.functions[i].cold_mut().line_table = fb.line_table.into_boxed_slice();
        }
        if !fb.local_vars.is_empty() {
            module.functions[i].cold_mut().local_vars = fb.local_vars.into_boxed_slice();
        }
    }
}

fn load_zbc_bytes(raw: &[u8]) -> Result<LoadedArtifact> {
    if raw.len() < 4 || &raw[0..4] != ZBC_MAGIC {
        bail!("not a binary zbc payload: expected ZBC magic bytes");
    }

    let mut module = read_zbc(raw).context("cannot parse binary zbc")?;

    build_interned_strings(&mut module);
    build_type_registry(&mut module);
    verify_constraints(&module)
        .with_context(|| format!("constraint verification failed for module `{}`", module.name))?;
    build_block_indices(&mut module);
    build_func_index(&mut module);

    let import_namespaces = extract_import_namespaces_from_module(&module);

    let test_index =
        read_test_index_resolved(raw).context("cannot read TIDX section")?;

    Ok(LoadedArtifact {
        module,
        entry_hint: None,
        dependencies: vec![],
        import_namespaces,
        test_index,
        impl_pairs: vec![],
        package_name: None, // bare .zbc carries no zpkg package name
    })
}

fn load_zpkg(path: &str) -> Result<LoadedArtifact> {
    let raw = fs_backend::active().read(path).with_context(|| format!("cannot read `{path}`"))?;

    // add-indexed-zpkg-min-patch (zpkg 0.24): indexed main file (flags bit0
    // clear, not a SymOnly sidecar) → load scattered self-contained zbc via
    // the FILE directory. Path-aware only (needs the package directory).
    if raw.len() >= 10 {
        let flags = u16::from_le_bytes([raw[8], raw[9]]);
        if (flags & 0x01) == 0 && (flags & 0x04) == 0 {
            return load_zpkg_indexed(path, &raw)
                .with_context(|| format!("cannot load indexed zpkg `{path}`"));
        }
    }

    // 1.5b split-debug-symbols: probe `<basename>.zsym` adjacent to the main
    // .zpkg and merge per-module debug info into the loaded modules when
    // build_id matches. We do this before merge_modules so that line tables
    // land in the right place (before namespace flattening).
    let sidecar_path = Path::new(path).with_extension("zsym");
    let sidecar_raw = sidecar_path.to_str().and_then(|sp| fs_backend::active().read(sp).ok());

    load_zpkg_bytes_with_sidecar(&raw, sidecar_raw.as_deref(), Some(&sidecar_path))
        .with_context(|| format!("cannot parse zpkg `{path}`"))
}

/// Indexed zpkg load (add-indexed-zpkg-min-patch): read the FILE directory,
/// load each scattered `<stem>.zbc` (self-contained fullMode) relative to the
/// main file's directory, verify its BLAKE3-128 content hash against the
/// index, then run the same aggregation pipeline as packed. No `.zsym`
/// pairing — indexed is dev-mode (DBUG inline in each scattered zbc).
fn load_zpkg_indexed(path: &str, raw: &[u8]) -> Result<LoadedArtifact> {
    let meta = read_zpkg_meta(raw).context("cannot read zpkg metadata")?;
    let entries = read_zpkg_file_entries(raw).context("cannot read indexed FILE directory")?;
    let base = Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let mut module_triples = Vec::with_capacity(entries.len());
    for e in &entries {
        let stem = e.rel.strip_suffix(".z42").unwrap_or(&e.rel);
        let zbc_path = base.join(format!("{stem}.zbc"));
        let zbc_path_str = zbc_path.to_str().with_context(|| {
            format!("indexed zpkg: non-UTF8 scattered zbc path `{}`", zbc_path.display())
        })?;
        let bytes = fs_backend::active().read(zbc_path_str).with_context(|| {
            format!("indexed zpkg: cannot read scattered zbc `{}`", zbc_path.display())
        })?;
        // plain BLAKE3-128（区别于 build_id::compute 的"尾 16B 清零"BLID 语义——
        // 那是给自含 BLID 段的文件用的；散装 zbc 是内容原样哈希）。
        let got = hex_lower(&blake3::hash(&bytes).as_bytes()[..16]);
        if got != e.zbc_hash {
            bail!(
                "indexed zpkg: `{}` content hash mismatch (index has {}, file is {}) — \
                 scattered zbc out of sync with the main index; rebuild the package",
                zbc_path.display(), e.zbc_hash, got
            );
        }
        let module = read_zbc(&bytes)
            .with_context(|| format!("decoding scattered zbc `{}`", zbc_path.display()))?;
        let tidx = zbc_tidx_bytes(&bytes)?;
        module_triples.push((module, e.namespace.clone(), tidx));
    }
    assemble_zpkg_artifact(meta.entry, meta.dependencies, module_triples,
        crate::metadata::zbc_reader::read_zpkg_impl_pairs(raw).context("cannot read zpkg IMPL section")?,
        Some(meta.name))
}

/// Verbatim TIDX section payload of a standalone zbc (empty when absent) —
/// feeds the same per-module TIDX aggregation as packed MODS bodies.
fn zbc_tidx_bytes(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        return Ok(Vec::new());
    }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory_pub(data, sec_count)?;
    Ok(dir
        .get(b"TIDX")
        .map(|&(off, len)| data[off..off + len].to_vec())
        .unwrap_or_default())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn load_zpkg_bytes_with_sidecar(
    raw: &[u8],
    sym_raw: Option<&[u8]>,
    sym_path: Option<&Path>,
) -> Result<LoadedArtifact> {
    if raw.len() < 4 || &raw[0..4] != ZPKG_MAGIC {
        bail!("not a binary zpkg payload: expected ZPKG magic bytes");
    }

    let meta = read_zpkg_meta(raw).context("cannot read zpkg metadata")?;
    let mut module_triples = read_zpkg_modules(raw).context("cannot load modules from zpkg")?;

    // Apply sidecar (if present + valid + build_id matches) to per-module
    // function debug fields before flattening.
    if let Some(sym) = sym_raw {
        apply_zpkg_sidecar(&mut module_triples, raw, sym, sym_path);
    }

    // aggregate-zpkg-tidx (2026-06-06): walk per-module TIDX bytes BEFORE
    // moving the modules into merge_modules — we need each module's local
    // function count + string pool length to compute the cumulative
    // offsets that remap `method_id` and `*_str_idx` into the merged
    // module's index space. Empty `tidx_bytes` (modules with no [Test])
    // contribute zero entries but still bump the offset counters.
    assemble_zpkg_artifact(meta.entry, meta.dependencies, module_triples,
        crate::metadata::zbc_reader::read_zpkg_impl_pairs(raw).context("cannot read zpkg IMPL section")?,
        Some(meta.name))
}

/// Shared tail of every zpkg load path (packed by path / packed by bytes /
/// indexed): TIDX aggregation → merge → registries/indices → test-string
/// resolution. Extracted by add-indexed-zpkg-min-patch so indexed reuses the
/// exact packed pipeline (zero divergence after module_triples).
fn assemble_zpkg_artifact(
    entry_hint: Option<String>,
    dependencies: Vec<ZpkgDep>,
    module_triples: Vec<(Module, String, Vec<u8>)>,
    impl_pairs: Vec<(String, String)>,
    package_name: Option<String>,
) -> Result<LoadedArtifact> {
    let aggregated_test_index =
        aggregate_zpkg_test_index(&module_triples).context("aggregating zpkg TIDX entries")?;

    let modules: Vec<Module> = module_triples.into_iter().map(|(m, _, _)| m).collect();
    let mut module = merge_modules(modules).context("merging zpkg modules")?;

    // interned_strings: populated inside merge_modules.
    build_type_registry(&mut module);
    verify_constraints(&module)
        .with_context(|| format!("constraint verification failed for module `{}`", module.name))?;
    build_block_indices(&mut module);
    build_func_index(&mut module);

    // Resolve `*_str_idx` → `Option<String>` against the merged pool BEFORE
    // `rebuild_string_pool` is called (it isn't here — `merge_modules`
    // concatenates pools verbatim), so the resolved strings stay valid for
    // the runner.
    let mut test_index = aggregated_test_index;
    super::test_index::resolve_test_index_strings(&mut test_index, &module.string_pool);

    Ok(LoadedArtifact {
        module,
        entry_hint,
        dependencies,
        import_namespaces: vec![],
        test_index,
        impl_pairs,
        package_name,
    })
}

/// aggregate-zpkg-tidx (2026-06-06): merge per-module TIDX bytes into a
/// flat `Vec<TestEntry>` whose `method_id` / `*_str_idx` values reference
/// the merged module's `functions[]` / `string_pool[]` index space.
///
/// Cumulative offsets mirror `merge_modules`' string-pool / function-list
/// concatenation order: module N's local indices land at
/// `[prev modules' counts, …]`. `*_str_idx == 0` ("no string") stays 0
/// since it's a sentinel, not a real index.
pub(crate) fn aggregate_zpkg_test_index(
    module_triples: &[(super::bytecode::Module, String, Vec<u8>)],
) -> Result<Vec<super::test_index::TestEntry>> {
    let mut aggregated = Vec::new();
    let mut cumulative_func: u32 = 0;
    let mut cumulative_str:  u32 = 0;
    for (module, _ns, tidx_bytes) in module_triples {
        let func_offset = cumulative_func;
        let str_offset  = cumulative_str;
        cumulative_func = cumulative_func.saturating_add(module.functions.len() as u32);
        cumulative_str  = cumulative_str.saturating_add(module.string_pool.len() as u32);
        if tidx_bytes.is_empty() { continue; }
        let mut entries = super::test_index::read_test_index(tidx_bytes)
            .context("decoding per-module TIDX in zpkg")?;
        for e in entries.iter_mut() {
            // `method_id` is a 0-based index into module.functions[];
            // bump by cumulative function count of prior modules.
            e.method_id = e.method_id.saturating_add(func_offset);
            // `*_str_idx` is 1-based with 0 = absent. Only adjust when
            // non-zero.
            if e.skip_reason_str_idx       != 0 { e.skip_reason_str_idx       = e.skip_reason_str_idx.saturating_add(str_offset); }
            if e.skip_platform_str_idx     != 0 { e.skip_platform_str_idx     = e.skip_platform_str_idx.saturating_add(str_offset); }
            if e.skip_feature_str_idx      != 0 { e.skip_feature_str_idx      = e.skip_feature_str_idx.saturating_add(str_offset); }
            if e.expected_throw_type_idx   != 0 { e.expected_throw_type_idx   = e.expected_throw_type_idx.saturating_add(str_offset); }
            for tc in e.test_cases.iter_mut() {
                if tc.arg_repr_str_idx != 0 { tc.arg_repr_str_idx = tc.arg_repr_str_idx.saturating_add(str_offset); }
            }
        }
        aggregated.extend(entries);
    }
    Ok(aggregated)
}

fn apply_zpkg_sidecar(
    module_pairs: &mut Vec<(Module, String, Vec<u8>)>,
    main: &[u8],
    sym: &[u8],
    sym_path: Option<&Path>,
) {
    let display_path = sym_path.map(|p| p.display().to_string()).unwrap_or_else(|| "<sidecar>".to_owned());
    let main_blid = match read_build_id(main) {
        Some(b) => b,
        None => {
            tracing::warn!(
                "found {display_path} but main zpkg has no BLID section; ignoring sidecar"
            );
            return;
        }
    };
    let sidecar = match parse_zpkg_sidecar(sym) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ignoring corrupt zpkg sidecar {display_path}: {e}");
            return;
        }
    };
    if sidecar.build_id != main_blid {
        tracing::warn!(
            "{display_path} build_id mismatch: main={} sidecar={}; ignored",
            super::build_id::short_hex(&main_blid),
            super::build_id::short_hex(&sidecar.build_id),
        );
        return;
    }

    // Match by namespace: sidecar order matches main MODS order, but we
    // double-check ns equality to be defensive against future MDBG layout
    // changes.
    for ((module, ns, _tidx), (sym_ns, fns)) in module_pairs.iter_mut().zip(sidecar.modules.into_iter()) {
        if ns != &sym_ns {
            tracing::warn!(
                "{display_path}: sidecar module ns mismatch (main={ns}, sidecar={sym_ns}); skipped this module"
            );
            continue;
        }
        if fns.len() != module.functions.len() {
            tracing::warn!(
                "{display_path}: function count mismatch in module '{ns}' (main={}, sidecar={}); skipped",
                module.functions.len(),
                fns.len(),
            );
            continue;
        }
        for (i, fb) in fns.into_iter().enumerate() {
            if !fb.line_table.is_empty() {
                module.functions[i].cold_mut().line_table = fb.line_table.into_boxed_slice();
            }
            if !fb.local_vars.is_empty() {
                module.functions[i].cold_mut().local_vars = fb.local_vars.into_boxed_slice();
            }
        }
    }
}

fn load_zpkg_bytes(raw: &[u8]) -> Result<LoadedArtifact> {
    if raw.len() < 4 || &raw[0..4] != ZPKG_MAGIC {
        bail!("not a binary zpkg payload: expected ZPKG magic bytes");
    }

    let meta = read_zpkg_meta(raw).context("cannot read zpkg metadata")?;
    let module_triples = read_zpkg_modules(raw).context("cannot load modules from zpkg")?;
    assemble_zpkg_artifact(meta.entry, meta.dependencies, module_triples,
        crate::metadata::zbc_reader::read_zpkg_impl_pairs(raw).context("cannot read zpkg IMPL section")?,
        Some(meta.name))
}

// ── Namespace resolution ──────────────────────────────────────────────────────

/// Resolve which files provide a given namespace.
///
/// Multiple zpkgs may legitimately declare the same namespace (C# assembly
/// model). Returns **all** matching files sorted by search tier:
///   1. `module_paths`: scan `.zbc` files (binary, read namespace from header)
///   2. `libs_paths`:   scan `.zpkg` files (binary, read NSPC section)
///
/// If a `.zbc` file in `module_paths` matches, `.zpkg` files in `libs_paths`
/// are **not** scanned (module-path override). This preserves the historical
/// override behaviour without coupling it to single-result semantics.
///
/// Used by compiler tooling and diagnostics. The VM's lazy loader no longer
/// routes by namespace; it uses zpkg file names (`resolve_dependency`).
pub fn resolve_namespace(
    ns: &str,
    module_paths: &[PathBuf],
    libs_paths: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let zbc_matches = find_namespace_in_zbc_dirs(ns, module_paths)?;
    if !zbc_matches.is_empty() {
        return Ok(zbc_matches);
    }
    find_namespace_in_zpkg_dirs(ns, libs_paths)
}

/// Resolve a zpkg dependency by its file name (e.g. `"z42.collections.zpkg"`).
/// Searches `libs_paths` in order and returns the first match. Used by the
/// lazy loader to locate declared dependencies for on-demand load.
pub fn resolve_dependency(
    zpkg_file: &str,
    libs_paths: &[PathBuf],
) -> Result<Option<PathBuf>> {
    for dir in libs_paths {
        let path = dir.join(zpkg_file);
        if path.is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn find_namespace_in_zbc_dirs(ns: &str, dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = Vec::new();
    let backend = fs_backend::active();
    for dir in dirs {
        let dir_str = match dir.to_str() { Some(s) => s, None => continue };
        let names = match backend.read_dir(dir_str) { Ok(e) => e, Err(_) => continue };
        for name in names {
            let path = dir.join(&name);
            if path.extension().and_then(|e| e.to_str()) != Some("zbc") { continue; }
            let path_str = match path.to_str() { Some(s) => s, None => continue };
            let data = match backend.read(path_str) { Ok(d) => d, Err(_) => continue };
            if data.len() < 4 || &data[0..4] != ZBC_MAGIC { continue; }
            let file_ns = match read_zbc_namespace(&data) { Ok(n) => n, Err(_) => continue };
            if file_ns == ns && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

fn find_namespace_in_zpkg_dirs(ns: &str, dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = Vec::new();
    let backend = fs_backend::active();
    for dir in dirs {
        let dir_str = match dir.to_str() { Some(s) => s, None => continue };
        let names = match backend.read_dir(dir_str) { Ok(e) => e, Err(_) => continue };
        for name in names {
            let path = dir.join(&name);
            if path.extension().and_then(|e| e.to_str()) != Some("zpkg") { continue; }
            let path_str = match path.to_str() { Some(s) => s, None => continue };
            let data = match backend.read(path_str) { Ok(d) => d, Err(_) => continue };
            if data.len() < 4 || &data[0..4] != ZPKG_MAGIC { continue; }
            let namespaces = match read_zpkg_namespaces(&data) { Ok(v) => v, Err(_) => continue };
            if namespaces.iter().any(|n| n == ns) && !found.contains(&path) {
                found.push(path);
            }
        }
    }
    Ok(found)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Reads the namespace from a binary zbc buffer (NSPC section fast-path).
pub fn read_zbc_namespace(data: &[u8]) -> Result<String> {
    use super::formats::ZBC_MAGIC;
    if data.len() < 16 { bail!("zbc buffer too short ({} bytes)", data.len()) }
    if &data[0..4] != ZBC_MAGIC { bail!("not a zbc file (bad magic)") }

    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = super::zbc_reader::read_directory_pub(data, sec_count)?;

    match dir.get(b"NSPC") {
        None => Ok(String::new()),
        Some(&(off, size)) => {
            if off + size > data.len() { bail!("NSPC section out of bounds") }
            let sec = &data[off..off + size];
            if sec.len() < 2 { return Ok(String::new()); }
            let len = u16::from_le_bytes([sec[0], sec[1]]) as usize;
            if len == 0 || sec.len() < 2 + len { return Ok(String::new()); }
            Ok(std::str::from_utf8(&sec[2..2 + len])?.to_owned())
        }
    }
}

/// Extract unique namespace prefixes from a module's external calls and static
/// field accesses.
///
/// Namespace = first two dot-separated components of a Call / static.get target
/// not defined locally.
///
/// 2026-04-27 fix-static-field-access: 加上 StaticGet 扫描。修前 user code
/// `Math.PI` 编译为 `static.get @Std.Math.Math.PI`，但 namespace 提取只看 Call/
/// Builtin → 不发现 Std.Math 依赖 → 不 lazy-load z42.math → __static_init__ 不
/// 跑 → 字段永远 null。
fn extract_import_namespaces_from_module(module: &Module) -> Vec<String> {
    use super::bytecode::Instruction;
    let defined: std::collections::HashSet<&str> =
        module.functions.iter().map(|f| f.name.as_str()).collect();
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for func in &module.functions {
        for block in &func.blocks {
            for instr in &block.instructions {
                let target = match instr {
                    Instruction::Call(insn)    if !defined.contains(insn.func.as_str()) => &insn.func,
                    Instruction::Builtin(insn) => &insn.name,
                    Instruction::StaticGet(insn) => &insn.field,
                    Instruction::StaticSet(insn) => &insn.field,
                    // fix-objnew-import-ns (2026-05-29): `new Foo()` on an imported
                    // class emits ObjNew (not Call), and the subsequent method
                    // calls are VCall (vtable) — neither was scanned, so the
                    // providing zpkg's namespace was never inferred and the lazy
                    // loader never declared/loaded it → `VCall: function not
                    // found` on the first method. Use `class_name` so the
                    // constructed type's namespace (e.g. `Std.Collections` from
                    // `Std.Collections.LinkedList`) is registered; loading the
                    // zpkg then makes every method resolvable. Previously masked
                    // when the method happened to compile to a Call (DepIndex
                    // shortcut) instead of a VCall — order-dependent, hence the
                    // cross-platform-flaky failures.
                    Instruction::ObjNew(insn) if !defined.contains(insn.class_name.as_str()) => &insn.class_name,
                    _ => continue,
                };
                for ns in infer_namespace_candidates(target) {
                    if seen.insert(ns.to_owned()) {
                        result.push(ns.to_owned());
                    }
                }
            }
        }
    }
    result
}

/// Extract candidate import-namespace prefixes from a list of fully-qualified
/// call targets. Returns each unique prefix in first-seen order.
pub fn extract_import_namespaces(imports: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for import in imports {
        for ns in infer_namespace_candidates(import) {
            if seen.insert(ns.to_owned()) { result.push(ns.to_owned()); }
        }
    }
    result
}

/// All candidate namespace prefixes of a fully-qualified call target.
///
/// For `Std.IO.Binary.BinaryWriter.WriteByte` returns
/// `["Std", "Std.IO", "Std.IO.Binary", "Std.IO.Binary.BinaryWriter"]` — every
/// `.`-bounded prefix shorter than the full name. The lazy loader feeds these
/// to `resolve_namespace`: only prefixes that match an actual zpkg's declared
/// namespace pull in deps. Returning the full set covers stdlib namespaces of
/// any depth (`Std.IO` vs `Std.IO.Binary`) without the resolver needing to
/// know in advance where the namespace ends and `class.method` begins.
///
/// Names with no dot fall back to the name itself (preserves legacy behaviour
/// for single-segment idents).
fn infer_namespace_candidates(name: &str) -> Vec<&str> {
    let mut result: Vec<&str> = name
        .char_indices()
        .filter_map(|(i, c)| if c == '.' { Some(&name[..i]) } else { None })
        .collect();
    if result.is_empty() { result.push(name); }
    result
}

// ── TypeDesc registry ─────────────────────────────────────────────────────────

/// review.md C3 / Part 5 P3 Phase 1 (2026-06-03, add-string-literal-interning-phase1):
/// pre-allocate per-pool-slot `Arc<str>` cache once, so each subsequent
/// `ConstStr` instruction can clone the existing `Arc` (atomic refcount
/// increment) instead of `String.clone() + .into::<Arc<str>>()` (two
/// heap allocations per call).
///
/// Each loaded zbc / zpkg pays this O(n) cost once, where n is the pool
/// size. Stdlib `"Length"` / `"ToString"` / `"_zeroBytes"` literals are
/// now interned per-module to a single Arc.
pub fn build_interned_strings(module: &mut Module) {
    module.interned_strings = module.string_pool
        .iter()
        .map(|s| Arc::from(s.as_str()))
        .collect();
}

/// Pre-build a `TypeDesc` for every class in `module.classes` and store the
/// results in `module.type_registry` (by-name HashMap) **and**
/// `module.type_registry_vec` (by-`TypeId` Vec, Phase 3 S1, 2026-05-09).
///
/// Algorithm (CoreCLR-inspired):
///   1. Topological sort: each class is processed after its base class.
///   2. Field slots: base fields first (already in base TypeDesc), then derived.
///   3. vtable: start with base vtable, override entries where derived defines
///      the same method name, append new methods at the end.
///   4. Both views populated: by-name HashMap and by-TypeId Vec[id] = Arc.
pub fn build_type_registry(module: &mut Module) {
    let order = topo_sort_classes(module);
    let mut registry: HashMap<String, Arc<TypeDesc>> = HashMap::new();
    let mut registry_vec: Vec<Arc<TypeDesc>> = Vec::with_capacity(order.len());
    // introduce-method-token 2026-05-08: assign TypeId in topo order so that
    // each TypeDesc has a stable per-module id. VCallIC / FieldIC compare
    // receiver TypeId via single u32 equality (no name hash).
    // Phase 3 S1: TypeId.0 is also the index in `registry_vec` (invariant).
    let mut next_type_id: u32 = 0;

    for class_name in &order {
        let desc = match module.classes.iter().find(|c| &c.name == class_name) {
            Some(d) => d,
            None    => continue,
        };

        // ── Own fields (this class's own declarations) ────────────────────
        // fix-cross-pkg-subclass-fields (2026-05-14): preserved separately
        // so the lazy-loader fixup pass can rebuild merged `fields` once
        // the cross-zpkg base resolves.
        let own_fields: Vec<FieldSlot> = desc.fields.iter().map(|f| FieldSlot {
            name: f.name.clone().into(),
            type_tag: f.type_tag.clone().into(),
            visibility: f.visibility,
        }).collect();

        // ── Own methods (this class's own declarations) ───────────────────
        // review.md E5.5 (2026-05-27): store qualified func names only;
        // the simple vtable-slot name is re-derived at merge time via
        // `TypeDesc::derive_simple_method_name`.
        let mut own_methods: Vec<Box<str>> = Vec::new();
        let prefix = format!("{}.", class_name);
        for func in &module.functions {
            if !func.name.starts_with(&prefix) { continue; }
            let method = &func.name[prefix.len()..];
            // Skip constructors (same name as class simple name) and __static_init__
            let simple_name = class_name.split('.').next_back().unwrap_or(class_name.as_str());
            if method == simple_name || method.starts_with("__") { continue; }
            own_methods.push(func.name.clone().into_boxed_str());
        }

        // ── Initial merged view: inherit from local-registry base if present.
        // Cross-zpkg base classes contribute nothing here — that's fixed up
        // later by `try_fixup_inheritance` once the dep is loaded.
        let (fields, field_index, vtable, vtable_index) =
            merge_with_base(&own_fields, &own_methods, class_name, desc.base_class.as_deref(), &registry);

        let type_id = crate::metadata::tokens::TypeId(next_type_id);
        next_type_id += 1;
        // add-field-attribute-reflection: index per-field attr refs (instance +
        // static fields with attributes) by field name for reflection.
        let field_attributes: Box<[(Box<str>, Box<[crate::metadata::bytecode::AttributeRef]>)]> =
            desc.fields.iter()
                .chain(desc.static_fields.iter())
                .filter(|f| !f.attributes.is_empty())
                .map(|f| (f.name.as_str().into(), f.attributes.clone()))
                .collect();
        let cold_inner = crate::metadata::types::TypeDescCold {
            own_fields:             own_fields.into(),
            own_methods:            own_methods.into(),
            type_params:            desc.type_params.clone(),
            type_args:              vec![].into(),
            type_param_constraints: desc.type_param_constraints.clone(),
            // C3 add-attribute-reflection: carry the class's user attributes.
            custom_attributes:      desc.attributes.clone(),
            // add-reflection-static-fields: carry the class's static fields.
            static_fields:          desc.static_fields.clone(),
            // add-field-attribute-reflection: per-field attr refs by name.
            field_attributes,
            // add-reflection-get-interfaces: carry the class's declared interfaces.
            interfaces:             desc.interfaces.iter().map(|s| s.as_str().into()).collect(),
            // add-enum-type-metadata: carry the enum's (name, value) members.
            enum_members:           desc.enum_members.clone(),
            // add-interface-member-reflection: carry the interface's method sigs.
            iface_methods:          desc.iface_methods.clone(),
            // add-struct-value-semantics: carry the value-struct byte+ref layout
            // (from the zbc TYPE-section struct block). `map` → shared `Arc`.
            struct_layout:          desc.struct_layout.as_ref().map(|l| {
                std::sync::Arc::new(crate::metadata::types::StructTypeLayout {
                    size:        l.size as usize,
                    ref_offsets: l.ref_offsets.clone(),
                    ref_kinds:   l.ref_kinds.clone(),
                })
            }),
            // add-struct-heap-inline (P3b): the class's composed inline-struct layout
            // (zbc 1.32 inline block). `map` → shared `Arc`, same as struct_layout.
            inline_layout:          desc.inline_layout.as_ref().map(|l| {
                std::sync::Arc::new(crate::metadata::types::StructTypeLayout {
                    size:        l.size as usize,
                    ref_offsets: l.ref_offsets.clone(),
                    ref_kinds:   l.ref_kinds.clone(),
                })
            }),
        };
        let cold = if cold_inner.own_fields.is_empty()
            && cold_inner.own_methods.is_empty()
            && cold_inner.type_params.is_empty()
            && cold_inner.type_param_constraints.is_empty()
            && cold_inner.custom_attributes.is_empty()
            && cold_inner.static_fields.is_empty()
            && cold_inner.field_attributes.is_empty()
            && cold_inner.interfaces.is_empty()
            && cold_inner.enum_members.is_empty()
            && cold_inner.iface_methods.is_empty()
            // add-struct-heap-inline (P3b): keep cold if it carries an inline layout
            // (an inline-field class always has own_fields too, but guard explicitly).
            && cold_inner.inline_layout.is_none()
        {
            None
        } else {
            Some(Box::new(cold_inner))
        };
        let arc = Arc::new(TypeDesc {
            name: class_name.clone(),
            base_name: desc.base_class.clone(),
            fields,
            field_index,
            vtable,
            vtable_index,
            // add-reflection-type-flags (zbc 1.12): carry the class-shape flags.
            class_flags: desc.class_flags,
            cold,
            id: type_id,
        });
        debug_assert_eq!(
            registry_vec.len() as u32, type_id.0,
            "type_registry_vec invariant: index == TypeId.0"
        );
        registry_vec.push(arc.clone());
        registry.insert(class_name.clone(), arc);
    }

    module.type_registry = registry;
    module.type_registry_vec = registry_vec;
}

// ── fix-cross-pkg-subclass-fields (2026-05-14) ─────────────────────────────
//
// Two-phase type loading: `build_type_registry` runs per-module and only
// resolves base-class inheritance against the local module's registry.
// Cross-zpkg subclasses (subclass in zpkg B, base in zpkg A) get an empty
// inherited slice. `try_fixup_inheritance` runs at lazy-load merge time
// from `LazyLoader::load_zpkg_file` to fill them in, using the global
// type_registry that now contains both A's and B's types.

/// Merge inherited fields/vtable from `base_class_name` (looked up in
/// `registry`) with `own_fields` / `own_methods`. Returns the four fields
/// stored on `TypeDesc`: `(fields, field_index, vtable, vtable_index)`.
///
/// If `base_class_name` is `None`, `own_*` becomes the entire merged view.
/// If `base_class_name` is `Some(b)` but `b` isn't in `registry`, the merge
/// degrades to "own only" (cross-zpkg base unresolved — fixup later).
///
/// review.md E5.5 (2026-05-27): `own_methods` carries qualified func names
/// only; the vtable slot key (simple name) is derived per entry via
/// `TypeDesc::derive_simple_method_name(class_name, fq)`.
fn merge_with_base(
    own_fields:  &[FieldSlot],
    own_methods: &[Box<str>],
    class_name:  &str,
    base_class_name: Option<&str>,
    registry:    &HashMap<String, Arc<TypeDesc>>,
) -> (Vec<FieldSlot>, NameIndex, Vec<(String, String)>, NameIndex) {
    let (mut fields, mut vtable, mut vtable_index) = match base_class_name.and_then(|b| registry.get(b)) {
        Some(base) => (base.fields.clone(), base.vtable.clone(), base.vtable_index.clone()),
        None       => (Vec::new(), Vec::new(), NameIndex::new()),
    };

    // Append own fields skipping name collisions (subclass can't shadow).
    for f in own_fields {
        if !fields.iter().any(|s| s.name == f.name) {
            fields.push(f.clone());
        }
    }
    let field_index: NameIndex = fields.iter().enumerate()
        .map(|(i, f)| (f.name.to_string(), i))
        .collect();

    // Apply own methods: override if base method same simple name, else append.
    for fq_func_name in own_methods {
        let simple_name = TypeDesc::derive_simple_method_name(class_name, fq_func_name);
        if let Some(&slot) = vtable_index.get(simple_name) {
            vtable[slot] = (simple_name.to_string(), fq_func_name.to_string());
        } else {
            let slot = vtable.len();
            vtable_index.insert(simple_name.to_string(), slot);
            vtable.push((simple_name.to_string(), fq_func_name.to_string()));
        }
    }

    (fields, field_index, vtable, vtable_index)
}

/// Walk the global type `registry` and, for any TypeDesc whose base class
/// has become resolvable since its last build, rebuild `fields` /
/// `field_index` / `vtable` / `vtable_index` from the global registry's
/// view of the base.
///
/// Returns the number of types newly fixed up — caller loops until this
/// returns 0 (fixed-point), so multi-level deferred chains converge.
///
/// **Mutation strategy**: types in the global registry are `Arc<TypeDesc>`,
/// but at lazy-load time (well before any instance is created from these
/// types) the registry is the sole strong-Arc holder. We use `Arc::get_mut`
/// to obtain `&mut TypeDesc` and mutate in place. If `get_mut` ever returns
/// `None` (an instance was created before all deps loaded — out-of-order
/// usage that the loader should prevent), we skip the entry and log a
/// warning rather than panic; the entry's `fields` stays as it was.
///
/// **Idempotent**: re-running with the same registry produces the same
/// merged layout; types already correctly fixed up are detected via the
/// `needs_fixup` predicate and skipped.
///
/// **Why one `&mut HashMap` argument (not separate targets + global)**:
/// a "snapshot" clone of the registry would `Arc::clone` every TypeDesc,
/// bumping the strong-count to 2 and breaking `Arc::get_mut` on the
/// mutation side. Instead, we do all reads against the same `registry`
/// in an immutable preprocessing pass, materialise the new layouts into
/// owned `Vec` / `HashMap`s (no shared Arc data), then run a separate
/// mutation pass that only borrows the registry mutably.
pub fn try_fixup_inheritance(
    registry: &mut HashMap<String, Arc<TypeDesc>>,
) -> usize {
    // ── Phase 1: immutable scan — compute new layouts without mutating anything.
    type MergedLayout = (Vec<FieldSlot>, NameIndex, Vec<(String, String)>, NameIndex);
    let mut planned: Vec<(String, MergedLayout)> = Vec::new();
    for (name, td) in registry.iter() {
        if !needs_fixup(td, registry) {
            continue;
        }
        let layout = merge_with_base(
            td.own_fields(),
            td.own_methods(),
            &td.name,
            td.base_name.as_deref(),
            registry,
        );
        planned.push((name.clone(), layout));
    }

    // ── Phase 2: apply mutations.
    let mut newly_fixed = 0;
    for (name, (new_fields, new_field_index, new_vtable, new_vtable_index)) in planned {
        let arc = match registry.get_mut(&name) {
            Some(arc) => arc,
            None      => continue, // unreachable in normal use
        };
        match Arc::get_mut(arc) {
            Some(td) => {
                td.fields       = new_fields;
                td.field_index  = new_field_index;
                td.vtable       = new_vtable;
                td.vtable_index = new_vtable_index;
                newly_fixed += 1;
            }
            None => {
                tracing::warn!(
                    "try_fixup_inheritance: TypeDesc `{}` has additional Arc holders \
                     before fixup completed; cross-zpkg fields may be silently wrong",
                    name
                );
            }
        }
    }
    newly_fixed
}

/// Names of types still reporting `needs_fixup` — used only to make the
/// loader's non-convergence safety-cap error message actionable.
pub fn unconverged_type_names(registry: &HashMap<String, Arc<TypeDesc>>) -> Vec<String> {
    registry.iter()
        .filter(|(_, td)| needs_fixup(td, registry))
        .map(|(name, _)| name.clone())
        .collect()
}

/// True if this TypeDesc's currently-merged view is missing inherited
/// entries that have since become resolvable in `registry`. We compare
/// `fields.len()` against `own_fields.len() + base.fields.len()`; a
/// mismatch means a fixup is needed.
///
/// `own_methods` is allowed to contain multiple entries with the same
/// `simple_name` (arity-overloaded methods like `Foo$1` / `Foo$2` both
/// map to the same vtable slot `Foo`); count *distinct* simple names
/// for the vtable size projection, mirroring [`merge_with_base`].
fn needs_fixup(td: &TypeDesc, registry: &HashMap<String, Arc<TypeDesc>>) -> bool {
    let Some(base_name) = td.base_name.as_deref() else { return false; };
    let Some(base) = registry.get(base_name) else { return false; }; // base still unresolvable
    // Count *distinct* own field names not already in base — mirroring
    // `merge_with_base`, which pushes each own field only if its name isn't
    // already present (dedup against the growing layout). Counting with
    // multiplicity here would permanently disagree with the merged layout for
    // a class carrying duplicate field names (e.g. a copy-pasted declaration),
    // making `needs_fixup` true forever → the caller's fixed-point loop spins.
    let mut seen_f: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let expected_field_count = base.fields.len()
        + td.own_fields().iter()
            .filter(|f| !base.fields.iter().any(|b| b.name == f.name))
            .filter(|f| seen_f.insert(f.name.as_ref()))
            .count();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let own_unique_methods = td.own_methods().iter()
        .map(|fq| TypeDesc::derive_simple_method_name(&td.name, fq))
        .filter(|simple| !base.vtable_index.contains_key(*simple))
        .filter(|simple| seen.insert(*simple))
        .count();
    let expected_vtable_count = base.vtable.len() + own_unique_methods;
    td.fields.len() != expected_field_count || td.vtable.len() != expected_vtable_count
}

// ── L3-G3a: constraint verification pass ───────────────────────────────────

/// Run after `build_type_registry` to validate that every constraint reference
/// (base class or interface) resolves to a known class/interface in the type
/// registry, or (for `Std.*` names) is left to the lazy loader to resolve later.
///
/// Reports the first unresolved reference and aborts load, surfacing the name
/// in the error message so zbc tampering is flagged clearly.
pub fn verify_constraints(module: &Module) -> Result<()> {
    for cls in module.type_registry.values() {
        let tp_names: Vec<&str> = cls.type_params().iter().map(|s| s.as_str()).collect();
        for bundle in cls.type_param_constraints() {
            check_constraint_refs(bundle, &module.type_registry, &cls.name, &tp_names)?;
        }
    }
    for f in &module.functions {
        let tp_names: Vec<&str> = f.type_params().iter().map(|s| s.as_str()).collect();
        for bundle in f.type_param_constraints() {
            check_constraint_refs(bundle, &module.type_registry, &f.name, &tp_names)?;
        }
    }
    Ok(())
}

fn check_constraint_refs(
    b: &crate::metadata::bytecode::ConstraintBundle,
    registry: &std::collections::HashMap<String, Arc<TypeDesc>>,
    owner: &str,
    type_params: &[&str],
) -> Result<()> {
    check_one(b.base_class.as_deref(), registry, owner)?;
    for iface in &b.interfaces {
        check_one(Some(iface), registry, owner)?;
    }
    // add-generic-func-constraint (2026-05-11): validate type-name references in
    // the function signature constraint. Primitives / Std.* / owner's type-params
    // don't need registry lookup; user classes do.
    if let Some(sig) = &b.func_signature {
        for p in &sig.params {
            check_signature_type_ref(p, registry, owner, type_params)?;
        }
        check_signature_type_ref(&sig.ret, registry, owner, type_params)?;
    }
    Ok(())
}

/// Check a type-name reference inside a func-constraint signature.
/// Accepts primitives, "void", arrays (T[]), Std.* names, and type-param names
/// of the owning decl without registry lookup; for other names, falls back to `check_one`.
fn check_signature_type_ref(
    name: &str,
    registry: &std::collections::HashMap<String, Arc<TypeDesc>>,
    owner: &str,
    type_params: &[&str],
) -> Result<()> {
    if matches!(name,
        "void" | "int" | "long" | "short" | "byte" | "sbyte"
        | "uint" | "ulong" | "ushort"
        | "float" | "double" | "bool" | "char" | "string" | "object"
    ) {
        return Ok(());
    }
    // Strip nullable / array / generic decoration before lookup (best-effort);
    // strict structural checking is the C# TypeChecker's job. VM verify is a
    // sanity gate against tampered zbc.
    let base = name
        .trim_end_matches('?')
        .trim_end_matches("[]")
        .split('<').next().unwrap_or(name);
    if type_params.iter().any(|tp| *tp == base) {
        return Ok(());
    }
    check_one(Some(base), registry, owner)
}

fn check_one(
    name: Option<&str>,
    registry: &std::collections::HashMap<String, Arc<TypeDesc>>,
    owner: &str,
) -> Result<()> {
    let Some(n) = name else { return Ok(()); };
    if registry.contains_key(n) { return Ok(()); }
    // Std.* references are resolved by the lazy zpkg loader after module load.
    if n.starts_with("Std.") { return Ok(()); }
    // Interface-only bundles may reference interfaces not yet in the type_registry
    // (which currently holds classes only). Soft-allow; strict interface tracking lands in L3-G3b.
    if n.starts_with('I') && n.chars().nth(1).is_some_and(|c| c.is_ascii_uppercase()) {
        return Ok(());
    }
    bail!("InvalidConstraintReference: `{n}` on `{owner}` not found in type registry")
}

/// Precompute block label → index mapping for all functions in the module.
/// This eliminates the O(n) HashMap construction in every exec_function call.
pub fn build_block_indices(module: &mut Module) {
    use crate::metadata::bytecode::{BranchTargets, Terminator};
    for func in &mut module.functions {
        func.block_index = func.blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.label.clone(), i))
            .collect();
        // perf-vm-iteration: pre-resolve each block's branch terminator labels
        // to block indices so the interp jumps by index (no per-branch SipHash).
        // NoBranch when a label is undefined — runtime then falls back to the
        // label HashMap, preserving the existing "undefined block" error path.
        let idx = &func.block_index;
        func.branch_targets = func.blocks
            .iter()
            .map(|b| match &b.terminator {
                Terminator::Br { label } => idx
                    .get(label.as_str())
                    .map(|&t| BranchTargets::Br(t))
                    .unwrap_or(BranchTargets::NoBranch),
                Terminator::BrCond { true_label, false_label, .. } => {
                    match (idx.get(true_label.as_str()), idx.get(false_label.as_str())) {
                        (Some(&t), Some(&f)) => BranchTargets::BrCond(t, f),
                        _ => BranchTargets::NoBranch,
                    }
                }
                _ => BranchTargets::NoBranch,
            })
            .collect();
        // interp-superinstr-fusion: recognize fused block tails once, parallel to
        // branch_targets (needs the index-resolved targets above).
        func.fused_tails = super::superinstr::compute_fused_tails(&func.blocks, &func.branch_targets, &func.reg_types);
        // perf-frame-name-precompute: build the stack-frame (name, file) Arc<str>
        // pair once here so `exec_function_body` clones it O(1) per call instead
        // of re-formatting + allocating on every call (40–60% of call-heavy
        // interp time). `format_frame_name` needs `param_types` (in the cold box)
        // + the display name; the file comes from the line table's first entry.
        let file: std::sync::Arc<str> = func.line_table().first()
            .and_then(|e| e.file.clone())
            .map(std::sync::Arc::from)
            .unwrap_or_else(|| std::sync::Arc::from(""));
        let name = std::sync::Arc::from(crate::metadata::bytecode::format_frame_name(func));
        func.frame_meta = Some((name, file));
    }
}

/// Precompute function name → index mapping for O(1) call dispatch.
pub fn build_func_index(module: &mut Module) {
    module.func_index = module.functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
}

/// Return class names in topological order (base before derived).
fn topo_sort_classes(module: &Module) -> Vec<String> {
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut order: Vec<String> = Vec::new();

    fn visit(
        name: &str,
        module: &Module,
        visited: &mut std::collections::HashSet<String>,
        order: &mut Vec<String>,
    ) {
        if visited.contains(name) { return; }
        visited.insert(name.to_string());
        if let Some(desc) = module.classes.iter().find(|c| c.name == name) {
            if let Some(base) = &desc.base_class {
                visit(base, module, visited, order);
            }
        }
        order.push(name.to_string());
    }

    for cls in &module.classes {
        visit(&cls.name, module, &mut visited, &mut order);
    }
    order
}

#[cfg(test)]
#[path = "loader_tests.rs"]
mod tests;
