use super::*;

// ── Sidecar API (1.2 / 0.3 split-debug-symbols) ──────────────────────────────

/// Decoded contents of a `.zbc` sidecar (zbc with `ZbcFlags::SymOnly` set).
/// Mirrors C# `ZbcReader.SidecarData`.
#[derive(Debug)]
pub struct ZbcSidecarData {
    pub build_id: [u8; crate::metadata::build_id::SIZE],
    pub functions: Vec<DbugFuncEntry>,
}

/// Decoded contents of a `.zpkg` sidecar (zpkg with `FlagSymOnly` set).
/// Mirrors C# `ZpkgReader.ZpkgSidecarData`.
#[derive(Debug)]
pub struct ZpkgSidecarData {
    pub build_id: [u8; crate::metadata::build_id::SIZE],
    /// (namespace, per-function debug). Order matches main zpkg's MODS section.
    pub modules: Vec<(String, Vec<DbugFuncEntry>)>,
}

/// Reads only the BLID section (16-byte build_id) from any zbc or zpkg.
/// Returns None when the file has no BLID section (e.g. non-strip build).
pub fn read_build_id(data: &[u8]) -> Option<[u8; crate::metadata::build_id::SIZE]> {
    if data.len() < 16 { return None; }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count).ok()?;
    let sec = get_section(data, &dir, b"BLID")?;
    if sec.len() < crate::metadata::build_id::SIZE { return None; }
    let mut out = [0u8; crate::metadata::build_id::SIZE];
    out.copy_from_slice(&sec[..crate::metadata::build_id::SIZE]);
    Some(out)
}

/// Parses a `.zbc` sidecar (SymOnly zbc): NSPC + STRS + DBUG + BLID.
/// Returns Err when the file lacks the SymOnly flag, has no BLID, or content
/// is malformed. Caller is responsible for build_id pairing with main zbc.
pub fn parse_zbc_sidecar(data: &[u8]) -> Result<ZbcSidecarData> {
    if data.len() < 16 || &data[0..4] != ZBC_MAGIC {
        bail!("not a zbc sidecar (bad magic)");
    }
    let major = u16::from_le_bytes([data[4], data[5]]);
    let minor = u16::from_le_bytes([data[6], data[7]]);
    if major != ZBC_VERSION_MAJOR || minor != ZBC_VERSION_MINOR {
        bail!(
            "zbc sidecar {major}.{minor} not supported (writer is at \
             {ZBC_VERSION_MAJOR}.{ZBC_VERSION_MINOR}); \
             regen via xtask regen"
        );
    }
    let flags = u16::from_le_bytes([data[8], data[9]]);
    if (flags & 0x04) == 0 {
        bail!("expected SymOnly flag set; this is not a debug-symbol sidecar");
    }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count)?;

    let blid = get_section(data, &dir, b"BLID")
        .ok_or_else(|| anyhow::anyhow!("zbc sidecar missing BLID section"))?;
    if blid.len() < crate::metadata::build_id::SIZE {
        bail!("zbc sidecar BLID section too short");
    }
    let mut build_id = [0u8; crate::metadata::build_id::SIZE];
    build_id.copy_from_slice(&blid[..crate::metadata::build_id::SIZE]);

    let pool = get_section(data, &dir, b"STRS")
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();

    let functions = get_section(data, &dir, b"DBUG")
        .map(|s| read_dbug(s, &pool))
        .transpose()?
        .unwrap_or_default();

    Ok(ZbcSidecarData { build_id, functions })
}

/// Parses a `.zpkg` sidecar (SymOnly zpkg): META + STRS + MDBG + BLID.
pub fn parse_zpkg_sidecar(data: &[u8]) -> Result<ZpkgSidecarData> {
    if data.len() < 16 || &data[0..4] != ZPKG_MAGIC {
        bail!("not a zpkg sidecar (bad magic)");
    }
    let major = u16::from_le_bytes([data[4], data[5]]);
    let minor = u16::from_le_bytes([data[6], data[7]]);
    if major != ZPKG_VERSION_MAJOR || minor != ZPKG_VERSION_MINOR {
        bail!(
            "zpkg sidecar {major}.{minor} not supported (writer is at \
             {ZPKG_VERSION_MAJOR}.{ZPKG_VERSION_MINOR}); \
             regen via xtask build stdlib"
        );
    }
    let flags = u16::from_le_bytes([data[8], data[9]]);
    if (flags & 0x04) == 0 {
        bail!("expected SymOnly flag set; this is not a debug-symbol sidecar");
    }
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let dir = read_directory(data, sec_count)?;

    let blid = get_section(data, &dir, b"BLID")
        .ok_or_else(|| anyhow::anyhow!("zpkg sidecar missing BLID section"))?;
    if blid.len() < crate::metadata::build_id::SIZE {
        bail!("zpkg sidecar BLID section too short");
    }
    let mut build_id = [0u8; crate::metadata::build_id::SIZE];
    build_id.copy_from_slice(&blid[..crate::metadata::build_id::SIZE]);

    let pool = get_section(data, &dir, b"STRS")
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();

    let mdbg = get_section(data, &dir, b"MDBG")
        .ok_or_else(|| anyhow::anyhow!("zpkg sidecar missing MDBG section"))?;
    let modules = read_mdbg_section(mdbg, &pool)?;

    Ok(ZpkgSidecarData { build_id, modules })
}

pub(super) fn read_mdbg_section(sec: &[u8], pool: &[String]) -> Result<Vec<(String, Vec<DbugFuncEntry>)>> {
    let mut c = Cursor::new(sec);
    let mod_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(mod_count);
    for _ in 0..mod_count {
        let ns_idx   = c.read_u32()?;
        // add-offline-symbolication (zpkg 0.34): per-func frame-name key array
        // precedes the dbug blob. The runtime merges sidecar line tables into the
        // module BY INDEX (names come from the main zpkg), so it only needs to
        // SKIP the frame-name idxs to reach the dbug body. `z42d symbolicate`
        // (offline, z42 side) is the consumer that actually reads the keys.
        let func_count = c.read_u32()? as usize;
        for _ in 0..func_count { let _frame_name_idx = c.read_u32()?; }
        let dbug_len = c.read_u32()? as usize;
        let dbug     = c.read_bytes(dbug_len)?;
        let ns = pool_str_owned(pool, ns_idx)?;
        let entries = if dbug.is_empty() { Vec::new() } else { read_dbug(dbug, pool)? };
        result.push((ns, entries));
    }
    Ok(result)
}
