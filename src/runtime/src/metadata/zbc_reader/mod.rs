/// Binary zbc v0.3 and zpkg v0.1 reader for the Rust VM.
///
/// Mirrors the C# ZbcWriter/ZpkgWriter layout exactly.
///
/// zbc layout:
///   Header (16): magic[4] + major[2] + minor[2] + flags[2] + sec_count[2] + reserved[4]
///   Directory (sec_count × 12): tag[4] + offset[4] + size[4]
///   Sections at absolute offsets.
///
/// zpkg layout: same header/directory structure, different section tags.
use std::collections::HashMap;

use anyhow::{bail, Result};

use super::bytecode::{
    BasicBlock, ClassDesc, ConstraintBundle, ExceptionEntry, FieldDesc, Function, Instruction, Module, Terminator,
};
use super::bytecode::{
    AsCastInsn, BuiltinInsn, CallInsn, CallNativeInsn, FieldGetInsn, FieldSetInsn, IsInstanceInsn,
    LoadFieldAddrInsn, LoadFnCachedInsn, LoadFnInsn, MkClosInsn, ObjNewInsn, StaticGetInsn,
    StaticSetInsn, StructAllocInsn, TypeofInsn, VCallInsn,
};
use super::formats::{ZpkgDep, ZPKG_MAGIC, ZBC_MAGIC};
use super::types::ExecMode;

// ── Submodules (refactor-zbc-reader-split): 2344-line god-file → cohesive units ──
mod versions;
mod opcodes;
mod cursor;
mod type_reader;
mod func_reader;
mod instr_decode;
mod sidecar;
mod zpkg;
mod zpkg_index;

// Internal hub: private re-glob of every submodule so `read_zbc` (here) and each
// sibling's own `use super::*` resolve all reader helpers uniformly.
use versions::*;
use opcodes::*;
use cursor::*;
use type_reader::*;
use func_reader::*;
use instr_decode::*;
use zpkg::*;

// Public API preserved at `metadata::zbc_reader::*` for in-crate consumers.
pub use versions::{ZBC_VERSION_MAJOR, ZBC_VERSION_MINOR, ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR};
pub use func_reader::DbugFuncEntry;
pub use sidecar::{ZbcSidecarData, ZpkgSidecarData, read_build_id, parse_zbc_sidecar, parse_zpkg_sidecar};
pub use zpkg::{ZpkgInfo, read_zpkg_meta, read_zpkg_modules, read_zpkg_namespaces};
pub use zpkg_index::{
    ZpkgFileEntry, read_zpkg_impl_pairs, read_zpkg_file_entries,
    read_test_index_section, read_test_index_resolved, read_raw_string_pool,
};

// ── Section directory ─────────────────────────────────────────────────────────

/// Public re-export for loader.rs (namespace fast-scan).
pub fn read_directory_pub(data: &[u8], sec_count: u16) -> Result<HashMap<[u8;4], (usize, usize)>> {
    read_directory(data, sec_count)
}

fn read_directory(data: &[u8], sec_count: u16) -> Result<HashMap<[u8;4], (usize, usize)>> {
    let mut dir = HashMap::new();
    if sec_count == 0 {
        // Legacy v0.2 sequential scan (no directory): header is 16 bytes,
        // each section: tag[4] + len[4] + data[len]
        let mut pos = 16usize;
        while pos + 8 <= data.len() {
            let tag: [u8;4] = data[pos..pos+4].try_into().unwrap();
            let len = u32::from_le_bytes(data[pos+4..pos+8].try_into().unwrap()) as usize;
            dir.insert(tag, (pos + 8, len));
            pos += 8 + len;
        }
    } else {
        // v0.3 directory: starts at byte 16
        let mut pos = 16usize;
        for _ in 0..sec_count {
            if pos + 12 > data.len() { break; }
            let tag: [u8;4] = data[pos..pos+4].try_into().unwrap();
            let offset = u32::from_le_bytes(data[pos+4..pos+8].try_into().unwrap()) as usize;
            let size   = u32::from_le_bytes(data[pos+8..pos+12].try_into().unwrap()) as usize;
            dir.insert(tag, (offset, size));
            pos += 12;
        }
    }
    Ok(dir)
}

fn get_section<'d>(data: &'d [u8], dir: &HashMap<[u8;4], (usize, usize)>, tag: &[u8;4]) -> Option<&'d [u8]> {
    dir.get(tag).and_then(|&(off, size)| data.get(off..off+size))
}

/// Read a full-mode binary zbc file and reconstruct a Module.
pub fn read_zbc(data: &[u8]) -> Result<Module> {
    verify_zbc_version(data)?;  // strict-pin: magic + exact major/minor (versions.rs)
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let flags = u16::from_le_bytes([data[8], data[9]]);
    if (flags & 0x04) != 0 {
        bail!(
            "zbc file has SymOnly flag set: it is a debug-symbol sidecar (.zsym), \
             not a loadable module"
        );
    }
    let has_is_static = true;  // v1.0+ always has is_static in SIGS
    let dir = read_directory(data, sec_count)?;

    let namespace = get_section(data, &dir, b"NSPC")
        .map(|s| read_nspc(s))
        .transpose()?
        .unwrap_or_default();

    let pool_raw = get_section(data, &dir, b"STRS")
        .or_else(|| get_section(data, &dir, b"BSTR"))
        .map(|s| read_strs(s))
        .transpose()?
        .unwrap_or_default();

    let classes = get_section(data, &dir, b"TYPE")
        .map(|s| read_type(s, &pool_raw))
        .transpose()?
        .unwrap_or_default();

    let sigs = get_section(data, &dir, b"SIGS")
        .map(|s| read_sigs(s, &pool_raw, has_is_static))
        .transpose()?
        .unwrap_or_default();

    // Phase 3 S3c: zbc 1.0+ exclusive — IdMap always v1.0.
    let id_map = IdMap::for_v1(
        &pool_raw,
        sigs.iter().map(|s| s.name.clone()).collect(),
        classes.iter().map(|c| c.name.clone()).collect(),
    );

    let func_bodies = get_section(data, &dir, b"FUNC")
        .map(|s| read_func(s, &pool_raw, &id_map))
        .transpose()?
        .unwrap_or_default();

    let mut dbug_entries = get_section(data, &dir, b"DBUG")
        .map(|s| read_dbug(s, &pool_raw))
        .transpose()?
        .unwrap_or_default();

    // jit-type-specialization C2 P0 step 0.4 (zbc 1.8, 2026-05-27): per-
    // function register IrType bytes. Absent (legacy zbc / writer-only-bumped
    // path) → all functions get empty reg_types.
    let mut regt_entries = get_section(data, &dir, b"REGT")
        .map(read_regt)
        .transpose()?
        .unwrap_or_default();

    // Assemble functions from SIGS + FUNC + DBUG + REGT
    let mut functions: Vec<Function> = func_bodies.into_iter().enumerate().map(|(i, body)| {
        let sig = sigs.get(i);
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

    let name = if namespace.is_empty() { "unknown".to_owned() } else { namespace };
    let string_pool = rebuild_string_pool(&pool_raw, &mut functions);
    // 2026-05-02 add-method-group-conversion (D1b): FRCS section holds the
    // FuncRef cache slot count (u32). Absent / empty → 0.
    let func_ref_cache_slots = get_section(data, &dir, b"FRCS")
        .filter(|s| s.len() >= 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .unwrap_or(0);
    Ok(Module {
        name, string_pool, classes, functions,
        type_registry: rustc_hash::FxHashMap::default(),
        type_registry_vec: Vec::new(),
        func_index: rustc_hash::FxHashMap::default(),
        func_ref_cache_slots,
    })
}

#[cfg(test)]
#[path = "../zbc_reader_tests.rs"]
mod tests;
