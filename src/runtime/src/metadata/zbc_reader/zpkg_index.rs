use super::*;

/// Spec R1 — read the optional TIDX section from a zbc file. Returns an empty
/// vec if the section is absent (older zbc / module without test methods).
///
/// **The returned `TestEntry` rows have only `*_str_idx` populated**;
/// `*_resolved` `Option<String>` fields are `None`. Use
/// [`read_test_index_resolved`] for the loader path that wants resolved
/// strings, or call
/// [`crate::metadata::test_index::resolve_test_index_strings`] manually with
/// a raw STRS pool obtained via [`read_raw_string_pool`].
pub fn read_test_index_section(data: &[u8]) -> Result<Vec<crate::metadata::TestEntry>> {
    if data.len() < 16 { bail!("zbc file too short") }
    if &data[0..4] != ZBC_MAGIC { bail!("not a binary zbc (bad magic)") }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count)?;
    Ok(get_section(data, &dir, b"TIDX")
        .map(crate::metadata::test_index::read_test_index)
        .transpose()?
        .unwrap_or_default())
}

/// Read the **raw** STRS string pool (pre-rebuild). The TIDX `*_str_idx` fields
/// reference indices in this pool; `read_zbc` discards strings only referenced
/// by TIDX during `rebuild_string_pool`, so loader code must read the raw pool
/// **before** running through `read_zbc` if it wants to resolve TIDX strings.
pub fn read_raw_string_pool(data: &[u8]) -> Result<Vec<String>> {
    if data.len() < 16 { bail!("zbc file too short") }
    if &data[0..4] != ZBC_MAGIC { bail!("not a binary zbc (bad magic)") }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count)?;
    Ok(get_section(data, &dir, b"STRS")
        .or_else(|| get_section(data, &dir, b"BSTR"))
        .map(read_strs)
        .transpose()?
        .unwrap_or_default())
}

/// Convenience: read the TIDX section AND resolve all `*_str_idx` fields to
/// `Option<String>` using the raw STRS pool from the same `data`. This is the
/// API used by [`loader::load_artifact`].
pub fn read_test_index_resolved(data: &[u8]) -> Result<Vec<crate::metadata::TestEntry>> {
    let mut entries = read_test_index_section(data)?;
    if !entries.is_empty() {
        let pool = read_raw_string_pool(data)?;
        crate::metadata::test_index::resolve_test_index_strings(&mut entries, &pool);
    }
    Ok(entries)
}

/// Read zpkg header metadata (fast path, no module decode).
/// add-crosspkg-impl-reflection (unify P1-e): parse the zpkg IMPL section into
/// `(target_fq, trait_fq)` pairs for the runtime impls registry (backs
/// `Type.GetInterfaces()` seeing cross-package `impl Trait for Type`).
///
/// Reads dir + STRS + IMPL independently from the raw zpkg bytes (packed and
/// indexed both carry IMPL as a top-level section) — deliberately does NOT
/// reshape `read_zpkg_modules`. Type args and per-impl method signatures are
/// skipped: dispatch already works via vtable/func_index; reflection only
/// needs the type↔trait association. Method record layout mirrors z42c
/// `ZpkgWriterZ._writeMethod`: name(4)+ret(4)+vis(4)+flags(1)+min_arg(2)
/// +param_count(1)+params_from(1) = 17 bytes, then param_count×(4+4).
/// Returns empty for missing/empty IMPL. Version checking is the caller's
/// concern (load paths already strict-pin before calling).
pub fn read_zpkg_impl_pairs(data: &[u8]) -> Result<Vec<(String, String)>> {
    if data.len() < 16 { return Ok(Vec::new()) }
    if &data[0..4] != ZPKG_MAGIC { return Ok(Vec::new()) }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count)?;
    let impl_sec = match get_section(data, &dir, b"IMPL") {
        Some(s) if !s.is_empty() => s,
        _ => return Ok(Vec::new()),
    };
    let pool = get_section(data, &dir, b"STRS")
        .map(read_strs)
        .transpose()?
        .unwrap_or_default();
    let mut c = Cursor::new(impl_sec);
    let mut pairs = Vec::new();
    let mod_count = c.read_u16()?;
    for _ in 0..mod_count {
        let _ns = c.read_u32()?;
        let impl_count = c.read_u16()?;
        for _ in 0..impl_count {
            let target_idx = c.read_u32()?;
            let trait_idx = c.read_u32()?;
            let tac = c.read_u8()? as usize;
            for _ in 0..tac { c.read_u32()?; }
            let method_count = c.read_u16()?;
            for _ in 0..method_count {
                c.read_u32()?; c.read_u32()?; c.read_u32()?;   // name / ret / visibility
                c.read_u8()?;                                   // flags
                c.read_u16()?;                                  // min_arg
                let pc = c.read_u8()? as usize;                 // param_count
                c.read_u8()?;                                   // params_from
                for _ in 0..pc { c.read_u32()?; c.read_u32()?; }
            }
            pairs.push((
                c.pool_str(&pool, target_idx)?.to_owned(),
                c.pool_str(&pool, trait_idx)?.to_owned(),
            ));
        }
    }
    Ok(pairs)
}

/// One FILE-section entry of an indexed zpkg (add-indexed-zpkg-min-patch,
/// zpkg 0.24): source rel path + namespace + content hash of the scattered
/// self-contained fullMode `<stem>.zbc` (BLAKE3-128 hex). `fn_count` /
/// `first_sig` mirror the MODS header so SIGS pairing stays isomorphic.
pub struct ZpkgFileEntry {
    pub rel: String,
    pub src_hash: String,
    pub namespace: String,
    pub fn_count: u16,
    pub first_sig: u32,
    pub zbc_hash: String,
}

/// Read the FILE directory of an indexed zpkg main file. Bails when the
/// package is packed (no FILE section), on strict-pin mismatch, or on a
/// SymOnly sidecar.
pub fn read_zpkg_file_entries(data: &[u8]) -> Result<Vec<ZpkgFileEntry>> {
    verify_zpkg_version(data)?;
    let flags     = u16::from_le_bytes([data[8], data[9]]);
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    if (flags & 0x04) != 0 {
        bail!("zpkg has SymOnly flag set: it is a debug-symbol sidecar (.zsym)");
    }
    if (flags & 0x01) != 0 {
        bail!("packed zpkg has no FILE directory (indexed-only section)");
    }
    let dir = read_directory(data, sec_count)?;
    let pool = get_section(data, &dir, b"STRS")
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();
    let sec = get_section(data, &dir, b"FILE")
        .ok_or_else(|| anyhow::anyhow!("indexed zpkg missing FILE section"))?;
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let ns_idx   = c.read_u32()?;
        let src_idx  = c.read_u32()?;
        let hash_idx = c.read_u32()?;
        let fn_count = c.read_u16()?;
        let first_sig = c.read_u32()?;
        let zbc_idx  = c.read_u32()?;
        entries.push(ZpkgFileEntry {
            rel:       pool_str_owned(&pool, src_idx)?,
            src_hash:  pool_str_owned(&pool, hash_idx)?,
            namespace: pool_str_owned(&pool, ns_idx)?,
            fn_count,
            first_sig,
            zbc_hash:  pool_str_owned(&pool, zbc_idx)?,
        });
    }
    Ok(entries)
}
