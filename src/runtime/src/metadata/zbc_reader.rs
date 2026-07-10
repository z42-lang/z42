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
    StaticSetInsn, TypeofInsn, VCallInsn,
};
use super::formats::{ZpkgDep, ZPKG_MAGIC, ZBC_MAGIC};
use super::types::ExecMode;

// ── zbc wire format version (mirror of C# ZbcWriter.VersionMajor/Minor) ──────
//
// Strict-pin policy (freeze-zbc-v1, 2026-05-14):
// reader accepts exactly major == ZBC_VERSION_MAJOR && minor == ZBC_VERSION_MINOR.
// Bumping either requires synchronized update of:
//   1. src/compiler/z42.IR/BinaryFormat/ZbcWriter.cs (VersionMajor / VersionMinor)
//   2. these two constants
//   3. docs/design/runtime/zbc.md "Minor changelog" table
//   4. src/tests/zbc-format/generate-fixtures.sh regen
// See docs/design/runtime/zbc.md + .claude/rules/workflow.md for the full procedure.

pub const ZBC_VERSION_MAJOR: u16 = 1;
// 2026-05-30 add-test-timeout-attribute: TIDX v=3 carries per-test
// `timeout_ms: i32` after each TestEntry's TestCase array. 0 = no
// override (runner default applies); positive = per-test wallclock
// cap in ms. Strict-pin policy: pre-1.9 zbc no longer readable.
// 2026-06-09 add-attribute-reflection (C3): bumped to 1.10 — TYPE section adds
// per-class user-attribute refs (u16 count + (type-name, factory-func) str-idx
// pairs) after the type-param block. See read_type_section below.
// 2026-06-09 add-attribute-reflection-methods (C3b): bumped to 1.11 — SIGS
// section adds the same per-function attr refs after the type-param block.
// 2026-06-10 add-reflection-type-flags: bumped to 1.12 — TYPE section appends
// a class-shape flags byte (abstract/sealed/struct/record) per class record.
// 2026-06-10 add-reflection-static-fields: bumped to 1.13 — TYPE section
// appends a static-fields block (u16 count + per-field) after the flags byte.
// 2026-06-10 add-field-attribute-reflection: bumped to 1.14 — each field record
// (instance + static block) appends a per-field attr-ref block after type_str.
// 2026-06-10 add-parameter-attribute-reflection: bumped to 1.15 — each SIGS
// function record appends, after the method-level attr block, a per-parameter
// attr-ref block (param_count blocks of u16 count + (type, factory) pairs).
// 2026-06-11 add-reflection-array-element-type: bumped to 1.16 — ArrayNew /
// ArrayNewLit append an element-type-name string-pool index (`element_type`),
// stored on the runtime ArrayObj so arr.GetType().GetElementType() is non-erased.
// 2026-06-14 add-reflection-get-interfaces: bumped to 1.17 — TYPE section appends
// a per-class interface block (u16 count + name str idx[]) after the static-fields
// block. Loaded into TypeDescCold.interfaces; surfaced by Type.GetInterfaces().
// 2026-06-16 add-reflection-generic-type-definition: bumped to 1.18 — new Typeof
// opcode (0x73) carries structured generic instantiation args (TypeName str idx +
// u8 count + str idx[]), replacing the __typeof builtin. Surfaces
// Type.IsGenericTypeDefinition / GetGenericTypeDefinition + fixes GetGenericArguments.
// 2026-06-16 add-reflection-interface-class-predicates: bumped to 1.19 —
// interfaces now emit a (minimal) TYPE entry; class_flags byte gains bit4 =
// interface. Surfaces Type.IsInterface + excludes interfaces from Type.IsClass.
// 2026-06-16 add-reflection-assignable-from: bumped to 1.20 — TYPE-section
// interface block now stores fully-qualified interface names (was bare).
// Real interface handles from GetInterfaces() + robust interface identity for
// is/as/IsAssignableFrom. Structure unchanged; field semantics bare→FQ.
// 2026-07-09 reencode-strs-segment-dict: bumped to 1.21 — STRS re-encoded as a
// segment dictionary. Layout: segCount u32 + (varint segLen + utf8)×segCount +
// strCount u32 + (varint segN + varint segIdx×segN)×strCount. Each pooled string
// is the '.'-join of its segment sequence. Removes the redundant per-string
// offset field and dedups namespace prefixes (−44% STRS on z42.core). String
// pool indices (how other sections reference strings) are unchanged.
// 2026-07-09 add-enum-type-metadata (unify-type-metadata P1-a): bumped to 1.22 —
// enum types now emit a TYPE-section entry with CLASS_FLAG_ENUM (bit5) and a
// trailing enum-member block (member_count:u16 + (name_idx:u32, value:i64)×n),
// gated on the flag so non-enum class records are byte-unchanged. Backs
// Type.IsEnum + Enum.GetNames/GetValues/GetName + typeof(EnumType).
// 2026-07-09 add-member-visibility (unify P1-b): bumped to 1.23 - TYPE field
// blocks (instance + static) gain a trailing visibility:u8 per field; SIGS gains a
// visibility:u8 after is_static. 0=public/1=private/2=protected. Backs
// FieldInfo.IsPublic/IsPrivate + MethodInfo.IsPublic/IsPrivate.
// 2026-07-10 add-method-modifiers (unify P1-c): bumped to 1.24 - SIGS gains a
// method_flags:u8 after visibility (bit0=virtual/bit1=abstract). Backs
// MethodInfo.IsVirtual (authoritative) + IsAbstract.
pub const ZBC_VERSION_MINOR: u16 = 24;

// ── zpkg wire format version (mirror of C# ZpkgWriter.VersionMajor/Minor) ────
//
// Strict-pin policy (freeze-zpkg-v0, 2026-05-14). Same coupling rules as zbc:
// reader accepts exactly major == ZPKG_VERSION_MAJOR && minor == ZPKG_VERSION_MINOR.
// Additionally, zbc minor bump REQUIRES zpkg minor bump (strong coupling per
// docs/design/runtime/zpkg.md). See .claude/rules/workflow.md for the bump
// procedure (zbc bump → 4 zbc steps + 4 zpkg steps in the same commit).

pub const ZPKG_VERSION_MAJOR: u16 = 0;
// 2026-06-06 aggregate-zpkg-tidx: bumped to 0.11 to match ZpkgWriter; the
// only zpkg wire-format delta is the per-module trailing `tidx_len: u32 +
// tidx_data` after REGT inside MODS. Reader for those new fields lives
// below in `read_mods_section` (returns 3-tuple incl. raw TIDX bytes);
// `loader::load_zpkg_bytes` aggregates entries with cumulative function +
// string-pool offsets so the public LoadedArtifact.test_index resolves
// against the merged module's index space.
// 2026-06-09 add-attribute-reflection (C3): bumped to 0.12 to mirror ZpkgWriter,
// coupled with inner zbc 1.10 (per-class attribute refs). Outer zpkg layout
// unchanged; bump tracks the inner zbc format change per the coupling rule.
// 2026-06-09 add-attribute-reflection-methods (C3b): bumped to 0.13, coupled
// with inner zbc 1.11 (per-function attr refs).
// 2026-06-10 add-reflection-type-flags: bumped to 0.14, coupled with inner
// zbc 1.12 (TYPE section class-shape flags byte).
// 2026-06-10 add-reflection-static-fields: bumped to 0.15, coupled with inner
// zbc 1.13 (TYPE section static-fields block).
// 2026-06-10 add-field-attribute-reflection: bumped to 0.16, coupled with inner
// zbc 1.14 (per-field attr-ref block).
// 2026-06-10 add-parameter-attribute-reflection: bumped to 0.17, coupled with
// inner zbc 1.15 (per-parameter attr-ref block in SIGS).
// 2026-06-11 add-reflection-array-element-type: bumped to 0.18, coupled with
// inner zbc 1.16 (ArrayNew/ArrayNewLit element-type field).
// 2026-06-14 add-reflection-get-interfaces: bumped to 0.19, coupled with inner
// zbc 1.17 (TYPE section per-class interface block).
// 2026-06-16 add-reflection-generic-type-definition: bumped to 0.20, coupled with
// inner zbc 1.18 (new Typeof opcode w/ structured generic args).
// 2026-06-16 add-reflection-interface-class-predicates: bumped to 0.21, coupled
// with inner zbc 1.19 (interfaces emit minimal TYPE entry; class_flags bit4).
// 2026-06-16 add-reflection-assignable-from: bumped to 0.22, coupled with inner
// zbc 1.20 (TYPE-section interface block stores FQ names).
// 2026-07-01 add-params-varargs: bumped to 0.23 (zpkg-only; inner zbc unchanged).
// TSIG method/function records gain a trailing paramsFrom byte (0xFF = none)
// right after the existing paramCount byte, before the per-parameter entries.
// 2026-07-08 add-indexed-zpkg-min-patch: bumped to 0.24 (zpkg-only; inner zbc
// unchanged; packed byte layout unchanged). Indexed mode redefined: main file =
// packed sections minus MODS plus FILE (per file: ns/src/srcHash pool idx +
// fnCount u16 + firstSig u32 + zbcHash pool idx — BLAKE3-128 hex of the
// scattered self-contained fullMode <stem>.zbc). VM loads indexed via the
// path-aware loader (scattered zbc verified against zbcHash).
// 2026-07-09 reencode-strs-segment-dict: bumped to 0.25, coupled with inner zbc
// 1.21 (STRS segment-dict re-encoding). Outer zpkg section layout unchanged; the
// STRS section body — shared with .zbc via the same reader — carries the new
// segment-dict encoding. Also applies to the .zsym sidecar's symPool STRS.
// 2026-07-09 add-enum-type-metadata: bumped to 0.26, coupled with inner zbc 1.22
// (TYPE-section enum member block). Outer zpkg layout unchanged.
// 2026-07-09 add-member-visibility: bumped to 0.27, coupled inner zbc 1.23
// (TYPE/SIGS member visibility). Outer zpkg layout unchanged.
// 2026-07-10 add-method-modifiers: bumped to 0.28, coupled inner zbc 1.24
// (SIGS +method_flags:u8). Outer zpkg layout unchanged.
pub const ZPKG_VERSION_MINOR: u16 = 28;

// ── Opcode constants (must match C# Opcodes.cs) ───────────────────────────────

const OP_CONST_I: u8     = 0x00;
const OP_CONST_F: u8     = 0x01;
const OP_CONST_BOOL: u8  = 0x02;
const OP_CONST_STR: u8   = 0x03;
const OP_CONST_NULL: u8  = 0x04;
const OP_COPY: u8        = 0x05;
const OP_CONST_CHAR: u8  = 0x08;

const OP_ADD: u8         = 0x10;
const OP_SUB: u8         = 0x11;
const OP_MUL: u8         = 0x12;
const OP_DIV: u8         = 0x13;
const OP_REM: u8         = 0x14;
const OP_NEG: u8         = 0x15;
const OP_AND: u8         = 0x16;
const OP_OR: u8          = 0x17;
const OP_NOT: u8         = 0x18;
const OP_BIT_AND: u8     = 0x19;
const OP_BIT_OR: u8      = 0x1A;
const OP_BIT_XOR: u8     = 0x1B;
const OP_BIT_NOT: u8     = 0x1C;
const OP_SHL: u8         = 0x1D;
const OP_SHR: u8         = 0x1E;
const OP_TO_STR: u8      = 0x1F;

const OP_EQ: u8          = 0x30;
const OP_NE: u8          = 0x31;
const OP_LT: u8          = 0x32;
const OP_LE: u8          = 0x33;
const OP_GT: u8          = 0x34;
const OP_GE: u8          = 0x35;

const OP_BR: u8          = 0x40;
const OP_BR_COND: u8     = 0x41;
const OP_RET: u8         = 0x42;
const OP_RET_VAL: u8     = 0x43;
const OP_THROW: u8       = 0x44;

const OP_CALL: u8                = 0x50;
const OP_BUILTIN: u8             = 0x51;
const OP_VCALL: u8               = 0x52;
const OP_CALL_NATIVE: u8         = 0x53;
const OP_CALL_NATIVE_VTABLE: u8  = 0x54;
const OP_LOAD_FN: u8             = 0x55;
const OP_CALL_INDIRECT: u8       = 0x56;
const OP_MK_CLOS: u8             = 0x57;
const OP_LOAD_FN_CACHED: u8      = 0x58;  // D1b add-method-group-conversion

const OP_FIELD_GET: u8   = 0x60;
const OP_FIELD_SET: u8   = 0x61;
const OP_STATIC_GET: u8  = 0x62;
const OP_STATIC_SET: u8  = 0x63;

const OP_OBJ_NEW: u8     = 0x70;
const OP_IS_INSTANCE: u8 = 0x71;
const OP_AS_CAST: u8     = 0x72;
const OP_TYPEOF: u8      = 0x73;

const OP_ARRAY_NEW: u8     = 0x80;
const OP_ARRAY_NEW_LIT: u8 = 0x81;
const OP_ARRAY_GET: u8     = 0x82;
const OP_ARRAY_SET: u8     = 0x83;
const OP_ARRAY_LEN: u8     = 0x84;
const OP_STR_CONCAT: u8    = 0x85;

const OP_PIN_PTR: u8       = 0x90;
const OP_UNPIN_PTR: u8     = 0x91;

// Spec impl-ref-out-in-runtime: address-load opcodes producing Value::Ref.
const OP_LOAD_LOCAL_ADDR: u8 = 0xA0;
const OP_LOAD_ELEM_ADDR:  u8 = 0xA1;
const OP_LOAD_FIELD_ADDR: u8 = 0xA2;

// add-default-generic-typeparam (D-8b-3 Phase 2): runtime resolution of
// `default(T)` where T is a generic type-parameter on the receiver class.
const OP_DEFAULT_OF: u8 = 0xB0;
// fix-numeric-cast-lowering (2026-05-13): explicit numeric type conversion.
const OP_CONVERT: u8 = 0xB1;

// ── Type tag constants ────────────────────────────────────────────────────────

const TAG_I64: u8 = 0x05;

// ── Low-level reader helpers ──────────────────────────────────────────────────

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self { Cursor { data, pos: 0 } }

    fn remaining(&self) -> usize { self.data.len() - self.pos }

    fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() { bail!("unexpected end of data (u8)") }
        let v = self.data[self.pos]; self.pos += 1; Ok(v)
    }
    fn read_u16(&mut self) -> Result<u16> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos+1]]);
        self.pos += 2; Ok(v)
    }
    fn read_u32(&mut self) -> Result<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.data[self.pos..self.pos+4].try_into().unwrap());
        self.pos += 4; Ok(v)
    }
    fn read_i32(&mut self) -> Result<i32> {
        self.need(4)?;
        let v = i32::from_le_bytes(self.data[self.pos..self.pos+4].try_into().unwrap());
        self.pos += 4; Ok(v)
    }
    fn read_i64(&mut self) -> Result<i64> {
        self.need(8)?;
        let v = i64::from_le_bytes(self.data[self.pos..self.pos+8].try_into().unwrap());
        self.pos += 8; Ok(v)
    }
    fn read_f64(&mut self) -> Result<f64> {
        self.need(8)?;
        let v = f64::from_le_bytes(self.data[self.pos..self.pos+8].try_into().unwrap());
        self.pos += 8; Ok(v)
    }
    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.need(n)?;
        let s = &self.data[self.pos..self.pos+n]; self.pos += n; Ok(s)
    }
    /// Unsigned LEB128 varint (STRS segment-dict). Max 5 bytes for u32.
    fn read_varint(&mut self) -> Result<u32> {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;
        for _ in 0..5 {
            let b = self.read_u8()?;
            result |= ((b & 0x7F) as u32) << shift;
            if b & 0x80 == 0 { return Ok(result); }
            shift += 7;
        }
        bail!("varint too long (>5 bytes)")
    }
    fn read_utf8_u16len(&mut self) -> Result<String> {
        let len = self.read_u16()? as usize;
        let b = self.read_bytes(len)?;
        Ok(std::str::from_utf8(b)?.to_owned())
    }
    fn need(&self, n: usize) -> Result<()> {
        if self.pos + n > self.data.len() { bail!("unexpected end of data") }
        Ok(())
    }
    fn pool_str<'p>(&self, pool: &'p [String], idx: u32) -> Result<&'p str> {
        pool.get(idx as usize)
            .map(|s| s.as_str())
            .ok_or_else(|| anyhow::anyhow!("string pool index {} out of range (pool size {})", idx, pool.len()))
    }
}

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

// ── String heap (STRS / BSTR) ─────────────────────────────────────────────────

/// STRS segment-dict (zbc 1.21 / zpkg 0.25): unique `.`-split segments deduped once,
/// each string = sequence of segment indices, reconstructed via `join('.')`.
fn read_strs(sec: &[u8]) -> Result<Vec<String>> {
    let mut c = Cursor::new(sec);
    let seg_count = c.read_u32()? as usize;
    let mut seg_dict: Vec<&str> = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        let len = c.read_varint()? as usize;
        let b = c.read_bytes(len)?;
        seg_dict.push(std::str::from_utf8(b)?);
    }
    let str_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(str_count);
    for _ in 0..str_count {
        let seg_n = c.read_varint()? as usize;
        let mut name = String::new();
        for j in 0..seg_n {
            let seg_idx = c.read_varint()? as usize;
            let seg = seg_dict.get(seg_idx).ok_or_else(|| {
                anyhow::anyhow!("STRS segment index {} out of range ({})", seg_idx, seg_count)
            })?;
            if j > 0 { name.push('.'); }
            name.push_str(seg);
        }
        result.push(name);
    }
    Ok(result)
}

// ── NSPC section ─────────────────────────────────────────────────────────────

fn read_nspc(sec: &[u8]) -> Result<String> {
    if sec.len() < 2 { return Ok(String::new()); }
    let len = u16::from_le_bytes([sec[0], sec[1]]) as usize;
    if len == 0 || sec.len() < 2 + len { return Ok(String::new()); }
    Ok(std::str::from_utf8(&sec[2..2+len])?.to_owned())
}

// ── TYPE section ──────────────────────────────────────────────────────────────

fn read_type(sec: &[u8], pool: &[String]) -> Result<Vec<ClassDesc>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut classes = Vec::with_capacity(count);
    for _ in 0..count {
        let name_idx = c.read_u32()?;
        let base_idx = c.read_u32()?;
        let fld_count = c.read_u16()? as usize;
        let name = c.pool_str(pool, name_idx)?.to_owned();
        let base_class = if base_idx == u32::MAX {
            None
        } else {
            Some(c.pool_str(pool, base_idx)?.to_owned())
        };
        let mut fields = Vec::with_capacity(fld_count);
        for _ in 0..fld_count {
            let fnam_idx = c.read_u32()?;
            let _type_tag_hint = c.read_u8()?;       // 1.7: tag retained as hint only
            let type_str_idx = c.read_u32()?;        // 1.7 align-zbc-reader-writer-asymmetry: authoritative
            let name = c.pool_str(pool, fnam_idx)?.to_owned();
            let type_tag = c.pool_str(pool, type_str_idx)?.to_owned();
            let attributes = read_attr_refs(&mut c, pool)?;  // 1.14 field attrs
            let visibility = c.read_u8()?;                    // 1.23 add-member-visibility
            fields.push(FieldDesc { name, type_tag, attributes, visibility });
        }
        // Generic type parameters + per-tp constraints (L3-G3a)
        let tp_count = c.read_u8()? as usize;
        let mut type_params = Vec::with_capacity(tp_count);
        let mut type_param_constraints = Vec::with_capacity(tp_count);
        for _ in 0..tp_count {
            let tp_idx = c.read_u32()?;
            type_params.push(c.pool_str(pool, tp_idx)?.to_owned());
            type_param_constraints.push(read_constraint_bundle(&mut c, pool)?);
        }
        // C3 add-attribute-reflection (zbc 1.10): per-class user attribute refs.
        let attr_count = c.read_u16()? as usize;
        let mut attributes = Vec::with_capacity(attr_count);
        for _ in 0..attr_count {
            let type_idx = c.read_u32()?;
            let factory_idx = c.read_u32()?;
            attributes.push(crate::metadata::bytecode::AttributeRef {
                type_name: c.pool_str(pool, type_idx)?.to_owned(),
                factory_func: c.pool_str(pool, factory_idx)?.to_owned(),
            });
        }
        // add-reflection-type-flags (zbc 1.12): class-shape flags byte.
        let class_flags = c.read_u8()?;
        // add-reflection-static-fields (zbc 1.13): static fields block (same
        // shape as the instance fields block above).
        let static_count = c.read_u16()? as usize;
        let mut static_fields = Vec::with_capacity(static_count);
        for _ in 0..static_count {
            let snam_idx = c.read_u32()?;
            let _type_tag_hint = c.read_u8()?;
            let type_str_idx = c.read_u32()?;
            let name = c.pool_str(pool, snam_idx)?.to_owned();
            let type_tag = c.pool_str(pool, type_str_idx)?.to_owned();
            let attributes = read_attr_refs(&mut c, pool)?;  // 1.14 field attrs
            let visibility = c.read_u8()?;                    // 1.23 add-member-visibility
            static_fields.push(crate::metadata::bytecode::FieldDesc { name, type_tag, attributes, visibility });
        }
        // add-reflection-get-interfaces (zbc 1.17): per-class interface block —
        // u16 count + interface_name_idx[] u32. Surfaced by Type.GetInterfaces().
        let iface_count = c.read_u16()? as usize;
        let mut interfaces = Vec::with_capacity(iface_count);
        for _ in 0..iface_count {
            let idx = c.read_u32()?;
            interfaces.push(c.pool_str(pool, idx)?.to_owned());
        }
        // add-enum-type-metadata (zbc 1.22): trailing enum-member block, present
        // only when CLASS_FLAG_ENUM is set. member_count:u16 + (name_idx, i64)×n.
        let enum_members = if class_flags & crate::metadata::bytecode::CLASS_FLAG_ENUM != 0 {
            let em_count = c.read_u16()? as usize;
            let mut ems = Vec::with_capacity(em_count);
            for _ in 0..em_count {
                let nidx = c.read_u32()?;
                let val = c.read_i64()?;
                ems.push((c.pool_str(pool, nidx)?.to_owned(), val));
            }
            ems.into_boxed_slice()
        } else {
            Box::new([]) as Box<[(String, i64)]>
        };
        classes.push(ClassDesc {
            name,
            base_class,
            fields: fields.into_boxed_slice(),
            type_params: type_params.into_boxed_slice(),
            type_param_constraints: type_param_constraints.into_boxed_slice(),
            attributes: attributes.into_boxed_slice(),
            class_flags,
            static_fields: static_fields.into_boxed_slice(),
            interfaces: interfaces.into_boxed_slice(),
            enum_members,
        });
    }
    Ok(classes)
}

/// Decode one constraint bundle. Mirrors ZbcWriter.WriteConstraintBundle (v0.6).
/// Layout: `flags: u8, [if bit2] base_class_idx: u32, [if bit3] type_param_constraint_idx: u32,
///          interface_count: u8, iface_idx[]: u32`.
/// add-field-attribute-reflection (zbc 1.14): read a per-field attr-ref block
/// (u16 count + (type-name, factory) str-idx pairs).
fn read_attr_refs(
    c: &mut Cursor,
    pool: &[String],
) -> Result<Box<[crate::metadata::bytecode::AttributeRef]>> {
    let count = c.read_u16()? as usize;
    let mut refs = Vec::with_capacity(count);
    for _ in 0..count {
        let type_idx = c.read_u32()?;
        let factory_idx = c.read_u32()?;
        refs.push(crate::metadata::bytecode::AttributeRef {
            type_name: c.pool_str(pool, type_idx)?.to_owned(),
            factory_func: c.pool_str(pool, factory_idx)?.to_owned(),
        });
    }
    Ok(refs.into_boxed_slice())
}

fn read_constraint_bundle(c: &mut Cursor, pool: &[String]) -> Result<ConstraintBundle> {
    let flags = c.read_u8()?;
    let requires_class       = (flags & 0x01) != 0;
    let requires_struct      = (flags & 0x02) != 0;
    let has_base             = (flags & 0x04) != 0;
    let has_type_param       = (flags & 0x08) != 0;
    let requires_constructor = (flags & 0x10) != 0;
    let requires_enum        = (flags & 0x20) != 0;
    let has_func_sig         = (flags & 0x40) != 0; // add-generic-func-constraint (zbc 1.4+)
    let base_class = if has_base {
        let idx = c.read_u32()?;
        Some(c.pool_str(pool, idx)?.to_owned())
    } else { None };
    let type_param_constraint = if has_type_param {
        let idx = c.read_u32()?;
        Some(c.pool_str(pool, idx)?.to_owned())
    } else { None };
    let iface_count = c.read_u8()? as usize;
    let mut interfaces = Vec::with_capacity(iface_count);
    for _ in 0..iface_count {
        let idx = c.read_u32()?;
        interfaces.push(c.pool_str(pool, idx)?.to_owned());
    }
    let func_signature = if has_func_sig {
        let param_count = c.read_u8()? as usize;
        let mut params = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            let idx = c.read_u32()?;
            params.push(c.pool_str(pool, idx)?.to_owned());
        }
        let ret_idx = c.read_u32()?;
        let ret = c.pool_str(pool, ret_idx)?.to_owned();
        Some(crate::metadata::bytecode::FuncSigDescriptor { params, ret })
    } else { None };
    Ok(ConstraintBundle {
        requires_class, requires_struct, base_class, interfaces, type_param_constraint,
        requires_constructor, requires_enum,
        func_signature,
    })
}

// ── SIGS section ─────────────────────────────────────────────────────────────

struct FuncSig {
    name: String,
    param_count: usize,
    ret_type: String,
    exec_mode: ExecMode,
    is_static: bool,
    /// 1.23 add-member-visibility: 0=public / 1=private / 2=protected. Surfaced by
    /// MethodInfo.IsPublic / IsPrivate.
    visibility: u8,
    /// 1.24 add-method-modifiers: bit0=virtual / bit1=abstract. Surfaced by
    /// MethodInfo.IsVirtual (authoritative) / IsAbstract.
    method_flags: u8,
    /// 1.3 split-debug-symbols: per-parameter type names for trace signature
    /// decoration. Length always equals `param_count` (writer pads unknowns
    /// with "?"). Empty Vec when param_count == 0.
    param_types: Vec<String>,
    type_params: Vec<String>,
    type_param_constraints: Vec<ConstraintBundle>,
    /// C3b add-attribute-reflection-methods: user attributes on this function.
    custom_attributes: Vec<crate::metadata::bytecode::AttributeRef>,
    /// add-parameter-attribute-reflection (zbc 1.15): per-parameter attributes,
    /// aligned by index with the SIGS parameter array (length == param_count,
    /// incl. the implicit `this` slot for instance methods).
    param_attributes: Vec<Box<[crate::metadata::bytecode::AttributeRef]>>,
}

fn read_sigs(sec: &[u8], pool: &[String], has_is_static: bool) -> Result<Vec<FuncSig>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut sigs = Vec::with_capacity(count);
    for _ in 0..count {
        let name_idx    = c.read_u32()?;
        let param_count = c.read_u16()? as usize;
        let _ret_tag_hint = c.read_u8()?;            // 1.7: tag retained as hint only
        let ret_type_idx = c.read_u32()?;            // 1.7 align-zbc-reader-writer-asymmetry: authoritative
        let mode_byte   = c.read_u8()?;
        let is_static   = if has_is_static { c.read_u8()? != 0 } else { false };
        let visibility  = if has_is_static { c.read_u8()? } else { 0 };  // 1.23 add-member-visibility (after is_static)
        let method_flags = if has_is_static { c.read_u8()? } else { 0 }; // 1.24 add-method-modifiers (after visibility)

        // 1.3 split-debug-symbols: per-param type names (u32 strIdx × param_count).
        let mut param_types = Vec::with_capacity(param_count);
        for _ in 0..param_count {
            let pt_idx = c.read_u32()?;
            param_types.push(c.pool_str(pool, pt_idx)?.to_owned());
        }

        // Generic type params (added after is_static) + per-tp constraints (L3-G3a)
        let tp_count    = if has_is_static { c.read_u8()? as usize } else { 0 };
        let mut type_params = Vec::with_capacity(tp_count);
        let mut type_param_constraints = Vec::with_capacity(tp_count);
        for _ in 0..tp_count {
            let tp_idx = c.read_u32()?;
            type_params.push(c.pool_str(pool, tp_idx)?.to_owned());
            type_param_constraints.push(read_constraint_bundle(&mut c, pool)?);
        }
        // C3b add-attribute-reflection-methods (zbc 1.11): per-function attr refs.
        let attr_count = if has_is_static { c.read_u16()? as usize } else { 0 };
        let mut custom_attributes = Vec::with_capacity(attr_count);
        for _ in 0..attr_count {
            let type_idx = c.read_u32()?;
            let factory_idx = c.read_u32()?;
            custom_attributes.push(crate::metadata::bytecode::AttributeRef {
                type_name: c.pool_str(pool, type_idx)?.to_owned(),
                factory_func: c.pool_str(pool, factory_idx)?.to_owned(),
            });
        }
        // add-parameter-attribute-reflection (zbc 1.15): per-parameter attr block —
        // exactly param_count attr-ref blocks (each u16 count + (type, factory) pairs).
        let mut param_attributes = Vec::with_capacity(param_count);
        if has_is_static {
            for _ in 0..param_count {
                param_attributes.push(read_attr_refs(&mut c, pool)?);
            }
        }
        sigs.push(FuncSig {
            name: c.pool_str(pool, name_idx)?.to_owned(),
            param_count,
            ret_type: c.pool_str(pool, ret_type_idx)?.to_owned(),
            exec_mode: exec_mode_from_byte(mode_byte),
            is_static,
            visibility,
            method_flags,
            param_types,
            type_params,
            type_param_constraints,
            custom_attributes,
            param_attributes,
        });
    }
    Ok(sigs)
}

// ── FUNC section ─────────────────────────────────────────────────────────────

struct FuncBody {
    blocks: Vec<BasicBlock>,
    exception_table: Vec<ExceptionEntry>,
    // 1.2 split-debug-symbols: LineTable moved out of FuncBody (FUNC section)
    // into DBUG section. The merged Function.line_table is populated at
    // assembly time from the (optional) DBUG content.
}

// ── Phase 3 S3c (tokenize-ir-and-zbc-bump, 2026-05-09) ────────────────────────
//
// IdMap: maps a v1.0 zbc IR-field token to its FQ name string.
//
//   • token < IMPORT_BASE   → `local_funcs[token]` or `local_classes[token]`
//   • token >= IMPORT_BASE  → `pool[token - IMPORT_BASE]` (cross-zpkg STRS idx)
//   • token == UNRESOLVED   → "<unresolved>" diagnostic placeholder
//
// Pre-1.0 reading was supported in S3a/b transitionally, removed in S3c per
// CLAUDE.md "不为旧版本提供兼容".

const IMPORT_BASE_TOKEN: u32 = 0x8000_0000;
const UNRESOLVED_TOKEN:  u32 = 0xFFFF_FFFF;

struct IdMap<'a> {
    pool: &'a [String],
    local_funcs:   Vec<String>,
    local_classes: Vec<String>,
}

impl<'a> IdMap<'a> {
    fn for_v1(pool: &'a [String], local_funcs: Vec<String>, local_classes: Vec<String>) -> Self {
        Self { pool, local_funcs, local_classes }
    }

    fn resolve_method(&self, token: u32) -> Result<String> {
        if token == UNRESOLVED_TOKEN {
            return Ok("<unresolved>".to_owned());
        }
        if token >= IMPORT_BASE_TOKEN {
            return pool_str_owned(self.pool, token - IMPORT_BASE_TOKEN);
        }
        self.local_funcs.get(token as usize)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(
                "zbc 1.0 method token {} out of range (local_funcs len {})",
                token, self.local_funcs.len()))
    }

    fn resolve_type(&self, token: u32) -> Result<String> {
        if token == UNRESOLVED_TOKEN {
            return Ok("<unresolved>".to_owned());
        }
        if token >= IMPORT_BASE_TOKEN {
            return pool_str_owned(self.pool, token - IMPORT_BASE_TOKEN);
        }
        self.local_classes.get(token as usize)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(
                "zbc 1.0 type token {} out of range (local_classes len {})",
                token, self.local_classes.len()))
    }
}

fn read_func(sec: &[u8], pool: &[String], id_map: &IdMap) -> Result<Vec<FuncBody>> {
    let mut c = Cursor::new(sec);
    let func_count = c.read_u32()? as usize;
    let mut bodies = Vec::with_capacity(func_count);

    for _ in 0..func_count {
        let _reg_count  = c.read_u16()?;
        let block_count = c.read_u16()? as usize;
        let instr_len   = c.read_u32()? as usize;
        let exc_count   = c.read_u16()? as usize;
        // 1.2 split-debug-symbols: line_count + line_table no longer in FUNC.

        let mut block_offsets = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            block_offsets.push(c.read_u32()? as usize);
        }

        let mut raw_exc = Vec::with_capacity(exc_count);
        for _ in 0..exc_count {
            let try_start  = c.read_u16()?;
            let try_end    = c.read_u16()?;
            let catch_blk  = c.read_u16()?;
            let catch_type = c.read_u32()?;
            let catch_reg  = c.read_u16()?;
            raw_exc.push((try_start, try_end, catch_blk, catch_type, catch_reg));
        }

        let instr_bytes = c.read_bytes(instr_len)?;

        // Decode blocks
        let mut blocks = Vec::with_capacity(block_count);
        for bi in 0..block_count {
            let start = block_offsets[bi];
            let end   = if bi + 1 < block_count { block_offsets[bi + 1] } else { instr_len };
            let label = if bi == 0 { "entry".to_owned() } else { format!("block_{bi}") };
            let (instrs, term) = decode_block(&instr_bytes[start..end], pool, id_map)?;
            blocks.push(BasicBlock { label, instructions: instrs, terminator: term });
        }

        // Resolve exception table block indices to labels
        let exception_table = raw_exc.into_iter().map(|(ts, te, cb, ct, cr)| {
            let try_start  = block_label(ts as usize);
            let try_end    = if (te as usize) < blocks.len() {
                block_label(te as usize)
            } else {
                format!("block_{}", blocks.len())
            };
            let catch_label = block_label(cb as usize);
            let catch_type  = if ct == u32::MAX { None } else {
                pool.get(ct as usize).map(|s| s.clone())
            };
            ExceptionEntry { try_start, try_end, catch_label, catch_type, catch_reg: cr as u32 }
        }).collect();

        bodies.push(FuncBody { blocks, exception_table });
    }
    Ok(bodies)
}

fn block_label(idx: usize) -> String {
    if idx == 0 { "entry".to_owned() } else { format!("block_{idx}") }
}

// ── DBUG section (line table + local variable names; 1.2+) ──────────────────

#[derive(Default, Clone, Debug)]
pub struct DbugFuncEntry {
    pub line_table: Vec<crate::metadata::bytecode::LineEntry>,
    pub local_vars: Vec<crate::metadata::bytecode::LocalVar>,
}

fn read_dbug(sec: &[u8], pool: &[String]) -> Result<Vec<DbugFuncEntry>> {
    let mut c = Cursor::new(sec);
    let func_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(func_count);

    for _ in 0..func_count {
        // ── Line table ───────────────────────────────────────────────────
        let line_count = c.read_u16()? as usize;
        let mut line_table = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let blk     = c.read_u16()? as u32;
            let ins     = c.read_u16()? as u32;
            let line    = c.read_u32()?;
            let file_id = c.read_u32()?;
            let column  = c.read_u32()?;
            let file = if file_id == u32::MAX { None } else {
                pool.get(file_id as usize).cloned()
            };
            line_table.push(crate::metadata::bytecode::LineEntry {
                block: blk, instr: ins, line, file, column,
            });
        }

        // ── Local var table ──────────────────────────────────────────────
        let var_count = c.read_u16()? as usize;
        let mut local_vars = Vec::with_capacity(var_count);
        for _ in 0..var_count {
            let name_idx = c.read_u32()? as usize;
            let reg = c.read_u16()?;
            let name = pool.get(name_idx).cloned().unwrap_or_else(|| format!("?{name_idx}"));
            local_vars.push(crate::metadata::bytecode::LocalVar { name, reg });
        }

        result.push(DbugFuncEntry { line_table, local_vars });
    }
    Ok(result)
}

/// jit-type-specialization C2 P0 step 0.4 (zbc 1.8, 2026-05-27): decode the
/// REGT section into one `Box<[IrType]>` per function, indexed by position.
/// Reader is liberal — unknown byte values decode as `IrType::Unknown` (per
/// `IrType::from_u8`), so writer-side variant additions don't break older
/// runtimes.
fn read_regt(sec: &[u8]) -> Result<Vec<Box<[crate::metadata::IrType]>>> {
    use crate::metadata::IrType;
    let mut c = Cursor::new(sec);
    let func_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(func_count);
    for _ in 0..func_count {
        let reg_count = c.read_u32()? as usize;
        if reg_count == 0 {
            result.push(Box::new([]) as Box<[IrType]>);
            continue;
        }
        let mut types = Vec::with_capacity(reg_count);
        for _ in 0..reg_count {
            types.push(IrType::from_u8(c.read_u8()?));
        }
        result.push(types.into_boxed_slice());
    }
    Ok(result)
}

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

fn read_mdbg_section(sec: &[u8], pool: &[String]) -> Result<Vec<(String, Vec<DbugFuncEntry>)>> {
    let mut c = Cursor::new(sec);
    let mod_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(mod_count);
    for _ in 0..mod_count {
        let ns_idx   = c.read_u32()?;
        let dbug_len = c.read_u32()? as usize;
        let dbug     = c.read_bytes(dbug_len)?;
        let ns = pool_str_owned(pool, ns_idx)?;
        let entries = if dbug.is_empty() { Vec::new() } else { read_dbug(dbug, pool)? };
        result.push((ns, entries));
    }
    Ok(result)
}

// ── Block decoding ────────────────────────────────────────────────────────────

fn decode_block(data: &[u8], pool: &[String], id_map: &IdMap) -> Result<(Vec<Instruction>, Terminator)> {
    let mut c = Cursor::new(data);
    let mut instrs = Vec::new();

    while c.remaining() > 0 {
        let op  = c.read_u8()?;
        let typ = c.read_u8()?;
        let dst = c.read_u16()? as u32;

        match op {
            OP_RET     => return Ok((instrs, Terminator::Ret { reg: None })),
            OP_RET_VAL => return Ok((instrs, Terminator::Ret { reg: Some(dst) })),
            OP_BR      => {
                let lbl = c.read_u16()? as usize;
                return Ok((instrs, Terminator::Br { label: block_label(lbl) }));
            }
            OP_BR_COND => {
                let t = c.read_u16()? as usize;
                let f = c.read_u16()? as usize;
                return Ok((instrs, Terminator::BrCond {
                    cond: dst,
                    true_label:  block_label(t),
                    false_label: block_label(f),
                }));
            }
            OP_THROW => return Ok((instrs, Terminator::Throw { reg: dst })),
            _ => instrs.push(decode_instr(op, typ, dst, &mut c, pool, id_map)?),
        }
    }
    Ok((instrs, Terminator::Ret { reg: None }))
}

fn decode_instr(op: u8, typ: u8, dst: u32, c: &mut Cursor, pool: &[String], id_map: &IdMap) -> Result<Instruction> {
    let instr = match op {
        OP_CONST_STR  => Instruction::ConstStr { dst, idx: c.read_u32()? },
        OP_CONST_I if typ == TAG_I64
                      => Instruction::ConstI64 { dst, val: c.read_i64()? },
        OP_CONST_I    => Instruction::ConstI32 { dst, val: c.read_i32()? },
        OP_CONST_F    => Instruction::ConstF64 { dst, val: c.read_f64()? },
        OP_CONST_BOOL => Instruction::ConstBool { dst, val: c.read_u8()? != 0 },
        OP_CONST_CHAR => {
            let code_point = c.read_i32()? as u32;
            Instruction::ConstChar { dst, val: char::from_u32(code_point).unwrap_or('\0') }
        }
        OP_CONST_NULL => Instruction::ConstNull { dst },
        OP_COPY       => Instruction::Copy { dst, src: c.read_u16()? as u32 },

        OP_ADD     => { let (a,b) = read_ab(c)?; Instruction::Add { dst, a, b } }
        OP_SUB     => { let (a,b) = read_ab(c)?; Instruction::Sub { dst, a, b } }
        OP_MUL     => { let (a,b) = read_ab(c)?; Instruction::Mul { dst, a, b } }
        OP_DIV     => { let (a,b) = read_ab(c)?; Instruction::Div { dst, a, b } }
        OP_REM     => { let (a,b) = read_ab(c)?; Instruction::Rem { dst, a, b } }
        OP_AND     => { let (a,b) = read_ab(c)?; Instruction::And { dst, a, b } }
        OP_OR      => { let (a,b) = read_ab(c)?; Instruction::Or  { dst, a, b } }
        OP_BIT_AND => { let (a,b) = read_ab(c)?; Instruction::BitAnd { dst, a, b } }
        OP_BIT_OR  => { let (a,b) = read_ab(c)?; Instruction::BitOr  { dst, a, b } }
        OP_BIT_XOR => { let (a,b) = read_ab(c)?; Instruction::BitXor { dst, a, b } }
        OP_SHL     => { let (a,b) = read_ab(c)?; Instruction::Shl { dst, a, b } }
        OP_SHR     => { let (a,b) = read_ab(c)?; Instruction::Shr { dst, a, b } }
        OP_STR_CONCAT => { let (a,b) = read_ab(c)?; Instruction::StrConcat { dst, a, b } }
        OP_EQ      => { let (a,b) = read_ab(c)?; Instruction::Eq { dst, a, b } }
        OP_NE      => { let (a,b) = read_ab(c)?; Instruction::Ne { dst, a, b } }
        OP_LT      => { let (a,b) = read_ab(c)?; Instruction::Lt { dst, a, b } }
        OP_LE      => { let (a,b) = read_ab(c)?; Instruction::Le { dst, a, b } }
        OP_GT      => { let (a,b) = read_ab(c)?; Instruction::Gt { dst, a, b } }
        OP_GE      => { let (a,b) = read_ab(c)?; Instruction::Ge { dst, a, b } }

        OP_NEG     => Instruction::Neg    { dst, src: c.read_u16()? as u32 },
        OP_NOT     => Instruction::Not    { dst, src: c.read_u16()? as u32 },
        OP_BIT_NOT => Instruction::BitNot { dst, src: c.read_u16()? as u32 },
        OP_TO_STR  => Instruction::ToStr  { dst, src: c.read_u16()? as u32 },
        OP_ARRAY_LEN => Instruction::ArrayLen { dst, arr: c.read_u16()? as u32 },

        OP_CALL => {
            // Phase 3 S3a (tokenize-ir-and-zbc-bump, 2026-05-09): IdMap dispatches
            // to v0.9 (pool_str) or v1.0 (IMPORT_BASE bit) decode based on header.
            let func = id_map.resolve_method(c.read_u32()?)?;
            let args = read_args(c)?;
            Instruction::Call(Box::new(CallInsn { dst, func, args }))
        }
        OP_LOAD_FN => {
            let func = id_map.resolve_method(c.read_u32()?)?;
            Instruction::LoadFn(Box::new(LoadFnInsn { dst, func }))
        }
        OP_LOAD_FN_CACHED => {
            let func    = id_map.resolve_method(c.read_u32()?)?;
            let slot_id = c.read_u32()?;
            Instruction::LoadFnCached(Box::new(LoadFnCachedInsn { dst, func, slot_id }))
        }
        OP_CALL_INDIRECT => {
            let callee = c.read_u16()? as u32;
            let args   = read_args(c)?;
            Instruction::CallIndirect { dst, callee, args }
        }
        OP_MK_CLOS => {
            let fn_name     = id_map.resolve_method(c.read_u32()?)?;
            // 2026-05-02 impl-closure-l3-escape-stack: 1 byte flag
            let stack_alloc = c.read_u8()? != 0;
            let captures    = read_args(c)?;
            Instruction::MkClos(Box::new(MkClosInsn { dst, fn_name, captures, stack_alloc }))
        }
        OP_BUILTIN => {
            let name = pool_str_owned(pool, c.read_u32()?)?;
            let args = read_args(c)?;
            Instruction::Builtin(Box::new(BuiltinInsn { dst, name, args }))
        }
        OP_VCALL => {
            let method = pool_str_owned(pool, c.read_u32()?)?;
            let obj    = c.read_u16()? as u32;
            let args   = read_args(c)?;
            Instruction::VCall(Box::new(VCallInsn { dst, obj, method, args }))
        }
        OP_FIELD_GET => {
            let obj        = c.read_u16()? as u32;
            let field_name = pool_str_owned(pool, c.read_u32()?)?;
            Instruction::FieldGet(Box::new(FieldGetInsn { dst, obj, field_name }))
        }
        OP_FIELD_SET => {
            let obj        = c.read_u16()? as u32;
            let field_name = pool_str_owned(pool, c.read_u32()?)?;
            let val        = c.read_u16()? as u32;
            Instruction::FieldSet(Box::new(FieldSetInsn { obj, field_name, val }))
        }
        OP_STATIC_GET => Instruction::StaticGet(Box::new(StaticGetInsn { dst, field: pool_str_owned(pool, c.read_u32()?)? })),
        OP_STATIC_SET => {
            let field = pool_str_owned(pool, c.read_u32()?)?;
            let val   = c.read_u16()? as u32;
            Instruction::StaticSet(Box::new(StaticSetInsn { field, val }))
        }
        OP_OBJ_NEW => {
            let class_name = id_map.resolve_type(c.read_u32()?)?;
            let ctor_name  = id_map.resolve_method(c.read_u32()?)?;
            let args       = read_args(c)?;
            // D-8b-3 Phase 2: type_args list (resolved generic type-arguments)
            let t_count = c.read_u8()? as usize;
            let mut type_args = Vec::with_capacity(t_count);
            for _ in 0..t_count {
                type_args.push(pool_str_owned(pool, c.read_u32()?)?);
            }
            Instruction::ObjNew(Box::new(ObjNewInsn { dst, class_name, ctor_name, args, type_args: type_args.into_boxed_slice() }))
        }
        OP_TYPEOF => {
            // add-reflection-generic-type-definition: type_name + structured
            // generic args (mirrors ObjNew type_args encoding).
            let type_name = pool_str_owned(pool, c.read_u32()?)?;
            let t_count = c.read_u8()? as usize;
            let mut type_args = Vec::with_capacity(t_count);
            for _ in 0..t_count {
                type_args.push(pool_str_owned(pool, c.read_u32()?)?);
            }
            Instruction::Typeof(Box::new(TypeofInsn { dst, type_name, type_args: type_args.into_boxed_slice() }))
        }
        OP_IS_INSTANCE => {
            let obj        = c.read_u16()? as u32;
            let class_name = id_map.resolve_type(c.read_u32()?)?;
            Instruction::IsInstance(Box::new(IsInstanceInsn { dst, obj, class_name }))
        }
        OP_AS_CAST => {
            let obj        = c.read_u16()? as u32;
            let class_name = id_map.resolve_type(c.read_u32()?)?;
            Instruction::AsCast(Box::new(AsCastInsn { dst, obj, class_name }))
        }
        OP_ARRAY_NEW     => {
            let size = c.read_u16()? as u32;
            let elem_tag = c.read_u8()?;
            // add-reflection-array-element-type (zbc 1.16): element type FQ name.
            let et_idx = c.read_u32()?;
            let element_type = c.pool_str(pool, et_idx)?.to_owned();
            Instruction::ArrayNew(Box::new(crate::metadata::bytecode::ArrayNewInsn { dst, size, elem_tag, element_type }))
        }
        OP_ARRAY_NEW_LIT => {
            let elems = read_args(c)?;
            let et_idx = c.read_u32()?;
            let element_type = c.pool_str(pool, et_idx)?.to_owned();
            Instruction::ArrayNewLit(Box::new(crate::metadata::bytecode::ArrayNewLitInsn { dst, elems, element_type }))
        }
        OP_ARRAY_GET     => {
            let arr = c.read_u16()? as u32;
            let idx = c.read_u16()? as u32;
            Instruction::ArrayGet { dst, arr, idx }
        }
        OP_ARRAY_SET     => {
            let arr = c.read_u16()? as u32;
            let idx = c.read_u16()? as u32;
            let val = c.read_u16()? as u32;
            Instruction::ArraySet { arr, idx, val }
        }

        OP_CALL_NATIVE => {
            let module    = pool_str_owned(pool, c.read_u32()?)?;
            let type_name = pool_str_owned(pool, c.read_u32()?)?;
            let symbol    = pool_str_owned(pool, c.read_u32()?)?;
            let args      = read_args(c)?;
            Instruction::CallNative(Box::new(CallNativeInsn { dst, module, type_name, symbol, args }))
        }
        OP_CALL_NATIVE_VTABLE => {
            let recv = c.read_u16()? as u32;
            let slot = c.read_u16()?;
            let args = read_args(c)?;
            Instruction::CallNativeVtable { dst, recv, vtable_slot: slot, args }
        }
        OP_PIN_PTR   => Instruction::PinPtr   { dst, src: c.read_u16()? as u32 },
        OP_UNPIN_PTR => Instruction::UnpinPtr { pinned: c.read_u16()? as u32 },

        // Spec impl-ref-out-in-runtime: address-load decoding (operand layout
        // mirrors C# `BinaryFormat/ZbcWriter.Instructions.cs`).
        OP_LOAD_LOCAL_ADDR => {
            let slot = c.read_u16()? as u32;
            Instruction::LoadLocalAddr { dst, slot }
        }
        OP_LOAD_ELEM_ADDR => {
            let arr = c.read_u16()? as u32;
            let idx = c.read_u16()? as u32;
            Instruction::LoadElemAddr { dst, arr, idx }
        }
        OP_LOAD_FIELD_ADDR => {
            let obj = c.read_u16()? as u32;
            let field_name = pool_str_owned(pool, c.read_u32()?)?;
            Instruction::LoadFieldAddr(Box::new(LoadFieldAddrInsn { dst, obj, field_name }))
        }

        // add-default-generic-typeparam (D-8b-3 Phase 2)
        OP_DEFAULT_OF => {
            let param_index = c.read_u8()?;
            Instruction::DefaultOf { dst, param_index }
        }

        // fix-numeric-cast-lowering (2026-05-13)
        OP_CONVERT => {
            let src = c.read_u16()? as u32;
            Instruction::Convert { dst, src, to_tag: typ }
        }

        _ => bail!("unknown opcode 0x{op:02X}"),
    };
    Ok(instr)
}

fn read_ab(c: &mut Cursor) -> Result<(u32, u32)> {
    Ok((c.read_u16()? as u32, c.read_u16()? as u32))
}

fn read_args(c: &mut Cursor) -> Result<Box<[u32]>> {
    let count = c.read_u8()? as usize;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count { args.push(c.read_u16()? as u32); }
    Ok(args.into_boxed_slice())
}

fn pool_str_owned(pool: &[String], idx: u32) -> Result<String> {
    pool.get(idx as usize)
        .map(|s| s.clone())
        .ok_or_else(|| anyhow::anyhow!("string pool index {} out of range", idx))
}

// ── String pool rebuild (ConstStr remap) ─────────────────────────────────────

/// Rebuilds the module-local string pool from the global pool + ConstStr references,
/// and remaps ConstStr.idx from global to local indices in-place.
fn rebuild_string_pool(global: &[String], funcs: &mut [Function]) -> Vec<String> {
    let mut seen: HashMap<u32, u32> = HashMap::new();
    let mut local: Vec<String> = Vec::new();

    for func in funcs.iter() {
        for block in &func.blocks {
            for instr in &block.instructions {
                if let Instruction::ConstStr { idx, .. } = instr {
                    if !seen.contains_key(idx) {
                        let s = global.get(*idx as usize).cloned().unwrap_or_default();
                        let local_idx = local.len() as u32;
                        seen.insert(*idx, local_idx);
                        local.push(s);
                    }
                }
            }
        }
    }

    // Remap in-place
    for func in funcs.iter_mut() {
        for block in &mut func.blocks {
            for instr in &mut block.instructions {
                if let Instruction::ConstStr { idx, .. } = instr {
                    if let Some(&new_idx) = seen.get(idx) {
                        *idx = new_idx;
                    }
                }
            }
        }
    }

    local
}

// ── zbc public API ────────────────────────────────────────────────────────────

/// Read a full-mode binary zbc file and reconstruct a Module.
pub fn read_zbc(data: &[u8]) -> Result<Module> {
    if data.len() < 16 { bail!("zbc file too short") }
    if &data[0..4] != ZBC_MAGIC { bail!("not a binary zbc (bad magic)") }

    let major     = u16::from_le_bytes([data[4], data[5]]);
    let minor     = u16::from_le_bytes([data[6], data[7]]);
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    // Strict-pin policy (freeze-zbc-v1, 2026-05-14): exact match with writer.
    // Pre-1.0 z42 doesn't keep older zbc minor readable; regen via
    // xtask regen. See docs/design/runtime/zbc.md.
    if major != ZBC_VERSION_MAJOR {
        bail!("zbc major {major} not supported (writer is at {ZBC_VERSION_MAJOR})");
    }
    if minor != ZBC_VERSION_MINOR {
        bail!(
            "zbc minor {minor} not supported (writer is at {ZBC_VERSION_MINOR}); \
             regen via xtask regen"
        );
    }
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
        };
        let cold = if cold_inner.param_types.is_empty()
            && cold_inner.exception_table.is_empty()
            && cold_inner.line_table.is_empty()
            && cold_inner.local_vars.is_empty()
            && cold_inner.type_params.is_empty()
            && cold_inner.type_param_constraints.is_empty()
            && cold_inner.custom_attributes.is_empty()
            && cold_inner.param_attributes.iter().all(|p| p.is_empty())
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
            max_reg:         0,
            cold,
            reg_types,
            block_index:     std::collections::HashMap::new(),
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
        type_registry: std::collections::HashMap::new(),
        type_registry_vec: Vec::new(),
        func_index: std::collections::HashMap::new(),
        func_ref_cache_slots,
        // Populated by `loader::build_interned_strings` after deserialize.
        interned_strings: Vec::new(),
    })
}

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

/// Read zpkg header metadata (fast path, no module decode).
pub fn read_zpkg_meta(data: &[u8]) -> Result<ZpkgInfo> {
    if data.len() < 16 { bail!("zpkg file too short") }
    if &data[0..4] != ZPKG_MAGIC { bail!("not a binary zpkg (bad magic)") }

    // Strict-pin check (freeze-zpkg-v0): even the lightweight meta-only reader
    // must reject mismatched versions — otherwise tooling could surface a stale
    // META and the user has no clear signal to regen.
    let major     = u16::from_le_bytes([data[4], data[5]]);
    let minor     = u16::from_le_bytes([data[6], data[7]]);
    if major != ZPKG_VERSION_MAJOR {
        bail!("zpkg major {major} not supported (writer is at {ZPKG_VERSION_MAJOR})");
    }
    if minor != ZPKG_VERSION_MINOR {
        bail!(
            "zpkg minor {minor} not supported (writer is at \
             {ZPKG_VERSION_MAJOR}.{ZPKG_VERSION_MINOR}); \
             regen via xtask build stdlib"
        );
    }
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
    if data.len() < 16 { bail!("zpkg file too short") }
    if &data[0..4] != ZPKG_MAGIC { bail!("not a binary zpkg (bad magic)") }

    let major     = u16::from_le_bytes([data[4], data[5]]);
    let minor     = u16::from_le_bytes([data[6], data[7]]);
    let flags     = u16::from_le_bytes([data[8], data[9]]);
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    let is_packed = flags & 0x01 != 0;
    // Strict-pin policy (freeze-zpkg-v0, 2026-05-14): exact match with writer.
    // Pre-1.0 z42 doesn't keep older zpkg minor readable; regen via
    // xtask build stdlib. See docs/design/runtime/zpkg.md.
    if major != ZPKG_VERSION_MAJOR {
        bail!("zpkg major {major} not supported (writer is at {ZPKG_VERSION_MAJOR})");
    }
    if minor != ZPKG_VERSION_MINOR {
        bail!(
            "zpkg minor {minor} not supported (writer is at \
             {ZPKG_VERSION_MAJOR}.{ZPKG_VERSION_MINOR}); \
             regen via xtask build stdlib"
        );
    }
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
    if data.len() < 16 { bail!("zpkg file too short") }
    if &data[0..4] != ZPKG_MAGIC { bail!("not a binary zpkg (bad magic)") }
    let major     = u16::from_le_bytes([data[4], data[5]]);
    let minor     = u16::from_le_bytes([data[6], data[7]]);
    let flags     = u16::from_le_bytes([data[8], data[9]]);
    let sec_count = u16::from_le_bytes([data[10], data[11]]);
    if major != ZPKG_VERSION_MAJOR {
        bail!("zpkg major {major} not supported (writer is at {ZPKG_VERSION_MAJOR})");
    }
    if minor != ZPKG_VERSION_MINOR {
        bail!(
            "zpkg minor {minor} not supported (writer is at \
             {ZPKG_VERSION_MAJOR}.{ZPKG_VERSION_MINOR}); \
             regen via xtask build stdlib"
        );
    }
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

// ── zpkg section decoders ─────────────────────────────────────────────────────

fn read_meta_section(sec: &[u8]) -> Result<(String, String, Option<String>)> {
    let mut c = Cursor::new(sec);
    let name    = c.read_utf8_u16len()?;
    let version = c.read_utf8_u16len()?;
    let entry_s = c.read_utf8_u16len()?;
    let entry   = if entry_s.is_empty() { None } else { Some(entry_s) };
    Ok((name, version, entry))
}

fn read_nspc_list(sec: &[u8], pool: &[String]) -> Result<Vec<String>> {
    let mut c = Cursor::new(sec);
    let count = c.read_u32()? as usize;
    let mut ns = Vec::with_capacity(count);
    for _ in 0..count {
        let idx = c.read_u32()?;
        ns.push(pool_str_owned(pool, idx)?);
    }
    Ok(ns)
}

fn read_deps_section(sec: &[u8], pool: &[String]) -> Result<Vec<ZpkgDep>> {
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
fn read_mods_section(
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
            };
            let cold = if cold_inner.param_types.is_empty()
                && cold_inner.exception_table.is_empty()
                && cold_inner.line_table.is_empty()
                && cold_inner.local_vars.is_empty()
                && cold_inner.type_params.is_empty()
                && cold_inner.type_param_constraints.is_empty()
                && cold_inner.custom_attributes.is_empty()
                && cold_inner.param_attributes.iter().all(|p| p.is_empty())
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
                max_reg:         0,
                cold,
                reg_types,
                block_index:     std::collections::HashMap::new(),
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
            type_registry: std::collections::HashMap::new(),
            type_registry_vec: Vec::new(),
            func_index: std::collections::HashMap::new(),
            func_ref_cache_slots: 0,
            // Populated inside `merge_modules` (these per-namespace modules
            // are always merged before consumption).
            interned_strings: Vec::new(),
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

// ── Conversion helpers ────────────────────────────────────────────────────────

/// Decode a u8 TypeTag to its canonical string. Kept as a debug / disasm
/// helper after 1.7 align-zbc-reader-writer-asymmetry made SIGS/TYPE carry
/// the authoritative string via str_idx. Reader no longer calls it on the
/// hot path; future linter / disasm tooling may.
#[allow(dead_code)]
fn type_tag_to_str(tag: u8) -> &'static str {
    match tag {
        0x01 => "bool",
        0x02 => "i8",
        0x03 => "i16",
        0x04 => "i32",
        0x05 => "i64",
        0x06 => "u8",
        0x07 => "u16",
        0x08 => "u32",
        0x09 => "u64",
        0x0A => "f32",
        0x0B => "f64",
        0x0C => "char",
        0x0D => "str",
        0x20 => "object",
        0x21 => "array",
        _    => "void",
    }
}

fn exec_mode_from_byte(b: u8) -> ExecMode {
    match b {
        1 => ExecMode::Jit,
        2 => ExecMode::Aot,
        _ => ExecMode::Interp,
    }
}

#[cfg(test)]
#[path = "zbc_reader_tests.rs"]
mod tests;
