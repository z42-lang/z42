//! 对象 / struct 字节布局：StructTypeLayout / ObjectLayout / InlineRef / compose·synthesize。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// Cold side-table for `TypeDesc`. Holds inheritance fixup inputs +
/// generics metadata. Touched only by loader fixup, reflection /
/// `DefaultOf` opcode, and constraint verification — never by hot
/// dispatch.
/// add-struct-value-semantics: reference-leaf kind in a value-struct's reference
/// bitmap (mirrors the compiler's `StructLeafKind` for the two reference kinds;
/// primitive leaves are never listed). Both are 16 B managed handles; the kind is
/// retained so boxing / diagnostics can recover the precise kind. Copy and GC
/// scan treat all reference leaves uniformly via `Value`, so the value logic does
/// not branch on kind.
pub const STRUCT_REF_ARC_STRING: u8 = 1;
pub const STRUCT_REF_GCREF: u8 = 2;

/// add-struct-value-semantics: runtime byte + reference layout of a value-struct
/// type, delivered by the zbc TYPE-section struct block (A-use). `size` = the
/// byte-blob size; `ref_offsets` / `ref_kinds` = the byte offset + kind of each
/// reference leaf (parallel arrays, bitmap order). Pure-primitive structs have
/// empty reference arrays. A type with no delivered layout resolves (in
/// `interp::exec_struct::resolve_layout`) to a `size`-only empty layout, which
/// reproduces the pre-A-use pure-primitive behavior byte-for-byte.
#[derive(Debug, Default)]
pub struct StructTypeLayout {
    pub size: usize,
    pub ref_offsets: Box<[u32]>,
    pub ref_kinds: Box<[u8]>,
}

impl StructTypeLayout {
    /// Map a reference-leaf byte offset to its index in `ref_offsets` (and thus a
    /// blob's `refs` side-slice). Linear scan — reference leaves per struct are few.
    #[inline]
    pub fn ref_index(&self, byte_off: u32) -> Option<usize> {
        self.ref_offsets.iter().position(|&o| o == byte_off)
    }

    /// Number of reference leaves (= a blob's `refs` length for this type).
    #[inline]
    pub fn ref_count(&self) -> usize {
        self.ref_offsets.len()
    }
}

/// unify-object-byte-layout (PR-2, D12): resolved per-field access descriptor for a
/// direct field of a reference class — the hot-path form consumed by `FieldGet`/
/// `FieldSet`. Precomputed at load time (one array-index per access, no per-access
/// string match). Parallel by index with `TypeDesc::fields` / `field_index` slot.
///
/// - `offset` / `width` = the field's byte window in `ScriptObject::bytes`.
/// - `tag` = the **exact** `ty::TAG_*` recovered from the field's declared
///   `type_tag` string (via `tag_from_name`) — `field_kinds` (coarse `StructLeafKind`)
///   can't drive `decode_prim`, so the precise tag comes from the type string, the
///   same source `default_value_for` uses. `TAG_UNKNOWN` for struct-typed roots
///   (never reached by `FieldGet`; accessed via `StructFieldGetPrim`).
/// - `ref_slot` = index into `ScriptObject::refs` if this is a reference field, else
///   `-1` (primitive stored inline in `bytes`; PR-3 will inline the 8B pointer here).
#[derive(Debug, Clone, Copy, Default)]
pub struct FieldAccess {
    pub offset: u32,
    pub width: u32,
    pub tag: u8,
    pub ref_slot: i32,
}

/// unify-object-byte-layout (PR-2): map a field's declared `type_tag` string to its
/// exact `ty::TAG_*`. Mirrors `corelib::struct_reflect::tag_from_name` (kept here so
/// the loader can build the access table without a corelib dependency). Post-unify
/// canonical primitive names (`int`/`i32`, `long`/`i64`, …) and struct/ref types.
pub fn tag_from_type_name(t: &str) -> u8 {
    match t {
        "void" => TAG_UNKNOWN,
        "bool" => TAG_BOOL,
        "i8" | "sbyte" => TAG_I8,
        "i16" | "short" => TAG_I16,
        "i32" | "int" => TAG_I32,
        "i64" | "long" => TAG_I64,
        "u8" | "byte" => TAG_U8,
        "u16" | "ushort" => TAG_U16,
        "u32" | "uint" => TAG_U32,
        "u64" | "ulong" => TAG_U64,
        "f32" | "float" => TAG_F32,
        "f64" | "double" => TAG_F64,
        "char" => TAG_CHAR,
        "str" | "string" => TAG_STR,
        _ => TAG_OBJECT,
    }
}

/// unify-object-byte-layout (PR-2): the **composed** full-object byte layout of a
/// reference class's instances — the runtime form of the dormant zbc `object_layout`
/// (own-only) after inheritance composition (`base.composed ++ own`). Built at load
/// time (`compose_object_layout`) mirroring `merge_with_base`'s `fields = base.fields
/// ++ own_fields`, so `field_offsets[i]` aligns by index with `TypeDesc::fields[i]`.
///
/// - `size` = total object byte-region size (references at 8B here — the C#-equivalent
///   endpoint; PR-2 still stores references 16B in a `refs` side-table, so the 8B field
///   window is a dead hole until PR-3 inlines the pointer).
/// - `field_offsets` / `field_sizes` / `field_kinds` = per **merged** field the byte
///   offset / size / kind (`STRUCT_REF_*` for references, else primitive/struct), in
///   merged-field-index order (base fields first, then own).
/// - `ref_offsets` / `ref_kinds` = flattened composed reference bitmap (base leaves
///   first, then own leaves shifted by the base region size), including inline-struct
///   interior reference leaves. `ref_index(off)` maps a byte offset to its `refs`
///   side-table slot, same shape as `StructTypeLayout::ref_index`.
///
/// **Dormant in task 2.0**: composed here and unit-tested, but not yet consumed
/// (`ScriptObject` still uses `slots`); task 2.1+ switches field storage onto it.
#[derive(Debug, Default)]
pub struct ObjectLayout {
    pub size: usize,
    pub field_offsets: Box<[u32]>,
    pub field_sizes: Box<[u32]>,
    pub field_kinds: Box<[u8]>,
    /// **Side-table** reference bitmap: byte offsets of the reference leaves that
    /// live in `ScriptObject::refs` (not byte-inlined). Before PR-3 chunk 2b this
    /// was *every* reference leaf; chunk 2b removes the direct object/array leaves
    /// (they inline into `bytes`, see `inline_refs`), leaving only closure/func/
    /// string direct fields + every inline-struct interior reference leaf.
    pub ref_offsets: Box<[u32]>,
    pub ref_kinds: Box<[u8]>,
    /// unify-object-byte-layout (PR-3 chunk 2b): the direct **object/array** reference
    /// fields inlined as an 8B tagged pointer in `bytes` (removed from the `refs`
    /// side-table). GC scans these by reading the 8B window at each `offset` and
    /// rebuilding the right `Value` variant (`Value::Array` when `is_array`, else
    /// `Value::Object`). `0` at the window = an empty (`Null`) slot. Empty for
    /// synthesized/fallback layouts (which conservatively keep all refs in the
    /// side-table — no authoritative `field_kinds` to tell object from closure).
    pub inline_refs: Box<[InlineRef]>,
    /// unify-object-byte-layout (PR-2, D12): resolved per-field access table (offset /
    /// width / exact tag / refs-slot), parallel by index with `TypeDesc::fields`.
    /// Filled at load time from the composed offsets + the merged fields' `type_tag`
    /// strings. Empty when composed without field info (e.g. `compose_object_layout`
    /// called with `&[]` in unit tests that only check structural offsets).
    pub field_access: Box<[FieldAccess]>,
}

/// unify-object-byte-layout (PR-3 chunk 2b): a direct reference field byte-inlined
/// into `ScriptObject::bytes` as an 8B tagged `GcRef` pointer. `is_array` selects the
/// reconstructed `Value` variant (`Value::Array` vs `Value::Object`) — the raw 8B
/// pointer carries no object-vs-array discriminant, so the kind must come from the
/// compiler's `field_kinds` (`STRUCT_LEAF_GCREF` → object, `_GCREF_ARRAY` → array).
#[derive(Debug, Clone, Copy)]
pub struct InlineRef {
    pub offset: u32,
    pub is_array: bool,
}

// unify-object-byte-layout (PR-2): the compiler's `StructLeafKind` values carried in
// `ObjectLayoutDesc::field_kinds` (mirror `StructLayout.StructLeafKind`). These are the
// **authoritative** ref/prim/struct classification for a direct field (they come from
// the compiler's type resolution, unlike the field's `type_tag` string which may be an
// unresolved alias like `using Id = int`). Struct roots are accessed via
// `StructFieldGetPrim`, never `FieldGet`.
pub const STRUCT_LEAF_PRIM: u8 = 0;
pub const STRUCT_LEAF_ARCSTRING: u8 = 1;
pub const STRUCT_LEAF_GCREF: u8 = 2;
pub const STRUCT_LEAF_STRUCT: u8 = 3;
/// unify-object-byte-layout (PR-3 chunk 2a): refined direct-field ref kinds emitted by the
/// compiler's object block (`StructLayout._refineDirectRefKind`) — split the coarse `GcRef`
/// so the runtime (chunk 2b) can safely inline object/array references as 8B pointers while
/// keeping non-`GcRef` refs (delegate/func → `Value::Closure`/`FuncRef`) in the side-table.
/// **Dormant in 2a**: `compose_object_layout` treats all three as a side-table `GcRef`, so
/// runtime behavior is unchanged; chunk 2b flips these to drive inlining. See design D17.
pub const STRUCT_LEAF_GCREF_ARRAY: u8 = 4;   // array `T[]` → `Value::Array` (inline-able, chunk 2b)
pub const STRUCT_LEAF_GCREF_CLOSURE: u8 = 5; // delegate/func/opaque → `Value::Closure`/`FuncRef` (never inline)

/// unify-object-byte-layout (PR-2, D12): resolve a **primitive** field's exact
/// `ty::TAG_*` from its declared `type_tag` string, with a width-based fallback for
/// names `tag_from_type_name` doesn't recognize — user type aliases (`using Id = int`)
/// and FQ spellings leak through `type_tag` unresolved, but the compiler's
/// `field_sizes` (width) is always correct. Called only for fields the compiler
/// classified as `StructLeafKind.Prim`, so the type IS some primitive; width picks the
/// integer tag when the name is opaque. **Limitation**: an opaque alias of a *float*
/// (`using Real = double`) or *char* falls back to a same-width integer tag — a rare
/// edge; the definitive fix (exact tags in the object block) is deferred (would need a
/// zbc format bump). Recognized names (`int`/`i32`/`f64`/…) always resolve exactly.
pub fn resolve_prim_tag(type_tag: &str, width: u32) -> u8 {
    let t = tag_from_type_name(type_tag);
    if is_prim_tag(t) {
        return t;
    }
    // Opaque name (alias / FQ): pick a same-width signed-integer tag.
    match width {
        1 => TAG_I8,
        2 => TAG_I16,
        4 => TAG_I32,
        _ => TAG_I64,
    }
}

/// Whether `tag` is a scalar primitive tag (bool / int widths / floats / char) —
/// i.e. `decode_prim`/`encode_prim` can handle it. Excludes ref/unknown tags.
#[inline]
pub fn is_prim_tag(tag: u8) -> bool {
    matches!(tag,
        TAG_BOOL | TAG_I8 | TAG_I16 | TAG_I32 | TAG_I64
      | TAG_U8 | TAG_U16 | TAG_U32 | TAG_U64
      | TAG_F32 | TAG_F64 | TAG_CHAR)
}

impl ObjectLayout {
    /// Map a reference-leaf byte offset to its index in the composed reference
    /// bitmap (and thus the object's `refs` side-table slot). Linear scan —
    /// reference leaves per object are few.
    #[inline]
    pub fn ref_index(&self, byte_off: u32) -> Option<usize> {
        self.ref_offsets.iter().position(|&o| o == byte_off)
    }

    /// Number of reference leaves (= the object's `refs` side-table length).
    #[inline]
    pub fn ref_count(&self) -> usize {
        self.ref_offsets.len()
    }
}

/// unify-object-byte-layout (PR-2): compose a class's **own-only** `ObjectLayoutDesc`
/// (from the zbc 1.34 object block, offsets from 0) with its base class's already-
/// composed `ObjectLayout` into the merged runtime layout, mirroring
/// `merge_with_base`'s `fields = base.fields ++ own`. The own region begins at
/// `align_up(base.size, 8)` — the unified 8B inheritance boundary (matches the
/// compiler's independent base-shift when it bakes inline-struct leaf offsets, D9);
/// both must agree byte-for-byte, backstopped by self-host byte-identity.
///
/// `base` is `None` for a root class (or a cross-zpkg base not yet resolved — the
/// fixup pass recomposes once it resolves). Field/reference arrays are simple
/// concatenations with the own side shifted by `base_shift`.
///
/// `merged_fields` = the class's full merged field list (`base.fields ++ own`, same
/// order as the composed offsets); used to build the per-field access table
/// (`FieldAccess`) — the exact `ty::TAG` from each field's `type_tag` string (D12).
/// Pass `&[]` to skip the access table (structural-only, e.g. offset unit tests).
pub fn compose_object_layout(
    base: Option<&ObjectLayout>,
    own: &crate::metadata::bytecode::ObjectLayoutDesc,
    merged_fields: &[FieldSlot],
) -> ObjectLayout {
    // Unified 8B inheritance boundary: the own region starts after the base region,
    // rounded up to 8 so references (8B, 8-aligned) land aligned.
    let base_shift: u32 = match base {
        Some(b) => ((b.size as u32) + 7) & !7,
        None    => 0,
    };

    let base_fields = base.map_or(0, |b| b.field_offsets.len());
    let base_refs   = base.map_or(0, |b| b.ref_offsets.len());

    let mut field_offsets = Vec::with_capacity(base_fields + own.field_offsets.len());
    let mut field_sizes   = Vec::with_capacity(base_fields + own.field_sizes.len());
    let mut field_kinds   = Vec::with_capacity(base_fields + own.field_kinds.len());
    // Side-table ref bitmap (closure/func/string direct fields + inline-struct interior
    // leaves) — the inlined direct object/array leaves are pulled out into `inline_refs`.
    let mut ref_offsets   = Vec::with_capacity(base_refs + own.ref_offsets.len());
    let mut ref_kinds     = Vec::with_capacity(base_refs + own.ref_kinds.len());
    let mut inline_refs   = Vec::new();

    if let Some(b) = base {
        field_offsets.extend_from_slice(&b.field_offsets);
        field_sizes.extend_from_slice(&b.field_sizes);
        field_kinds.extend_from_slice(&b.field_kinds);
        // The base is already partitioned (composed with chunk 2b): its `ref_offsets`
        // is the side-table, its `inline_refs` the byte-inlined object/array fields.
        ref_offsets.extend_from_slice(&b.ref_offsets);
        ref_kinds.extend_from_slice(&b.ref_kinds);
        inline_refs.extend_from_slice(&b.inline_refs);
    }

    for &off in own.field_offsets.iter() { field_offsets.push(off + base_shift); }
    field_sizes.extend_from_slice(&own.field_sizes);
    field_kinds.extend_from_slice(&own.field_kinds);

    // PR-3 chunk 2b: partition the OWN reference bitmap into inline (direct object/array
    // fields, authoritative `field_kinds` says `GCREF`/`GCREF_ARRAY`) vs side-table
    // (everything else — closure/func/string direct fields + inline-struct interior
    // leaves). A direct field's ref leaf sits at exactly the field's byte offset, so
    // matching `own.ref_offsets` against `own.field_offsets`+kind is an exact key lookup.
    let own_inline_at = |off: u32| -> Option<bool> {
        own.field_offsets.iter().zip(own.field_kinds.iter())
            .find(|(&o, _)| o == off)
            .and_then(|(_, &k)| match k {
                STRUCT_LEAF_GCREF       => Some(false), // object/interface → Value::Object
                STRUCT_LEAF_GCREF_ARRAY => Some(true),  // array `T[]`       → Value::Array
                _ => None, // closure/func/string/prim/struct → side-table (or not a ref)
            })
    };
    for (&off, &rk) in own.ref_offsets.iter().zip(own.ref_kinds.iter()) {
        match own_inline_at(off) {
            Some(is_array) => inline_refs.push(InlineRef { offset: off + base_shift, is_array }),
            None => { ref_offsets.push(off + base_shift); ref_kinds.push(rk); }
        }
    }

    // D12: resolve the per-field access table from composed offsets + each field's
    // declared `type_tag`. chunk 2b: object/array fields are **inlined** (ref_slot = -1,
    // tag = TAG_OBJECT/TAG_ARRAY → `field_value` reads the 8B pointer from `bytes`);
    // closure/func/string stay in the side-table (ref_slot ≥ 0). Skipped when no field
    // info given (structural-only unit tests).
    let field_access: Box<[FieldAccess]> = if merged_fields.is_empty() {
        Box::new([])
    } else {
        let mut acc = Vec::with_capacity(field_offsets.len());
        for i in 0..field_offsets.len() {
            let off = field_offsets[i];
            let width = field_sizes.get(i).copied().unwrap_or(0);
            // Classify from the compiler's authoritative `field_kinds` (StructLeafKind),
            // not the field's `type_tag` string (which may be an unresolved alias).
            let kind = field_kinds.get(i).copied().unwrap_or(STRUCT_LEAF_PRIM);
            let type_tag = merged_fields.get(i).map(|f| f.type_tag.as_ref());
            // Side-table slot = position in the (already partitioned) side-table bitmap.
            let ref_slot_of = |off: u32| -> i32 {
                ref_offsets.iter().position(|&o| o == off).map_or(-1, |ri| ri as i32)
            };
            let (tag, ref_slot) = match kind {
                STRUCT_LEAF_STRUCT => (TAG_UNKNOWN, -1),
                STRUCT_LEAF_ARCSTRING => (TAG_STR, ref_slot_of(off)),
                // Inlined direct references (8B pointer in `bytes`, no side-table slot).
                STRUCT_LEAF_GCREF       => (TAG_OBJECT, -1),
                STRUCT_LEAF_GCREF_ARRAY => (TAG_ARRAY,  -1),
                // Non-`GcRef` reference (delegate/func → `Value::Closure`/`FuncRef`): can't
                // be a raw 8B pointer → stays in the side-table.
                STRUCT_LEAF_GCREF_CLOSURE => (TAG_OBJECT, ref_slot_of(off)),
                // Prim (or unknown kind): resolve the exact primitive tag.
                _ => (resolve_prim_tag(type_tag.unwrap_or(""), width), -1),
            };
            acc.push(FieldAccess { offset: off, width, tag, ref_slot });
        }
        acc.into()
    };

    ObjectLayout {
        size: (base_shift + own.size) as usize,
        field_offsets: field_offsets.into(),
        field_sizes:   field_sizes.into(),
        field_kinds:   field_kinds.into(),
        ref_offsets:   ref_offsets.into(),
        ref_kinds:     ref_kinds.into(),
        inline_refs:   inline_refs.into(),
        field_access,
    }
}

/// unify-object-byte-layout (PR-2): synthesize a composed `ObjectLayout` directly from
/// a class's merged `fields` — the fallback for a normal reference class that carries
/// **no** zbc object block (synthetic / fallback / Rust-constructed types; every class
/// compiled at zbc ≥ 1.34 delivers a real block instead). Packs each field at its
/// natural alignment: primitives get their `prim_width` byte window; references (and
/// any non-primitive type name — a struct field never occurs in a layout-less type)
/// get an 8B slot + a `refs` side-table entry. Internally consistent (all of
/// `field_value` / `set_field_value` / GC read the same table); never cross-checked
/// against compiler output, so its exact packing only needs to be self-consistent.
pub fn synthesize_object_layout(fields: &[FieldSlot]) -> ObjectLayout {
    let mut cursor: u32 = 0;
    let mut field_offsets = Vec::with_capacity(fields.len());
    let mut field_sizes   = Vec::with_capacity(fields.len());
    let mut field_kinds   = Vec::with_capacity(fields.len());
    let mut field_access  = Vec::with_capacity(fields.len());
    let mut ref_offsets   = Vec::new();
    let mut ref_kinds     = Vec::new();
    for f in fields {
        let tag = tag_from_type_name(&f.type_tag);
        let is_ref = is_ref_tag(tag);
        let width: u32 = if is_ref { 8 } else { prim_width(tag).unwrap_or(8) as u32 };
        let align = width.max(1);
        let off = (cursor + (align - 1)) & !(align - 1);
        cursor = off + width;
        field_offsets.push(off);
        field_sizes.push(width);
        let ref_slot = if is_ref {
            let ri = ref_offsets.len() as i32;
            ref_offsets.push(off);
            // Distinguish arc-string vs gcref for GC precision (STRUCT_REF_*).
            ref_kinds.push(if tag == TAG_STR { STRUCT_REF_ARC_STRING } else { STRUCT_REF_GCREF });
            field_kinds.push(if tag == TAG_STR { 1u8 } else { 2u8 }); // StructLeafKind ArcString/GcRef
            ri
        } else {
            field_kinds.push(0u8); // StructLeafKind.Prim
            -1
        };
        field_access.push(FieldAccess { offset: off, width, tag, ref_slot });
    }
    let size = ((cursor + 7) & !7) as usize;
    ObjectLayout {
        size,
        field_offsets: field_offsets.into(),
        field_sizes:   field_sizes.into(),
        field_kinds:   field_kinds.into(),
        ref_offsets:   ref_offsets.into(),
        ref_kinds:     ref_kinds.into(),
        // Synthesized layouts conservatively keep every reference in the side-table:
        // without authoritative `field_kinds` we can't tell an object (inline-able) from
        // a delegate/func (`Value::Closure`, never inline-able → reading bytes = UB).
        inline_refs:   Box::new([]),
        field_access:  field_access.into(),
    }
}
