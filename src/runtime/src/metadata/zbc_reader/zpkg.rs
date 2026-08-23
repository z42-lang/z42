use super::*;

// ── zpkg public API ───────────────────────────────────────────────────────────

pub struct ZpkgInfo {
    pub name:         String,
    pub version:      String,
    pub entry:        Option<String>,
    pub namespaces:   Vec<String>,
    pub dependencies: Vec<ZpkgDep>,
    pub is_packed:    bool,
    pub is_exe:       bool,
}

pub fn read_zpkg_meta(data: &[u8]) -> Result<ZpkgInfo> {
    // Strict-pin check (freeze-zpkg-v0): even the lightweight meta-only reader
    // must reject mismatched versions — otherwise tooling could surface a stale
    // META and the user has no clear signal to regen.
    verify_zpkg_version(data)?;
    let flags     = u16::from_le_bytes([data[8], data[9]]);
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let is_packed = flags & 0x01 != 0;
    let is_exe    = flags & 0x02 != 0;

    let dir = read_directory(data, sec_count)?;

    // META: name, version, entry (inline UTF-8, no pool dependency)
    let (name, version, entry) = get_section(data, &dir, b"META")
        .map(|s| read_meta_section(s))
        .transpose()?
        .unwrap_or_else(|| (String::new(), String::new(), None));

    // STRS pool for NSPC + DEPS
    let pool = get_section(data, &dir, b"STRS")
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();

    // NSPC: list of namespace indices → strings
    let namespaces = get_section(data, &dir, b"NSPC")
        .map(|s| read_nspc_list(s, &pool))
        .transpose()?
        .unwrap_or_default();

    // DEPS
    let dependencies = get_section(data, &dir, b"DEPS")
        .map(|s| read_deps_section(s, &pool))
        .transpose()?
        .unwrap_or_default();

    Ok(ZpkgInfo { name, version, entry, namespaces, dependencies, is_packed, is_exe })
}

/// Read all modules from a packed zpkg. Returns (Module, namespace) pairs.
/// Decode every inner module from a binary zpkg, returning
/// `(Module, namespace, raw_tidx_bytes)` per module. `raw_tidx_bytes`
/// is the verbatim TIDX section payload for that module (empty when
/// the module has no [Test] / [Benchmark]). aggregate-zpkg-tidx
/// (zpkg 0.11, 2026-06-06): the third element is new — callers that
/// don't care about test metadata can ignore it.
pub fn read_zpkg_modules(data: &[u8]) -> Result<Vec<(Module, String, Vec<u8>)>> {
    verify_zpkg_version(data)?;
    let flags     = u16::from_le_bytes([data[8], data[9]]);
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let is_packed = flags & 0x01 != 0;
    if (flags & 0x04) != 0 {
        bail!(
            "zpkg has SymOnly flag set: it is a debug-symbol sidecar (.zsym), \
             not a loadable package"
        );
    }
    let has_is_static = true; // zpkg 0.2+ always has is_static in SIGS

    let dir = read_directory(data, sec_count)?;

    let pool = get_section(data, &dir, b"STRS")
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();

    if is_packed {
        // SIGS: global function signatures
        let sigs = get_section(data, &dir, b"SIGS")
            .map(|s| read_sigs(s, &pool, has_is_static))
            .transpose()?
            .unwrap_or_default();

        // MODS: per-module FUNC+TYPE bodies
        let mods_sec = get_section(data, &dir, b"MODS")
            .ok_or_else(|| anyhow::anyhow!("packed zpkg missing MODS section"))?;
        // Phase 3 S3c: zpkg 0.2+ exclusive → inner modules always v1.0.
        read_mods_section(mods_sec, &pool, &sigs)
    } else {
        // Indexed mode needs the main file's directory to read scattered zbc —
        // byte-only callers (embedding API) can't resolve them. The path-aware
        // loader (loader::load_zpkg) handles indexed via read_zpkg_file_entries.
        bail!(
            "indexed zpkg cannot be loaded from bytes alone (scattered .zbc need \
             the package directory); load it by path"
        )
    }
}

// ── zpkg section decoders ─────────────────────────────────────────────────────

pub(super) fn read_meta_section(sec: &[u8]) -> Result<(String, String, Option<String>)> {
    let mut c = Cursor::new(sec);
    let name    = c.read_utf8_u16len()?;
    let version = c.read_utf8_u16len()?;
    let entry_s = c.read_utf8_u16len()?;
    let entry   = if entry_s.is_empty() { None } else { Some(entry_s) };
    Ok((name, version, entry))
}

pub(super) fn read_nspc_list(sec: &[u8], pool: &[String]) -> Result<Vec<String>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut ns = Vec::with_capacity(count);
    for _ in 0..count {
        let idx = c.read_u32()?;
        ns.push(pool_str_owned(pool, idx)?);
    }
    Ok(ns)
}

pub(super) fn read_deps_section(sec: &[u8], pool: &[String]) -> Result<Vec<ZpkgDep>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut deps = Vec::with_capacity(count);
    for _ in 0..count {
        let file_idx = c.read_u32()?;
        let ns_count = c.read_u16()? as usize;
        let mut namespaces = Vec::with_capacity(ns_count);
        for _ in 0..ns_count {
            namespaces.push(pool_str_owned(pool, c.read_u32()?)?);
        }
        deps.push(ZpkgDep {
            file: pool_str_owned(pool, file_idx)?,
            namespaces,
        });
    }
    Ok(deps)
}

/// Decode the MODS section of a packed zpkg into one entry per inner
/// module. Returns the per-module `(Module, namespace, raw_tidx_bytes)`
/// triple — `raw_tidx_bytes` is empty when the module has no TIDX
/// annotation, otherwise carries the verbatim TIDX section payload (NOT
/// including the `tidx_len u32` framing). aggregate-zpkg-tidx
/// (2026-06-06) introduced the third element; the caller in
/// `loader::load_zpkg_bytes` decodes + remaps each module's entries
/// against the cumulative function + string-pool offsets so the merged
/// `LoadedArtifact.test_index` resolves through the unified index space.
pub(super) fn read_mods_section(
    sec: &[u8],
    pool: &[String],
    global_sigs: &[FuncSig],
) -> Result<Vec<(Module, String, Vec<u8>)>> {
    let mut c = Cursor::new(sec);
    let mod_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(mod_count);

    let mut sig_offset = 0usize;
    for _ in 0..mod_count {
        let ns_idx      = c.read_u32()?;
        let _src_idx    = c.read_u32()?;
        let _hash_idx   = c.read_u32()?;
        let func_count  = c.read_u16()? as usize;
        let first_sig   = c.read_u32()? as usize;
        let func_len    = c.read_u32()? as usize;
        let func_data   = c.read_bytes(func_len)?;
        let type_len    = c.read_u32()? as usize;
        let type_data   = c.read_bytes(type_len)?;
        // 1.2 split-debug-symbols: per-member DBUG body. 0 bytes = no debug.
        let dbug_len    = c.read_u32()? as usize;
        let dbug_data   = c.read_bytes(dbug_len)?;
        // jit-type-specialization C2 P0 (zpkg 0.9 / zbc 1.8, 2026-05-27):
        // per-member REGT body. 0 bytes = no typed regs (legacy zbc / mod
        // with no IrType-tagged functions).
        let regt_len    = c.read_u32()? as usize;
        let regt_data   = c.read_bytes(regt_len)?;
        // aggregate-zpkg-tidx (zpkg 0.11, 2026-06-06): per-member TIDX
        // body, length-prefixed like DBUG / REGT. 0 bytes = module has
        // no [Test] / [Benchmark] annotations — caller skips
        // `read_test_index`. Stored verbatim; caller-side aggregation
        // walks each module's entries with cumulative function + string
        // offsets to map module-local `method_id` / `*_str_idx` values
        // into the merged-module index space.
        let tidx_len    = c.read_u32()? as usize;
        let tidx_bytes  = c.read_bytes(tidx_len)?.to_vec();

        let namespace = pool_str_owned(pool, ns_idx)?;
        let sigs_slice = &global_sigs[first_sig..first_sig + func_count.min(global_sigs.len() - first_sig.min(global_sigs.len()))];

        let classes = if type_len > 0 { read_type(type_data, pool)? } else { vec![] };
        // Phase 3 S3c: zpkg 0.2+ inner modules are v1.0; IdMap always v1.0.
        let id_map = IdMap::for_v1(
            pool,
            sigs_slice.iter().map(|s| s.name.clone()).collect(),
            classes.iter().map(|c| c.name.clone()).collect(),
        );
        let func_bodies = read_func(func_data, pool, &id_map)?;
        let mut dbug_entries = if dbug_len > 0 {
            read_dbug(dbug_data, pool)?
        } else {
            Vec::new()
        };
        let mut regt_entries = if regt_len > 0 {
            read_regt(regt_data)?
        } else {
            Vec::new()
        };

        let mut functions: Vec<Function> = func_bodies.into_iter().enumerate().map(|(i, body)| {
            let sig = sigs_slice.get(i);
            let dbug = if i < dbug_entries.len() {
                std::mem::take(&mut dbug_entries[i])
            } else {
                DbugFuncEntry::default()
            };
            let reg_types = if i < regt_entries.len() {
                std::mem::take(&mut regt_entries[i])
            } else {
                Box::new([])
            };
            let cold_inner = crate::metadata::bytecode::FunctionCold {
                param_types:            sig.map(|s| s.param_types.clone()).unwrap_or_default().into_boxed_slice(),
                exception_table:        body.exception_table.into_boxed_slice(),
                line_table:             dbug.line_table.into_boxed_slice(),
                local_vars:             dbug.local_vars.into_boxed_slice(),
                type_params:            sig.map(|s| s.type_params.clone()).unwrap_or_default().into_boxed_slice(),
                type_param_constraints: sig.map(|s| s.type_param_constraints.clone()).unwrap_or_default().into_boxed_slice(),
                custom_attributes:      sig.map(|s| s.custom_attributes.clone()).unwrap_or_default().into_boxed_slice(),
                param_attributes:       sig.map(|s| s.param_attributes.clone()).unwrap_or_default().into_boxed_slice(),
                param_names:            sig.map(|s| s.param_names.clone()).unwrap_or_default().into_boxed_slice(),
                param_defaults:         sig.map(|s| s.param_defaults.clone()).unwrap_or_default().into_boxed_slice(),
            };
            let cold = if cold_inner.param_types.is_empty()
                && cold_inner.exception_table.is_empty()
                && cold_inner.line_table.is_empty()
                && cold_inner.local_vars.is_empty()
                && cold_inner.type_params.is_empty()
                && cold_inner.type_param_constraints.is_empty()
                && cold_inner.custom_attributes.is_empty()
                && cold_inner.param_attributes.iter().all(|p| p.is_empty())
                && cold_inner.param_names.is_empty()
                && cold_inner.param_defaults.is_empty()
            {
                None
            } else {
                Some(Box::new(cold_inner))
            };
            Function {
                name:            sig.map(|s| s.name.clone()).unwrap_or_else(|| format!("func#{i}")),
                param_count:     sig.map(|s| s.param_count).unwrap_or(0),
                ret_type:        sig.map(|s| s.ret_type.clone()).unwrap_or_else(|| "void".to_owned()),
                exec_mode:       sig.map(|s| s.exec_mode).unwrap_or(ExecMode::Interp),
                blocks:          body.blocks,
                is_static:       sig.map(|s| s.is_static).unwrap_or(false),
                visibility:      sig.map(|s| s.visibility).unwrap_or(0),
                method_flags:    sig.map(|s| s.method_flags).unwrap_or(0),
                min_arg:         sig.map(|s| s.min_arg).unwrap_or(0),
                params_from:     sig.map(|s| s.params_from).unwrap_or(0xFF),
                max_reg:         0,
                cold,
                reg_types,
                block_index:     std::collections::HashMap::new(),
            branch_targets:  Vec::new(),
            fused_tails:     Vec::new(),
            frame_meta:     None,
                resolved:        std::sync::OnceLock::new(),
            }
        }).collect();

        let name = if namespace.is_empty() { "unknown".to_owned() } else { namespace.clone() };
        let string_pool = rebuild_string_pool(pool, &mut functions);
        // 2026-05-02 D1b: zpkg packed-mode 暂不支持 method group cache slot
        // metadata（FRCS section 是 module-level；packed zpkg 的多 module 模式
        // 需要后续扩展 MODS 携带 per-module slot count）。当前 fallback 为 0；
        // zpkg 内的 LoadFnCached 命中会触发 OOB → bail（运行时报错）。视
        // packed zpkg 是否实际命中 LoadFnCached 决定 follow-up。
        result.push((Module {
            name, string_pool, classes, functions,
            type_registry: rustc_hash::FxHashMap::default(),
            type_registry_vec: Vec::new(),
            func_index: rustc_hash::FxHashMap::default(),
            func_ref_cache_slots: 0,
            // Populated inside `merge_modules` (these per-namespace modules
            // are always merged before consumption).
        }, namespace, tidx_bytes));

        sig_offset += func_count;
        let _ = sig_offset; // used for validation if needed
    }
    Ok(result)
}

// ── Zpkg namespace fast-scan ──────────────────────────────────────────────────

/// Read only the namespaces from a binary zpkg (fast path for dependency scanning).
pub fn read_zpkg_namespaces(data: &[u8]) -> Result<Vec<String>> {
    if data.len() < 16 { bail!("zpkg file too short") }
    if &data[0..4] != ZPKG_MAGIC { bail!("not a binary zpkg (bad magic)") }

    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count)?;

    let pool = get_section(data, &dir, b"STRS")
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();

    get_section(data, &dir, b"NSPC")
        .map(|s| read_nspc_list(s, &pool))
        .transpose()
        .map(|v| v.unwrap_or_default())
}


pub(super) fn exec_mode_from_byte(b: u8) -> ExecMode {
    match b {
        1 => ExecMode::Jit,
        2 => ExecMode::Aot,
        _ => ExecMode::Interp,
    }
}
