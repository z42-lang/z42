use super::*;

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
// 2026-07-10 add-param-metadata (unify P1-d): bumped to 1.25 - SIGS gains min_arg:u16
// + params_from:u8 after method_flags; each param gains name_str_idx:u32 + default_kind:u8
// (+payload). Backs ParameterInfo.IsOptional/IsParams/Name/DefaultValue.
// 2026-07-11 add-delegate-metadata (unify P1-e): bumped to 1.26 - class_flags
// bit6=delegate (delegate-as-class TYPE entry + synthesized Invoke stub; no
// extra payload, semantics-extension bump per the 1.19 interface precedent).
// 2026-07-14 stabilize-dispatch-keys (方案A): bumped to 1.27 - no wire-layout
// change; dispatch keys now full-signature mangled for every method, so
// CallInstr/VCall operand strings + SIGS method names are globally re-keyed.
// The version bump exists to trigger ci-bootstrap's two-generation self-host
// (coupled with zpkg 0.32) which rebuilds the whole tree consistently.
// 2026-07-18 fix-crosspkg-interface-impl: bumped to 1.28 - interface TYPE
// entries gain a trailing method-signature block (CLASS_FLAG_INTERFACE gated:
// mcount:u16 + (name:u32, ret:u32, pcount:u8, ptype:u32×pc)×n) so the compiler
// can restore imported interface methods from dep zpkgs (abstract iface methods
// have no body → absent from SIGS; EXPT was dropped). Coupled with zpkg 0.33.
// 2026-08-04 add-escape-analysis-stack-alloc: bumped to 1.29 - ObjNew / ArrayNew
// / ArrayNewLit encodings each gain a trailing u8 stack-alloc flag (1 = frame
// arena / 0 = heap), set by the escape-analysis compiler pass. Coupled with zpkg 0.34.
// 2026-08-06 impl-sealed-semantics-devirt: bumped to 1.30 - SIGS method_flags:u8
// gains bit2=sealed (METHOD_FLAG_SEALED) for `sealed override` / `sealed` methods.
// Byte layout unchanged (bit was reserved-0); semantics-extension bump so a 1.29
// reader can't silently misread bit2 (strict-pin divergence guard). Backs
// MethodInfo.IsSealed + compiler sealed-receiver devirtualization. Coupled with zpkg 0.35.
//
// 2026-08-08 add-struct-value-semantics A-use: bumped to 1.31 — TYPE section gains
// a value-struct layout block (size + typed reference bitmap, Flags bit2 gated) and
// z42c begins emitting StructAlloc/Copy/FieldGet/SetPrim. Coupled with zpkg 0.36.
//
// 2026-08-11 add-struct-heap-inline P3b: bumped to 1.32 — TYPE section gains a
// **composed inline-struct layout block** (same shape as the struct block; the
// class's object-relative inline byte-region size + reference bitmap), present when
// class_flags & CLASS_FLAG_HAS_INLINE_STRUCT (bit7=0x80), following the struct block.
// Delivers `TypeDescCold.inline_layout` for `class C { Point pt; }`. Coupled with zpkg 0.37.
//
// 2026-08-13 enforce-class-access: bumped to 1.33 — TYPE section gains a **class
// declaration visibility byte** (0=public/1=private/2=protected/3=internal),
// immediately following the class_flags u8 (every TYPE record grows by 1 byte).
// The compiler's cross-package `internal`-class reference enforcement reads it; the
// VM currently reads-and-discards it (no class-visibility reflection surface yet).
// Coupled with zpkg 0.38.
//
// 2026-08-14 unify-object-byte-layout PR-1: bumped to 1.34 — TYPE section gains a
// **full object field layout block** for normal reference classes (not struct /
// interface / enum / delegate). Layout: object_size:u32 + field_count:u16 +
// (field_off:u32, field_size:u32, field_kind:u8)×n + ref_count:u16 +
// (ref_off:u32, ref_kind:u8)×m — every direct field's byte offset/size/kind at 8B
// reference width (C# endpoint) + flattened 8B reference bitmap. Follows the inline
// block, gated by a derived predicate (class flags U8 is full). Delivers
// `TypeDescCold.object_layout` (dormant; PR-2 consumes it to replace `slots`).
// Coupled with zpkg 0.39.
//
// 2026-08-14 unify-object-byte-layout PR-3 chunk 2a: bumped to 1.35 — TYPE object
// block **direct-field FieldKinds refinement**: the coarse `GcRef` (=2) is split into
// `GcRefArray` (=4, array `T[]`) and `GcRefClosure` (=5, delegate/func/unresolved) so
// the runtime (chunk 2b) can safely inline object/array refs as 8B pointers while
// keeping non-GcRef refs (Value::Closure/FuncRef) in the side-table. Only the object
// block's direct-field `field_kinds` bytes change; size/align/ref-bitmap/struct block
// are byte-identical. Runtime is dormant (compose_object_layout maps 4/5 to the coarse
// GcRef side-table path); chunk 2b consumes it. Coupled with zpkg 0.40. See design D17.
//
// 2026-08-21 add-generic-methods: bumped to 1.36 — method-level generic type_args.
// New opcodes: MethodTypeArg (0xB2), MethodDefault (0xB3), CallGeneric (0xB4),
// VCallGeneric (0xB5). Non-generic Call/VCall (0x50/0x52) encoding unchanged
// (byte-identical); generic calls carry a `method_type_args` string list (count u16
// + pool idx u32×) between the method token and args. Coupled with zpkg 0.41.
// 2026-09-02 fix-generic-array-value-zero-init (方案 C): bumped to 1.37 — ArrayNew
// encoding gains a trailing type-param reference: type_param_kind (u8: 0=none /
// 1=method-level / 2=class-level) + (type_param_index+1) (u16, biased so -1/none → 0),
// after the escape-analysis stack-alloc flag. When kind!=0 the VM resolves the
// generic element to a concrete type via frame.method_type_args / receiver.type_args
// and initialises value-type array slots with the type's zero (not Null). Non-generic
// ArrayNew emits kind=0/index=-1 (tail bytes 00 00 00) — semantics unchanged.
// ArrayNewLit is NOT changed (all slots literal-written, no null-tail bug). Coupled with zpkg 0.42.
pub const ZBC_VERSION_MINOR: u16 = 38;

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
// 2026-07-10 add-param-metadata: bumped to 0.29, coupled inner zbc 1.25.
// 2026-07-11 add-delegate-metadata: bumped to 0.30, coupled inner zbc 1.26.
// 2026-07-11 drop-tsig-expt (unify P3): bumped to 0.31 - zpkg top-level EXPT + TSIG sections
// removed (EXPT write-only; TSIG superseded by TsigReconcile.Rebuild from TYPE/SIGS). IMPL kept.
// 2026-07-14 stabilize-dispatch-keys (方案A): bumped to 0.32, coupled inner zbc 1.27.
// Dispatch keys are now a pure function of each method's own signature (always
// full-signature mangled; protocol-exempt names stay bare), so exported method
// names / SIGS / embedded zbc CallInstr operands are globally re-keyed. Outer
// zpkg layout unchanged. The bump triggers ci-bootstrap's version-diff gate →
// two-generation self-host rebuilds the whole tree onto the stable keys.
// 2026-07-18 fix-crosspkg-interface-impl: bumped to 0.33, coupled inner zbc 1.28
// (interface TYPE entries gain a method-signature block; see zbc changelog).
// Outer zpkg layout unchanged.
// add-offline-symbolication (2026-08-04): the SymOnly (.zsym) sidecar MDBG layout
// changed WITHIN minor 33 (per-module now carries `funcCount + per-func
// frameName_idx[]` before the dbug blob so `.zsym` self-maps func-name → line
// table for offline `z42d symbolicate`). NOT a minor bump: MDBG lives only in the
// ephemeral, co-versioned `.zsym` (regenerated every release build, not a
// distributed stable artifact); writer + this reader land together; regular zpkg
// bytes are unchanged. read_mdbg_section skips the frame-name idxs (runtime merges
// sidecar line tables by index, names come from the main zpkg).
// 2026-08-04 add-escape-analysis-stack-alloc: bumped to 0.34, coupled inner zbc 1.29
// (ObjNew/ArrayNew/ArrayNewLit gain a trailing u8 stack-alloc flag). Outer zpkg
// layout unchanged; the bump triggers ci-bootstrap's version-diff two-gen self-host.
// 2026-08-06 impl-sealed-semantics-devirt: bumped to 0.35, coupled inner zbc 1.30
// (SIGS method_flags bit2=sealed). Outer zpkg layout unchanged; the bump triggers
// ci-bootstrap's version-diff two-gen self-host.
//
// 2026-08-08 add-struct-value-semantics A-use: bumped to 0.36, coupled inner zbc
// 1.31 (TYPE value-struct layout block + blob value-type instruction emission).
// 2026-08-11 add-struct-heap-inline P3b: bumped to 0.37, coupled inner zbc 1.32
// (TYPE composed inline-struct layout block, Flags bit7 gated). Outer zpkg layout
// unchanged; the bump triggers ci-bootstrap's version-diff two-gen self-host.
// 2026-08-13 enforce-class-access: bumped to 0.38, coupled inner zbc 1.33 (TYPE
// class-declaration visibility byte). Outer zpkg layout unchanged; the bump triggers
// ci-bootstrap's version-diff two-gen self-host.
// 2026-08-14 unify-object-byte-layout PR-1: bumped to 0.39, coupled inner zbc 1.34
// (TYPE full object field layout block for normal reference classes). Outer zpkg
// layout unchanged; the bump triggers ci-bootstrap's version-diff two-gen self-host.
// 2026-08-14 unify-object-byte-layout PR-3 chunk 2a: bumped to 0.40, coupled inner zbc
// 1.35 (TYPE object-block direct-field FieldKinds refinement: coarse GcRef split into
// GcRefArray/GcRefClosure so the runtime can safely inline object/array refs in chunk
// 2b). Outer zpkg layout unchanged; the bump triggers ci-bootstrap's version-diff
// two-gen self-host.
// 2026-08-21 add-generic-methods: bumped to 0.41, coupled inner zbc 1.36 (method-level
// generic type_args new opcodes; non-generic calls byte-identical). Outer zpkg layout
// unchanged; the bump triggers ci-bootstrap's version-diff two-gen self-host.
// 2026-09-02 fix-generic-array-value-zero-init: bumped to 0.42, coupled inner zbc 1.37
// (ArrayNew trailing type-param reference for generic value-type zero-init). Outer zpkg
// layout unchanged; the bump triggers ci-bootstrap's version-diff two-gen self-host.
pub const ZPKG_VERSION_MINOR: u16 = 43;

// ── Strict-pin header verification ────────────────────────────────────────────
//
// The reader accepts exactly one (major, minor) — the pinned writer version.
// These helpers centralize the magic + exact major/minor guard that was formerly
// copy-pasted across the full-mode readers. Error strings are byte-identical to
// the former inline checks (asserted by the `*_rejects_*` tests in
// zbc_reader_tests.rs); callers still read `flags` / `sec_count` + any SymOnly
// handling themselves, since those diverge per reader.

/// Reject truncated data, wrong magic, or any zbc major/minor ≠ the pinned writer
/// version. Used by [`super::read_zbc`]; the leaner `read_test_index_section` /
/// `read_raw_string_pool` paths intentionally do only a magic check (optional
/// sections), so they do not route through here.
pub(super) fn verify_zbc_version(data: &[u8]) -> Result<()> {
    if data.len() < 16 { bail!("zbc file too short") }
    if &data[0..4] != ZBC_MAGIC { bail!("not a binary zbc (bad magic)") }
    let major = u16::from_le_bytes([data[4], data[5]]);
    let minor = u16::from_le_bytes([data[6], data[7]]);
    // Strict-pin policy (freeze-zbc-v1, 2026-05-14): exact match with writer.
    // Pre-1.0 z42 doesn't keep older zbc minor readable; regen via xtask regen.
    if major != ZBC_VERSION_MAJOR {
        bail!("zbc major {major} not supported (writer is at {ZBC_VERSION_MAJOR})");
    }
    if minor != ZBC_VERSION_MINOR {
        bail!(
            "zbc minor {minor} not supported (writer is at {ZBC_VERSION_MINOR}); \
             regen via xtask regen"
        );
    }
    Ok(())
}

/// Reject truncated data, wrong magic, or any zpkg major/minor ≠ the pinned
/// writer version. Shared by the full-mode readers `read_zpkg_meta` /
/// `read_zpkg_modules` / `read_zpkg_file_entries` (each still reads `flags` /
/// `sec_count` + its own SymOnly handling afterward). The magic-only readers
/// (`read_zpkg_impl_pairs` / `read_zpkg_namespaces`) intentionally do not use it.
pub(super) fn verify_zpkg_version(data: &[u8]) -> Result<()> {
    if data.len() < 16 { bail!("zpkg file too short") }
    if &data[0..4] != ZPKG_MAGIC { bail!("not a binary zpkg (bad magic)") }
    let major = u16::from_le_bytes([data[4], data[5]]);
    let minor = u16::from_le_bytes([data[6], data[7]]);
    // Strict-pin policy (freeze-zpkg-v0, 2026-05-14): exact match with writer.
    // Pre-1.0 z42 doesn't keep older zpkg minor readable; regen via
    // xtask build stdlib.
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
    Ok(())
}
