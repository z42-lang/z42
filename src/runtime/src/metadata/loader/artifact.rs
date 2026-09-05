use super::*;

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
            crate::metadata::build_id::short_hex(&main_blid),
            crate::metadata::build_id::short_hex(&sidecar.build_id),
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
        // plain BLAKE3-128（区别于 BLID：那个是"尾 16B 清零"后的写入端标签、runtime
        // 只比相等不重算；散装 zbc 是内容原样哈希，这里**真的重算校验** ⇒ 它才是跨
        // 语言契约，算法不可单方面改）。
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
    crate::metadata::test_index::resolve_test_index_strings(&mut test_index, &module.string_pool);

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
    module_triples: &[(crate::metadata::bytecode::Module, String, Vec<u8>)],
) -> Result<Vec<crate::metadata::test_index::TestEntry>> {
    let mut aggregated = Vec::new();
    let mut cumulative_func: u32 = 0;
    let mut cumulative_str:  u32 = 0;
    for (module, _ns, tidx_bytes) in module_triples {
        let func_offset = cumulative_func;
        let str_offset  = cumulative_str;
        cumulative_func = cumulative_func.saturating_add(module.functions.len() as u32);
        cumulative_str  = cumulative_str.saturating_add(module.string_pool.len() as u32);
        if tidx_bytes.is_empty() { continue; }
        let mut entries = crate::metadata::test_index::read_test_index(tidx_bytes)
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
            crate::metadata::build_id::short_hex(&main_blid),
            crate::metadata::build_id::short_hex(&sidecar.build_id),
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
