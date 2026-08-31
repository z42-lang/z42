use std::sync::Arc;
use crate::metadata::vstr::Str;   // unify-object-byte-layout PR-4: 8B thin string handle

use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

// ── TypeDesc — runtime type descriptor ──────────────────────────────────────
//
// Equivalent to CoreCLR's MethodTable: pre-built at module load time,
// shared across all instances of a class via Arc.

/// A single field slot in a class layout (runtime representation).
///
/// review.md E2.P2 Step 1 (2026-05-27): `Box<str>` (16 B per field) instead
/// of `String` (24 B; the `cap` word is dead weight — slot fields are
/// immutable after `build_type_registry`). Saves 16 B per FieldSlot
/// (48 B → 32 B). Full E2.P2 target (48 B → 16 B with `name_id: StringId`
/// + `type_id: TypeId` + `offset` + `flags`) waits on StringId Phase B+
/// migration and a zbc minor bump.
#[derive(Debug, Clone)]
pub struct FieldSlot {
    pub name: Box<str>,
    /// Type tag from zbc (e.g. `"int"`, `"long"`, `"bool"`, `"f64"`, `"str"`,
    /// `"Demo.Box"`, …). Used by `ObjNew` to pick a per-type default `Value`
    /// for fields that have no explicit initializer.
    /// 2026-05-02 fix-class-field-default-init.
    pub type_tag: Box<str>,
    /// Member visibility (add-member-visibility, unify P1-b): 0=public /
    /// 1=private / 2=protected. Carried from the TYPE section's per-field
    /// `visibility:u8` so `FieldInfo.IsPublic` can report it via reflection.
    /// Defaults to 0 (public) for synthesized slots (gc / exception / tests).
    pub visibility: u8,
}

/// Returns the default `Value` for a field whose declared type tag is
/// `type_tag`. Mirrors the C# `EmitStaticInit` defaults. Used by `ObjNew`
/// (interp + JIT) to initialise fields without an explicit initializer.
///
/// Reference / unknown types fall back to `Null`. `char` follows the existing
/// "char-as-i64" representation (no separate `Value::Char` variant).
///
/// Three primitive vocabularies reach here: field-slot keywords (`int`), function
/// signature tags (`i32`), and — since fix-type-reflection-names — the FQ wrapper
/// names (`Std.Int32`), which reflective `MakeGenericMethod(typeof(int)).Invoke`
/// threads into `method_type_args` (the resolved Type arg's handle name). All three
/// must yield the same value-type zero so reflective `default(T)` matches a direct
/// `Zero<int>()` call.
pub fn default_value_for(type_tag: &str) -> Value {
    match type_tag {
        "int" | "long" | "short" | "byte" | "sbyte" | "ushort" | "uint" | "ulong"
        | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        | "isize" | "usize"
        | "Std.Int32" | "Std.Int64" | "Std.Int16" | "Std.SByte" | "Std.Byte"
        | "Std.UInt16" | "Std.UInt32" | "Std.UInt64" => Value::I64(0),
        "double" | "float" | "f32" | "f64" | "Std.Double" | "Std.Single" => Value::F64(0.0),
        "bool" | "Std.Boolean" => Value::Bool(false),
        "char" | "Std.Char" => Value::Char('\0'),
        _ => Value::Null,
    }
}

// ── zbc TypeTag bytes (mirror of C# Opcodes.TypeTags) ────────────────────────
//
// Single source of truth for the 1-byte type tag carried in instruction
// headers / extra fields. Keep these in sync with
// src/compiler/z42.IR/BinaryFormat/Opcodes.cs `TypeTags`.

pub const TAG_UNKNOWN: u8 = 0x00;
pub const TAG_BOOL:    u8 = 0x01;
pub const TAG_I8:      u8 = 0x02;
pub const TAG_I16:     u8 = 0x03;
pub const TAG_I32:     u8 = 0x04;
pub const TAG_I64:     u8 = 0x05;
pub const TAG_U8:      u8 = 0x06;
pub const TAG_U16:     u8 = 0x07;
pub const TAG_U32:     u8 = 0x08;
pub const TAG_U64:     u8 = 0x09;
pub const TAG_F32:     u8 = 0x0A;
pub const TAG_F64:     u8 = 0x0B;
pub const TAG_CHAR:    u8 = 0x0C;
pub const TAG_STR:     u8 = 0x0D;
pub const TAG_OBJECT:  u8 = 0x20;
pub const TAG_ARRAY:   u8 = 0x21;

/// Returns the default `Value` for a slot whose declared element type tag
/// is `tag`. Mirrors `default_value_for(&str)` but keyed on the wire byte
/// directly (no string lookup). Used by `ArrayNew` (interp + JIT) to
/// initialise array elements without an explicit literal.
///
/// fix-array-default-init, 2026-05-18.
pub fn default_value_for_tag(tag: u8) -> Value {
    match tag {
        TAG_BOOL => Value::Bool(false),
        TAG_I8 | TAG_I16 | TAG_I32 | TAG_I64
      | TAG_U8 | TAG_U16 | TAG_U32 | TAG_U64 => Value::I64(0),
        TAG_F32 | TAG_F64 => Value::F64(0.0),
        TAG_CHAR => Value::Char('\0'),
        _ => Value::Null,
    }
}

/// Pre-computed runtime type descriptor (CoreCLR MethodTable equivalent).
///
/// Built once per class at module load time; instances reference it via `Arc`.
/// Includes the flattened inheritance chain for both fields and virtual methods.
#[derive(Debug, Clone)]
pub struct TypeDesc {
    /// Fully-qualified class name (e.g. `"Demo.Point"`).
    pub name: String,
    /// Runtime token assigned by `metadata::resolver::resolve_module` (introduce-method-token,
    /// 2026-05-08). Stable for the lifetime of the loaded module; used by VCallIC / FieldIC
    /// for receiver-type comparison without name hash. Default `TypeId::UNRESOLVED` until
    /// resolver runs (back-compat — pre-resolver code doesn't depend on this).
    pub id: super::tokens::TypeId,
    /// Fully-qualified base class name, if any.
    pub base_name: Option<String>,
    /// Field slots in order (base fields first, then derived).
    ///
    /// **Cross-zpkg subclass note** (fix-cross-pkg-subclass-fields, 2026-05-14):
    /// `build_type_registry` populates this with base fields from the local
    /// module's registry only — cross-zpkg base classes contribute nothing
    /// until [`crate::metadata::loader::try_fixup_inheritance`] runs at
    /// lazy-load time, which rebuilds this vector to include inherited slots.
    pub fields: Vec<FieldSlot>,
    /// `field_name → slot index` — linear scan (review.md C4 P1, 2026-06-01:
    /// `NameIndex` replaces `HashMap<String, usize>` because typical class
    /// field counts ≤16, where `Vec<(Box<str>, usize)>` scan beats hash +
    /// string compare). Same cross-zpkg fixup semantics as `fields`.
    pub field_index: super::name_index::NameIndex,
    /// Virtual method table: slot → (simple_method_name, qualified_func_name).
    /// Derived class overrides replace base entries at the same slot index.
    /// Same cross-zpkg fixup semantics as `fields`.
    pub vtable: Vec<(String, String)>,
    /// `method_name → vtable slot index` — linear scan (review.md C5 P1,
    /// 2026-06-01). Same rationale as `field_index`.
    pub vtable_index: super::name_index::NameIndex,
    /// add-reflection-type-flags (zbc 1.12): class-shape flags byte
    /// (`bytecode::CLASS_FLAG_*` — abstract/sealed/struct/record). Reflection
    /// only; backs `Type.IsAbstract` / `Type.IsSealed`. A single byte kept hot
    /// (fits existing padding) rather than in the cold box.
    pub class_flags: u8,
    /// complete-class-access-control: class-declaration visibility byte
    /// (0=public / 1=private / 2=protected / 3=internal). Reflection only; backs
    /// `Type.IsPublic` / `IsNotPublic` / `IsNested{Public,Private,Family,Assembly}`.
    pub visibility: u8,
    /// review.md E2.P1 Step 1 (2026-05-27): five rarely-accessed fields
    /// (own_fields / own_methods / type_params / type_args /
    /// type_param_constraints) live behind an `Option<Box<TypeDescCold>>`.
    /// Hot path (FieldGet IC miss → `field_index`; VCall miss →
    /// `vtable_index`; subclass walk → `base_name`; instance ops →
    /// `fields`) never touches the cold box. Saves 5 × 16 B → 8 B
    /// (Option-niche on Box) ≈ 72 B per non-generic non-inheriting
    /// TypeDesc. Cold box allocated lazily by `cold_mut()` (loader fixup
    /// and tests) and freed when TypeDesc drops.
    ///
    /// Full E2.P1 target (hot 64 B via StringId / TypeId / MethodId
    /// migration + cold further packed) waits on StringId Phase B+.
    pub cold: Option<Box<TypeDescCold>>,
}

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
    own: &super::bytecode::ObjectLayoutDesc,
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

/// unify-object-byte-layout (PR-3 chunk 2b): read a byte-inlined direct object/array
/// reference — the 8B tagged `GcRef` pointer at `off` in an object's `bytes` — back into
/// a `Value`. `0` (the zero-initialized default / an explicit `Null` store) → `Value::Null`.
/// `is_array` picks the variant (`Value::Array` vs `Value::Object`); the raw pointer has
/// no object-vs-array discriminant, so the kind must come from the layout's `field_kinds`.
#[inline]
pub(crate) fn read_inline_ref(bytes: &[u8], off: usize, is_array: bool) -> Value {
    if off + 8 > bytes.len() {
        return Value::Null;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(&bytes[off..off + 8]);
    let bits = u64::from_le_bytes(b);
    if is_array {
        // SAFETY: `bits` was written by `write_inline_ref` from a live `GcRef<ArrayObj>`
        // whose backing Region outlives this object (a field reference is a strong root
        // kept alive by GC tracing of `inline_refs`); `0` → `None` → `Null`.
        match unsafe { GcRef::<ArrayObj>::from_tagged_bits(bits) } {
            Some(r) => Value::Array(r),
            None => Value::Null,
        }
    } else {
        // SAFETY: as above, `bits` came from a live `GcRef<ScriptObject>`.
        match unsafe { GcRef::<ScriptObject>::from_tagged_bits(bits) } {
            Some(r) => Value::Object(r),
            None => Value::Null,
        }
    }
}

/// unify-object-byte-layout (PR-3 chunk 2b): write a `Value` into a byte-inlined direct
/// object/array reference slot (`off` in `bytes`). Heap `Object`/`Array` → their 8B tagged
/// pointer; `Null` → `0`. Any other value (including a stack-escaped `StackObject`/
/// `StackArray`, which must have been heap-promoted before reaching a field — see
/// `exec_object::field_set`'s debug_assert) defensively stores `0` rather than a bogus
/// pointer. The write barrier is fired separately by the caller.
#[inline]
pub(crate) fn write_inline_ref(bytes: &mut [u8], off: usize, v: &Value) {
    let bits: u64 = match v {
        Value::Object(r) => r.to_tagged_bits(),
        Value::Array(r) => r.to_tagged_bits(),
        _ => {
            debug_assert!(
                matches!(v, Value::Null),
                "inlined object/array field only holds a heap Object/Array or Null, got {v:?}"
            );
            0
        }
    };
    if off + 8 <= bytes.len() {
        bytes[off..off + 8].copy_from_slice(&bits.to_le_bytes());
    }
}

// ── Value ↔ byte codec (unify-object-byte-layout PR-2) ───────────────────────
//
// Serialization of a primitive `Value` to/from a byte window, keyed by `ty::TAG_*`.
// Lives in `metadata` (not `interp`) because both object byte-storage
// (`ScriptObject::field_value`) and value-struct blobs (`interp::exec_struct`) consume
// it, and `metadata` is the lower layer both depend on. Moved here from
// `interp::exec_struct` by PR-2 (was `add-struct-heap-inline`).

/// Whether a leaf `ty::TAG_*` denotes a reference leaf (`string` / object / array),
/// which lives in the blob's / object's `refs` side-slice rather than byte-packed.
#[inline]
pub fn is_ref_tag(tag: u8) -> bool {
    matches!(tag, TAG_STR | TAG_OBJECT | TAG_ARRAY)
}

/// Byte width of a primitive leaf by its `ty::TAG_*`.
pub fn prim_width(kind: u8) -> anyhow::Result<usize> {
    Ok(match kind {
        TAG_BOOL | TAG_I8 | TAG_U8 => 1,
        TAG_I16 | TAG_U16 => 2,
        TAG_I32 | TAG_U32 | TAG_F32 | TAG_CHAR => 4,
        TAG_I64 | TAG_U64 | TAG_F64 => 8,
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    })
}

/// Decode `w` bytes at `off` into a `Value` per `kind`. Integers → `Value::I64`,
/// f32/f64 → `Value::F64`, bool → `Value::Bool`, char → `Value::Char` (mirrors the
/// VM's scalar representation of primitives).
pub fn decode_prim(bytes: &[u8], off: usize, w: usize, kind: u8) -> anyhow::Result<Value> {
    if off + w > bytes.len() {
        anyhow::bail!("struct field read out of blob bounds (off={off}, w={w}, len={})", bytes.len());
    }
    let b = &bytes[off..off + w];
    let v = match kind {
        TAG_BOOL => Value::Bool(b[0] != 0),
        TAG_I8   => Value::I64(b[0] as i8 as i64),
        TAG_U8   => Value::I64(b[0] as i64),
        TAG_I16  => Value::I64(i16::from_le_bytes([b[0], b[1]]) as i64),
        TAG_U16  => Value::I64(u16::from_le_bytes([b[0], b[1]]) as i64),
        TAG_I32  => Value::I64(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        TAG_U32  => Value::I64(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64),
        TAG_I64 | TAG_U64 => {
            let mut a = [0u8; 8]; a.copy_from_slice(b); Value::I64(i64::from_le_bytes(a))
        }
        TAG_F32  => Value::F64(f32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64),
        TAG_F64  => {
            let mut a = [0u8; 8]; a.copy_from_slice(b); Value::F64(f64::from_le_bytes(a))
        }
        TAG_CHAR => {
            let cp = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            Value::Char(char::from_u32(cp).unwrap_or('\0'))
        }
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    };
    Ok(v)
}

/// Encode `val` into `w` bytes at `off` per `kind` (in place).
pub fn encode_prim(bytes: &mut [u8], off: usize, w: usize, kind: u8, val: &Value) -> anyhow::Result<()> {
    if off + w > bytes.len() {
        anyhow::bail!("struct field write out of blob bounds (off={off}, w={w}, len={})", bytes.len());
    }
    match kind {
        TAG_BOOL => bytes[off] = if codec_as_bool(val)? { 1 } else { 0 },
        TAG_I8 | TAG_U8 => bytes[off] = codec_as_i64(val)? as u8,
        TAG_I16 | TAG_U16 => bytes[off..off + 2].copy_from_slice(&(codec_as_i64(val)? as u16).to_le_bytes()),
        TAG_I32 | TAG_U32 => bytes[off..off + 4].copy_from_slice(&(codec_as_i64(val)? as u32).to_le_bytes()),
        TAG_I64 | TAG_U64 => bytes[off..off + 8].copy_from_slice(&codec_as_i64(val)?.to_le_bytes()),
        TAG_F32 => bytes[off..off + 4].copy_from_slice(&(codec_as_f64(val)? as f32).to_le_bytes()),
        TAG_F64 => bytes[off..off + 8].copy_from_slice(&codec_as_f64(val)?.to_le_bytes()),
        TAG_CHAR => bytes[off..off + 4].copy_from_slice(&codec_as_char_u32(val)?.to_le_bytes()),
        other => anyhow::bail!("struct field: unsupported primitive tag {other:#x}"),
    }
    Ok(())
}

fn codec_as_i64(v: &Value) -> anyhow::Result<i64> {
    match v {
        Value::I64(n) => Ok(*n),
        Value::Bool(b) => Ok(*b as i64),
        Value::Char(c) => Ok(*c as i64),
        other => anyhow::bail!("struct field: expected an integer value, got {other:?}"),
    }
}

fn codec_as_f64(v: &Value) -> anyhow::Result<f64> {
    match v {
        Value::F64(f) => Ok(*f),
        Value::I64(n) => Ok(*n as f64),
        other => anyhow::bail!("struct field: expected a float value, got {other:?}"),
    }
}

fn codec_as_bool(v: &Value) -> anyhow::Result<bool> {
    match v {
        Value::Bool(b) => Ok(*b),
        Value::I64(n) => Ok(*n != 0),
        other => anyhow::bail!("struct field: expected a bool value, got {other:?}"),
    }
}

fn codec_as_char_u32(v: &Value) -> anyhow::Result<u32> {
    match v {
        Value::Char(c) => Ok(*c as u32),
        Value::I64(n) => Ok(*n as u32),
        other => anyhow::bail!("struct field: expected a char value, got {other:?}"),
    }
}

#[derive(Debug, Default, Clone)]
pub struct TypeDescCold {
    /// fix-cross-pkg-subclass-fields (2026-05-14): the fields **this class
    /// itself declares** (excluding inherited). Preserved so the cross-zpkg
    /// fixup pass can rebuild `fields` = base.fields ++ own_fields once the
    /// base class becomes resolvable via the global type registry.
    pub own_fields: Box<[FieldSlot]>,
    /// fix-cross-pkg-subclass-fields (2026-05-14): the **qualified func
    /// names** of methods this class itself defines, in the order they
    /// were discovered by `build_type_registry`. Used by fixup to rebuild
    /// `vtable` (preserving override-vs-append semantics) once the base
    /// class becomes resolvable.
    ///
    /// review.md E5.5 (2026-05-27): the simple method name (vtable slot
    /// key) is no longer stored — it's derived at merge time via
    /// [`TypeDesc::derive_simple_method_name`] given the owning class
    /// name. Saves one heap allocation + 16–24 B per method.
    pub own_methods: Box<[Box<str>]>,
    /// Generic type parameter names: ["T"], ["K", "V"]. Empty for non-generic classes.
    pub type_params: Box<[String]>,
    /// Concrete type arguments for an instantiated generic class: ["int"], ["string", "int"].
    /// Empty for non-generic classes and uninstantiated generic definitions.
    pub type_args: Box<[String]>,
    /// L3-G3a: constraint bundle per type parameter (aligned by index with `type_params`).
    /// Empty for non-generic classes; inner bundle may be empty for unconstrained params.
    pub type_param_constraints: Box<[super::bytecode::ConstraintBundle]>,
    /// C3 add-attribute-reflection: user attributes applied to this class
    /// (carried from the zbc TYPE section). Each is (attribute-type qualified
    /// name, factory-func qualified name). `__type_custom_attributes` calls each
    /// factory once and caches the resulting instances on the Type object.
    pub custom_attributes: Box<[super::bytecode::AttributeRef]>,
    /// add-reflection-static-fields (zbc 1.13): the class's static fields
    /// (separate from hot `TypeDesc::fields`, the instance layout). Reflection
    /// only — surfaced by `Type.GetFields()` with `FieldInfo.IsStatic = true`.
    pub static_fields: Box<[super::bytecode::FieldDesc]>,
    /// add-field-attribute-reflection (zbc 1.14): per-field user-attribute refs,
    /// indexed by field name (instance + static fields with attributes).
    /// `__field_custom_attributes` resolves a field's factories here.
    /// Reflection only; empty for classes with no field attributes.
    pub field_attributes: Box<[(Box<str>, Box<[super::bytecode::AttributeRef]>)]>,
    /// add-reflection-get-interfaces (zbc 1.17): the interface names this class
    /// directly declares (bare; e.g. "IFoo"). Reflection only — surfaced by
    /// `Type.GetInterfaces()`, which base-walks the `base_name` chain to also
    /// include inherited interfaces (dedup by name). Empty = none.
    pub interfaces: Box<[Box<str>]>,
    /// add-enum-type-metadata (zbc 1.22): enum member (name, i64 value) pairs.
    /// Reflection only — surfaced by `Enum.GetNames/GetValues/GetName`; presence
    /// mirrors `class_flags & CLASS_FLAG_ENUM` (i.e. `Type.IsEnum`). Empty = non-enum.
    pub enum_members: Box<[(String, i64)]>,
    /// add-interface-member-reflection: the interface's directly-declared method
    /// signatures (zbc 1.28 block). Reflection only — surfaced by
    /// `Type.GetMethods()`; presence mirrors `class_flags & CLASS_FLAG_INTERFACE`.
    /// Empty for non-interface types.
    pub iface_methods: Box<[super::bytecode::IfaceMethodSig]>,
    /// add-struct-value-semantics (A-use): the value-struct byte + reference
    /// layout (from the zbc TYPE-section struct block, present only when
    /// `class_flags & CLASS_FLAG_STRUCT`). Shared (`Arc`) so `StructAlloc` can
    /// hand it to every blob it allocates without recomputing. `None` for
    /// non-struct types and structs whose module predates the block.
    pub struct_layout: Option<std::sync::Arc<StructTypeLayout>>,
    /// add-struct-heap-inline (P3b, D1-a): the **composed** inline-struct layout of
    /// this class's instances — `size` = total `ScriptObject::struct_bytes` length,
    /// `ref_offsets`/`ref_kinds` = the object-relative reference bitmap of all inline
    /// struct fields (each leaf's `field_byte_off + leaf_off`, in order). Reuses
    /// `StructTypeLayout` because the object's inline region is exactly a byte blob +
    /// reference side-table, same shape as an arena blob. `None` for classes with no
    /// inline struct fields. Delivered by the zbc 1.32 inline-field table (stage 3);
    /// consumed by `ScriptObject` alloc + `exec_struct` object-base field access.
    pub inline_layout: Option<std::sync::Arc<StructTypeLayout>>,
    /// unify-object-byte-layout (PR-1): the class's **full object field layout** at 8B
    /// reference width (from the zbc 1.34 object block, present for normal reference
    /// classes). Carries per-field (offset/size/kind) + flattened 8B reference bitmap.
    /// **Dormant in PR-1** — carried here but not consumed (field access still goes
    /// through `slots`); PR-2 switches `ScriptObject` field storage to this byte layout.
    /// Holds the parsed descriptor directly (no runtime form yet). `None` for
    /// value/interface/enum/delegate types and modules predating the block.
    pub object_layout: Option<std::sync::Arc<super::bytecode::ObjectLayoutDesc>>,
    /// unify-object-byte-layout (PR-2): the **composed** runtime object layout —
    /// `object_layout` (own-only) merged with the base class's composed layout via
    /// `compose_object_layout` (`base.composed ++ own`, own region at
    /// `align_up(base.size, 8)`). Built at load time (`build_type_registry` for
    /// local bases, recomputed by `try_fixup_inheritance` for cross-zpkg bases),
    /// mirroring how `fields` is built from `own_fields`. `field_offsets[i]` aligns
    /// by index with `fields[i]`. **Dormant in task 2.0** — carried + unit-tested but
    /// not consumed (field access still via `slots`); task 2.1+ switches storage onto
    /// it. `None` for value/interface/enum/delegate types and modules predating the
    /// zbc 1.34 object block.
    pub composed_object_layout: Option<std::sync::Arc<ObjectLayout>>,
}

impl TypeDesc {
    #[inline]
    fn cold_slice<T, F: FnOnce(&TypeDescCold) -> &[T]>(&self, f: F) -> &[T] {
        match self.cold.as_ref() {
            Some(c) => f(c),
            None    => &[],
        }
    }

    #[inline] pub fn own_fields(&self)             -> &[FieldSlot]                              { self.cold_slice(|c| &c.own_fields) }
    #[inline] pub fn own_methods(&self)            -> &[Box<str>]                               { self.cold_slice(|c| &c.own_methods) }
    #[inline] pub fn type_params(&self)            -> &[String]                                 { self.cold_slice(|c| &c.type_params) }
    #[inline] pub fn type_args(&self)              -> &[String]                                 { self.cold_slice(|c| &c.type_args) }
    #[inline] pub fn type_param_constraints(&self) -> &[super::bytecode::ConstraintBundle]      { self.cold_slice(|c| &c.type_param_constraints) }
    /// C3 add-attribute-reflection: user attributes applied to this class.
    #[inline] pub fn custom_attributes(&self)      -> &[super::bytecode::AttributeRef]          { self.cold_slice(|c| &c.custom_attributes) }
    /// add-reflection-static-fields: the class's static fields (reflection only).
    #[inline] pub fn static_fields(&self)          -> &[super::bytecode::FieldDesc]             { self.cold_slice(|c| &c.static_fields) }
    /// add-interface-member-reflection: the interface's declared method signatures.
    #[inline] pub fn iface_methods(&self)          -> &[super::bytecode::IfaceMethodSig]        { self.cold_slice(|c| &c.iface_methods) }
    /// add-field-attribute-reflection: per-field attr refs (field name → refs).
    #[inline] pub fn field_attributes(&self)       -> &[(Box<str>, Box<[super::bytecode::AttributeRef]>)] { self.cold_slice(|c| &c.field_attributes) }
    /// add-reflection-get-interfaces: the class's directly-declared interfaces.
    #[inline] pub fn interfaces(&self)             -> &[Box<str>]                               { self.cold_slice(|c| &c.interfaces) }
    /// add-enum-type-metadata: enum member (name, value) pairs (reflection only).
    #[inline] pub fn enum_members(&self)           -> &[(String, i64)]                          { self.cold_slice(|c| &c.enum_members) }
    /// add-struct-value-semantics: the value-struct byte + reference layout, if
    /// this is a struct with a delivered TYPE-section struct block. Cloned `Arc`
    /// (cheap) so `StructAlloc` can share one layout across all blobs of the type.
    #[inline] pub fn struct_layout(&self) -> Option<std::sync::Arc<StructTypeLayout>> {
        self.cold.as_ref().and_then(|c| c.struct_layout.clone())
    }
    /// add-struct-heap-inline (P3b, D1-a): the composed inline-struct layout of this
    /// class's instances (object-relative byte size + reference bitmap). `None`
    /// unless the class declares inline struct fields. Cloned `Arc` (cheap).
    #[inline] pub fn inline_layout(&self) -> Option<std::sync::Arc<StructTypeLayout>> {
        self.cold.as_ref().and_then(|c| c.inline_layout.clone())
    }
    /// unify-object-byte-layout (PR-2): the composed runtime object byte layout of
    /// this reference class's instances (`base.composed ++ own`, own at
    /// `align_up(base.size, 8)`). `None` for value/interface/enum/delegate types and
    /// modules predating the zbc 1.34 object block. Cloned `Arc` (cheap). **Dormant
    /// in task 2.0** — not yet consumed for field storage.
    #[inline] pub fn composed_object_layout(&self) -> Option<std::sync::Arc<ObjectLayout>> {
        self.cold.as_ref().and_then(|c| c.composed_object_layout.clone())
    }
    /// add-struct-heap-inline (P3b, D1-a): total `(struct_bytes_len, struct_refs_len)`
    /// for an instance of this class — the size of the inline value-struct byte
    /// region + the reference side-table `ScriptObject` must allocate. `(0, 0)`
    /// unless the class declares inline struct fields.
    ///
    /// Reads the composed `inline_layout` on `TypeDescCold` (`size`, `ref_count`).
    /// `(0, 0)` until the zbc 1.32 inline-field table (stage 3) populates it — so
    /// every existing object stays byte-identical while the loader still delivers
    /// `None`.
    /// unify-object-byte-layout (PR-2): `(bytes_len, refs_len)` for a fresh instance —
    /// the whole object's byte region + reference side-table (replaces P3b's
    /// inline-only `inline_region_sizes`).
    ///
    /// - **Boxed value struct** (`is_struct`): the payload IS the struct blob — size
    ///   from the type's own `struct_layout` (blob byte size + reference-leaf count).
    ///   (add-boxed-struct-identity P4b: only boxes hit `alloc_object`; frame-arena
    ///   value structs never do.)
    /// - **Normal reference class**: the composed object layout (`base.composed ++ own`)
    ///   — `size` bytes + `ref_count` reference slots.
    /// - **No layout** (synthetic / Rust-constructed / fallback): synthesize from
    ///   `fields`; `(0, 0)` for a field-less type.
    #[inline]
    pub fn object_region_sizes(&self) -> (usize, usize) {
        if self.is_struct() {
            return match self.struct_layout() {
                Some(sl) => (sl.size, sl.ref_count()),
                None => (0, 0),
            };
        }
        if let Some(col) = self.composed_object_layout() {
            return (col.size, col.ref_count());
        }
        if self.fields.is_empty() { return (0, 0); }
        let l = synthesize_object_layout(&self.fields);
        (l.size, l.ref_count())
    }

    /// unify-object-byte-layout (PR-2): allocate the zero-initialized byte region +
    /// `Null`-filled reference side-table for a fresh instance. Zero bytes = every
    /// primitive field's default (0 / false / '\0'); `Null` refs = every reference
    /// field's default. Single sizing site for every `ScriptObject` construction.
    #[inline]
    pub fn object_regions(&self) -> (Box<[u8]>, Box<[Value]>) {
        let (nb, nr) = self.object_region_sizes();
        let bytes = if nb == 0 { Box::from([]) } else { vec![0u8; nb].into_boxed_slice() };
        let refs = if nr == 0 { Box::from([]) } else { vec![Value::Null; nr].into_boxed_slice() };
        (bytes, refs)
    }

    /// add-struct-value-semantics: whether this type is a value struct (Type.IsValueType).
    #[inline] pub fn is_struct(&self)             -> bool { self.class_flags & super::bytecode::CLASS_FLAG_STRUCT != 0 }
    /// add-record-value-semantics: whether this type is a `[Record]` (Type.IsRecord). Used by the
    /// boxed-struct vcall arm to step aside from the native ToString intercept so a record struct's
    /// compiler-synthesized `<Type>.ToString` (record format) is reached instead of the type name.
    #[inline] pub fn is_record(&self)             -> bool { self.class_flags & super::bytecode::CLASS_FLAG_RECORD != 0 }
    /// add-enum-type-metadata: whether this type is an enum (Type.IsEnum).
    #[inline] pub fn is_enum(&self)                -> bool { self.class_flags & super::bytecode::CLASS_FLAG_ENUM != 0 }
    #[inline] pub fn is_delegate(&self)            -> bool { self.class_flags & super::bytecode::CLASS_FLAG_DELEGATE != 0 }

    /// Lazy-init the cold side-table for mutation.
    #[inline]
    pub fn cold_mut(&mut self) -> &mut TypeDescCold {
        self.cold.get_or_insert_with(|| Box::new(TypeDescCold::default()))
    }

    /// review.md E5.5 (2026-05-27): derive the simple method name (vtable
    /// slot key) from a qualified function name in `own_methods`. Strips
    /// the owning class's `"<ClassName>."` prefix, then the arity suffix
    /// `"$N"` (so `Foo.Bar.Method$2` → `Method`).
    ///
    /// Returns the input unchanged when the prefix doesn't match — a
    /// defensive fallback that should never fire in practice because
    /// `build_type_registry` only inserts entries with the matching
    /// prefix.
    #[inline]
    pub fn derive_simple_method_name<'a>(class_name: &str, fq: &'a str) -> &'a str {
        let dot = class_name.len();
        if fq.len() <= dot + 1
            || !fq.is_char_boundary(dot)
            || !fq.as_bytes().get(dot).is_some_and(|&b| b == b'.')
            || &fq[..dot] != class_name
        {
            return fq;
        }
        let after_prefix = &fq[dot + 1..];
        after_prefix.split('$').next().unwrap_or(after_prefix)
    }
}

// ── NativeData — native backing for built-in class types ────────────────────
//
// Analogous to CoreCLR's inline data in String/Array objects.
// Provides a native backing store for classes that wrap VM primitives.

/// Native backing data for built-in classes.
///
/// Used by `ScriptObject` to hold VM-managed state that should not be
/// directly accessible as a z42 field (i.e. not visible in `slots`).
#[derive(Debug, Clone)]
pub enum NativeData {
    /// No native backing — ordinary user-defined class.
    None,
    /// 2026-05-04 expose-weak-ref-builtin (D-1a)：包装 GC 弱引用句柄。
    /// 由 `__obj_make_weak` builtin 创建；`__obj_upgrade_weak` 升格回原对象。
    /// 用户视角是 `Std.WeakHandle` 类（无字段）。
    WeakRef(crate::gc::WeakRef),
    /// 2026-06-08 add-reflection-mvp：`Std.Type` 对象携带的真实类型句柄。
    /// 由 `__obj_get_type` 对 `Value::Object` 创建（存对象 `type_desc` 的
    /// `Arc<TypeDesc>`）；反射 builtins（`__type_fields` / `__type_methods` /
    /// `__type_base` / `__type_generic_args`）据此枚举成员。基础类型/数组的
    /// synthetic Type 无此句柄（`NativeData::None`），成员查询退化为空。
    TypeHandle(Arc<TypeDesc>),
    /// 2026-07-30 add-load-context-model：`Std.Runtime.LoadContext` 对象携带的
    /// 上下文句柄（root = `ContextId::ROOT`）。`__lctx_*` builtins 据此查
    /// `VmCore.context_registry`。
    LoadContextHandle(super::context::ContextId),
    /// 2026-07-30 add-load-context-model：`Std.Reflection.Assembly` 对象携带的
    /// 程序集句柄（zpkg 运行时投影）。`__asm_*` builtins 据此查注册表。
    AssemblyHandle(super::context::AssemblyId),
    // 2026-04-26 script-first-stringbuilder: removed `StringBuilder(String)` —
    // `Std.Text.StringBuilder` is now a pure z42 script. Variant slot kept open
    // for future native-backed types (Stream / FileHandle / etc.).
}

// ── ScriptObject — unified managed object ───────────────────────────────────
//
// Replaces the old `ObjectData`. Every class instance is represented as a
// `ScriptObject`, which combines:
//   1. A type descriptor pointer (Arc<TypeDesc>) — the class identity
//   2. A flat slot array (Vec<Value>)            — instance fields by index
//   3. Optional native backing (NativeData)      — for built-in types

/// Heap-allocated managed object with reference semantics (CoreCLR Object equivalent).
#[derive(Debug)]
pub struct ScriptObject {
    /// Type descriptor shared across all instances of this class.
    pub type_desc: Arc<TypeDesc>,
    /// unify-object-byte-layout (PR-2): the object's **byte-packed** field storage.
    /// Every primitive leaf of every direct field (incl. inline-struct interior
    /// primitive leaves) lives at its composed byte offset (`ObjectLayout::field_access`
    /// / `field_offsets`); reference fields occupy an 8B hole here (dead in PR-2 — the
    /// value is in `refs`; PR-3 inlines the 8B pointer). Replaces the pre-PR-2
    /// `slots: Box<[Value]>` + `struct_bytes` (P3b). Size = `ObjectLayout::size`
    /// (or the type's `struct_layout` size for a boxed value struct). Zero-initialized
    /// at alloc = every primitive field's default (0 / false / '\0').
    pub bytes: Box<[u8]>,
    /// unify-object-byte-layout (PR-2): the object's **reference leaves** as real
    /// `Value`s in a side-table — every reference field + every inline-struct interior
    /// reference leaf, ordered by the composed reference bitmap
    /// (`ObjectLayout::ref_offsets` / `FieldAccess::ref_slot`). GC scans these directly
    /// (`visitor(&Value)`); a write to one is a plain `Value`-slot store routed through
    /// `write_barrier_field`. Replaces the pre-PR-2 `struct_refs` (P3b) and the
    /// reference cells of `slots`. `Null`-filled at alloc.
    pub refs: Box<[Value]>,
    /// Native backing for built-in types (e.g. StringBuilder buffer).
    pub native: NativeData,
    /// 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): per-instance
    /// generic type-arguments. For `new Foo<int, string>()` this is
    /// `["int", "string"]`. Empty for non-generic classes and uninstantiated
    /// generic definitions. Index aligns with `type_desc.type_params`.
    /// Read by `DefaultOf` opcode and any future runtime type-args queries.
    ///
    /// review.md E5.4 follow-up (2026-05-27): `Box<[String]>` instead of
    /// `Vec<String>` — written exactly once at `obj.new` time, then
    /// read-only for the object's lifetime. Saves 8 B/ScriptObject vs
    /// `Vec`. StringId migration deferred to Phase B+.
    pub type_args: Box<[String]>,
}

impl ScriptObject {
    /// unify Phase 2 R3（装箱统一）：若本对象是**整数基元装箱盒**（`type_desc` 是整数 wrapper、
    /// 标量 LE 字节存 `struct_bytes`，见 `corelib::convert::box_prim_to_heap`），读回其 i64 标量；
    /// 否则（多字段 struct 装箱 / 非整数 wrapper）返 `None`。按 wrapper 宽度 + 有无符号从
    /// `struct_bytes` 前 `width` 字节还原（signed narrow → 符号扩展，unsigned → 零扩展）。
    /// 让装箱整数盒与 struct 装箱盒共用 `Value::BoxedStruct`，同时保留整数的透明拆箱语义。
    pub fn boxed_prim_i64(&self) -> Option<i64> {
        let (width, signed) =
            crate::metadata::well_known_names::int_wrapper_scalar_spec(&self.type_desc.name)?;
        if self.bytes.len() < width {
            return None;
        }
        let mut buf = [0u8; 8];
        buf[..width].copy_from_slice(&self.bytes[..width]);
        let mut v = i64::from_le_bytes(buf);
        if signed && width < 8 {
            let shift = (8 - width) * 8;
            v = (v << shift) >> shift; // 符号扩展窄整数
        }
        Some(v)
    }

    /// unify-object-byte-layout (PR-2): the resolved `FieldAccess` for a direct field
    /// `slot` (see `TypeDesc::field_index`). Reads the type's composed object layout;
    /// falls back to on-the-fly synthesis for a layout-less type (rare — synthetic /
    /// Rust-constructed). `FieldAccess` is `Copy`, so no borrow of the layout escapes.
    #[inline]
    fn field_access_of(&self, slot: usize) -> Option<FieldAccess> {
        if let Some(col) = self.type_desc.composed_object_layout() {
            return col.field_access.get(slot).copied();
        }
        if self.type_desc.fields.is_empty() { return None; }
        synthesize_object_layout(&self.type_desc.fields).field_access.get(slot).copied()
    }

    /// unify-object-byte-layout (PR-2): read direct field `slot` as a `Value`.
    /// Primitive → `decode_prim` off `bytes`; reference → the `refs` side-table cell.
    /// `Null` for an out-of-range slot or a struct-typed root (accessed via
    /// `StructFieldGetPrim`, never `FieldGet`). Replaces `self.slots[slot].clone()`.
    #[inline]
    pub fn field_value(&self, slot: usize) -> Value {
        let fa = match self.field_access_of(slot) { Some(f) => f, None => return Value::Null };
        if fa.ref_slot >= 0 {
            return self.refs.get(fa.ref_slot as usize).cloned().unwrap_or(Value::Null);
        }
        // PR-3 chunk 2b: an inlined direct object/array reference (`ref_slot == -1` but a
        // reference tag) — read the 8B tagged pointer straight from `bytes` (0 = `Null`).
        if fa.tag == TAG_OBJECT || fa.tag == TAG_ARRAY {
            return read_inline_ref(&self.bytes, fa.offset as usize, fa.tag == TAG_ARRAY);
        }
        if fa.tag == TAG_UNKNOWN {
            return Value::Null; // struct-typed root — not a FieldGet target
        }
        decode_prim(&self.bytes, fa.offset as usize, fa.width as usize, fa.tag)
            .unwrap_or(Value::Null)
    }

    /// post-layout JIT perf (P5-B): if `name` is a direct **inline primitive**
    /// field — a scalar packed in `bytes`, NOT a `refs` side-table reference, a
    /// byte-inlined object/array pointer, a struct-typed root, or a string — return
    /// `(bytes base ptr, byte offset, width, tag)`. The JIT hoists this once per
    /// never-reassigned object and emits a native width-aware byte load/store
    /// (mirroring `decode_prim`/`encode_prim`) instead of calling `jit_field_get`/
    /// `jit_field_set`. `None` (→ keep the helper) for anything else, so reference
    /// writes still fire the GC `write_barrier_field` and struct/string/polymorphic
    /// access keeps its full semantics. The returned pointer is valid for the frame:
    /// non-moving GC + fixed `bytes` allocation + caller holds the object live.
    #[inline]
    pub fn inline_prim_field(&self, name: &str) -> Option<(*const u8, u32, u32, u8)> {
        let slot = *self.type_desc.field_index.get(name)?;
        let fa = self.field_access_of(slot)?;
        if fa.ref_slot >= 0 { return None; } // reference in `refs` side-table
        match fa.tag {
            // byte-inlined obj/array ref, struct root, or string → not a scalar prim
            TAG_OBJECT | TAG_ARRAY | TAG_UNKNOWN | TAG_STR => return None,
            _ => {}
        }
        Some((self.bytes.as_ptr(), fa.offset, fa.width, fa.tag))
    }

    /// post-layout JIT perf (T1-B): if `name` is a direct **byte-inlined reference**
    /// field — a class-instance (`STRUCT_LEAF_GCREF` → `TAG_OBJECT`) or array
    /// (`STRUCT_LEAF_GCREF_ARRAY` → `TAG_ARRAY`) whose 8B tagged pointer lives in
    /// `bytes` (`ref_slot == -1`) — return `(bytes base ptr, byte offset, is_array)`.
    /// The JIT hoists this once per never-reassigned receiver and emits a native 8B
    /// load of the tagged pointer + a `Value::Object`/`Value::Array` (or `Value::Null`
    /// for the `0` sentinel) register store, byte-identical to `read_inline_ref`,
    /// instead of calling `jit_field_get`. `None` (→ keep the helper) for a primitive,
    /// a side-table reference (`ref_slot ≥ 0`: closure/func/**string** — the string
    /// GcRef path stays on the helper), a struct-typed root (`TAG_UNKNOWN`), or an
    /// out-of-range slot. Reads only (no write barrier); the returned pointer is valid
    /// for the frame (non-moving GC + fixed `bytes` + caller holds the object live).
    #[inline]
    pub fn inline_ref_field(&self, name: &str) -> Option<(*const u8, u32, bool)> {
        let slot = *self.type_desc.field_index.get(name)?;
        let fa = self.field_access_of(slot)?;
        if fa.ref_slot >= 0 { return None; } // side-table reference (closure/func/string)
        match fa.tag {
            TAG_OBJECT => Some((self.bytes.as_ptr(), fa.offset, false)),
            TAG_ARRAY  => Some((self.bytes.as_ptr(), fa.offset, true)),
            _ => None, // primitive / struct root / string
        }
    }

    /// unify-object-byte-layout (PR-2): write direct field `slot` from `v`.
    /// Primitive → `encode_prim` into `bytes`; reference → the `refs` side-table cell.
    /// Returns `true` iff the target is a reference slot (so the caller fires a GC
    /// `write_barrier_field` when `v.is_heap_ref()`). No-op (returns `false`) for an
    /// out-of-range slot or struct-typed root. Replaces `self.slots[slot] = v`.
    #[inline]
    pub fn set_field_value(&mut self, slot: usize, v: &Value) -> bool {
        let fa = match self.field_access_of(slot) { Some(f) => f, None => return false };
        if fa.ref_slot >= 0 {
            if let Some(cell) = self.refs.get_mut(fa.ref_slot as usize) { *cell = v.clone(); }
            return true;
        }
        // PR-3 chunk 2b: an inlined direct object/array reference — write the 8B tagged
        // pointer into `bytes` (`Null`/non-heap → 0). Returns `true` so the caller still
        // fires `write_barrier_field` (the target IS a reference slot, just byte-inlined).
        if fa.tag == TAG_OBJECT || fa.tag == TAG_ARRAY {
            write_inline_ref(&mut self.bytes, fa.offset as usize, v);
            return true;
        }
        if fa.tag == TAG_UNKNOWN { return false; } // struct-typed root
        // Reflection (FieldInfo/PropertyInfo SetValue) passes primitives **boxed**
        // (`int` → a `Std.Int32` `BoxedStruct`); a boxed primitive's bytes ARE its raw
        // scalar, so decode it with the field's tag/width to recover the plain `Value`
        // that `encode_prim` needs. Non-boxed values (the common FieldSet path from z42
        // code) pass through untouched.
        let unboxed: Value;
        let src: &Value = match v {
            Value::BoxedStruct(gc) => {
                let b = gc.borrow();
                if b.bytes.len() >= fa.width as usize {
                    unboxed = decode_prim(&b.bytes, 0, fa.width as usize, fa.tag)
                        .unwrap_or(Value::Null);
                    &unboxed
                } else {
                    // Not a same-width primitive box — leave as-is (encode may reject).
                    v
                }
            }
            _ => v,
        };
        let _ = encode_prim(&mut self.bytes, fa.offset as usize, fa.width as usize, fa.tag, src);
        false
    }

    /// unify-object-byte-layout (PR-3 chunk 2b): visit the object's **byte-inlined**
    /// direct object/array references — the ones pulled out of `refs` into `bytes`.
    /// Reads each 8B tagged pointer at its `InlineRef::offset`, rebuilds the `Value`
    /// (`Object`/`Array` by `is_array`), and hands live (non-`Null`) ones to the GC
    /// visitor. Complements the `for r in &obj.refs` side-table scan; together they
    /// cover every reference edge. No-op for types without a composed object layout
    /// (value structs use `struct_layout`; synthesized layouts inline nothing).
    #[inline]
    pub fn trace_inline_refs(&self, visit: &mut dyn FnMut(&Value)) {
        if let Some(col) = self.type_desc.composed_object_layout() {
            for ir in col.inline_refs.iter() {
                let v = read_inline_ref(&self.bytes, ir.offset as usize, ir.is_array);
                if !matches!(v, Value::Null) {
                    visit(&v);
                }
            }
        }
    }

    /// unify-object-byte-layout (PR-3 chunk 2b): zero every byte-inlined object/array
    /// reference window (→ the `0` `Null` sentinel). The `bytes` twin of nulling the
    /// `refs` side-table: used when finalizing/tombstoning an object to break strong
    /// reference edges (both the side-table `refs` and the inlined pointers must be
    /// cleared, else a cycle stays anchored through `bytes`).
    #[inline]
    pub fn clear_inline_refs(&mut self) {
        // Collect offsets first so the `type_desc` layout borrow is released before the
        // mutable `bytes` write (disjoint fields, but keeps the borrow checker happy).
        let offsets: Vec<u32> = match self.type_desc.composed_object_layout() {
            Some(col) if !col.inline_refs.is_empty() => {
                col.inline_refs.iter().map(|ir| ir.offset).collect()
            }
            _ => return,
        };
        for off in offsets {
            let off = off as usize;
            if off + 8 <= self.bytes.len() {
                self.bytes[off..off + 8].fill(0);
            }
        }
    }
}

impl crate::gc::GcRef<ScriptObject> {
    /// **extract-typedesc-from-mutex (2026-05-31)**: lockless read of
    /// the object's `type_desc`. type_desc is set by `alloc_object` and
    /// never mutated for the object's lifetime — there's no concurrent
    /// writer, so bypassing the per-entry Mutex is sound. Used by
    /// hot-path IC scans (VCallIC, FieldIC, IsInstance) and the GC mark
    /// traversal.
    ///
    /// Returns a `&TypeDesc` borrowed for the GcRef's lifetime. The
    /// Arc itself stays alive through the entry's storage; the borrow
    /// is to the inner TypeDesc directly (one fewer deref at the call
    /// site than returning `&Arc<TypeDesc>`).
    #[inline]
    pub fn type_desc(&self) -> &TypeDesc {
        // SAFETY: type_desc is write-once-at-alloc. Verified 0 mutation
        // sites in the runtime via `grep -rn '.type_desc *=' src/`.
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_desc }
    }

    /// Lockless read of the object's `type_desc` as `&Arc<TypeDesc>`.
    /// Use this only when the caller needs to clone the Arc for
    /// ownership transfer (e.g. building a fallback TypeDesc, exception
    /// stack frames). Most callers want [`type_desc`] (returns plain
    /// `&TypeDesc`) which saves one deref.
    #[inline]
    pub fn type_desc_arc(&self) -> &Arc<TypeDesc> {
        // SAFETY: see type_desc() — write-once invariant.
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_desc }
    }

    /// **extract-typedesc-from-mutex (2026-05-31)**: lockless read of
    /// the object's `type_args` (generic type arguments at construction).
    /// Same write-once invariant as `type_desc` — set by `alloc_object`
    /// (per the spec, `alloc_object` accepts `type_args` and writes them
    /// before returning the GcRef), never mutated after.
    #[inline]
    pub fn type_args(&self) -> &[String] {
        // SAFETY: type_args is write-once-at-alloc; see type_desc().
        let obj_ptr: *const ScriptObject = self.data_ptr_unlocked();
        unsafe { &(*obj_ptr).type_args }
    }
}

// ── Value ────────────────────────────────────────────────────────────────────

/// Primitive and heap value types that the VM operates on at runtime.
///
/// Integer types are unified as I64 (all integer arithmetic is 64-bit internally).
/// The compiler emits ConstI32/ConstI64 which the VM widens to I64.
/// Floating-point is unified as F64 (double precision).
///
/// `Array` / `Object` 用 [`GcRef<T>`] 作为不透明堆引用句柄。Phase 3a backing
/// 是 `Rc<RefCell<T>>`（行为等价历史 `Rc<RefCell<...>>` 直构）；Phase 3b 切到
/// 自定义堆 + mark-sweep 时，本 enum 与所有 callsite 保持不变。
///
/// `Value::Str` remains a primitive for performance; member access on strings
/// is handled via virtual field dispatch in the interpreter.
///
/// 2026-04-29 remove-dead-value-map: 删除了 `Value::Map` variant —— 自从
/// 2026-04-26 extern-audit-wave0 把 `Std.Collections.Dictionary` 改为纯 z42
/// 脚本类（基于 `T[]`），Map variant 已无创建路径，作为 dead variant 一并清理。
/// review.md C2 P1 step 0 (2026-05-28): `#[repr(C, u8)]` locks the
/// discriminant + payload memory layout so the JIT can emit raw
/// `load`/`store` Cranelift instructions against register slots
/// without going through `extern "C"` helpers. Layout invariants:
///   * offset 0 — u8 discriminant (explicit assignments below)
///   * offset 8 — payload (aligned to max-payload alignment = 8)
///   * total size — 24 B (max payload = `Str(Arc<str>)` at 16 B)
/// Niche optimisation on `Option<Value>` is lost vs natural enum
/// layout, but `Value` is never stored as `Option<Value>` on hot
/// paths — `Frame::ret: Option<Value>` is the sole site and is
/// touched once per function return. Layout is pinned by
/// `value_layout_tests.rs`; drift fails CI before bad JIT code emits.
/// add-reflection-array-element-type (2026-06-11): the heap payload behind
/// `Value::Array`. Carries the element type's FQ name (written by `ArrayNew` /
/// `ArrayNewLit` from the compile-time-known element type) so reflection is
/// non-erased — `arr.GetType().GetElementType()` returns the real element type.
/// Derefs to the element `Vec<Value>` (plus `Index`/`IndexMut`) so every
/// existing array operation (len / index / iterate / push) works unchanged.
/// unify-gc-heap PR-3: `ArrayObj` is a fixed-length array header. Its element
/// storage lives in the **single GC variable-length heap** (`region_var`),
/// referenced by a [`VarGcRef`] inside [`ArrayBacking`] — no external `Vec`
/// (the CLR/JVM single-heap model). The header itself lives in `region_array`
/// (`Mutex`-guarded); the backing block is uniquely owned by this header and
/// accessed only under its `borrow`/`borrow_mut` lock, so element reads/writes
/// against the raw block payload are race-free (D13).
///
/// `ArrayObj` is intentionally **not `Clone`** (a derived shallow clone would
/// alias the backing block, breaking value-semantic array copies) — use
/// [`ArrayObj::deep_copy`] for a heap-aware independent copy.
#[derive(Debug)]
pub struct ArrayObj {
    /// Element type FQ name (e.g. "int" / "geometry.Point"). Empty = unknown
    /// (Rust-synthesized arrays like reflection result sets; user arrays from
    /// `ArrayNew` always carry it).
    pub element_type: Arc<str>,
    /// packed-primitive-arrays: element storage. **Step 1a** introduces this
    /// enum with only `Boxed` (behaviour-identical refactor). **Step 1b** adds
    /// packed primitive backings (Bytes/Chars/I32/I64/F64/Bool) — the C#
    /// value-type-array model (inline packed, no per-element boxing, GC skips).
    pub backing: ArrayBacking,
}

/// Array element storage — the C# value-type-vs-reference array distinction.
/// unify-gc-heap PR-3: every variant's element buffer is now a GC variable-length
/// block (`VarGcRef` into `region_var`) instead of an external `Vec` — the single
/// GC heap. Blocks are fixed-size (z42 arrays don't grow) and non-moving; the
/// variant tag discriminates boxing semantics + block layout:
/// - `Boxed`  → `BlockType::ArrayValue` block of `len` `Value`s (each a traced edge).
/// - packed (`Bool`/`Bytes`/`I32`/`I64`/`Chars`/`F64`) → `BlockType::ArrayPrim`
///   block of `len` packed `T`s (POD leaf — GC skips, no per-element boxing).
/// - `StructBytes` → **two** blocks: `bytes` (`ArrayStruct`, POD packed struct
///   bytes) + `refs` (`ArrayValue`, the reference side-table, traced).
/// box/unbox happens only at the ArrayGet/ArraySet boundary because interp
/// registers are `Value` (the JIT reads/writes packed blocks unboxed).
#[derive(Debug)]
pub enum ArrayBacking {
    Boxed { block: VarGcRef, len: usize },
    Bool  { block: VarGcRef, len: usize },
    Bytes { block: VarGcRef, len: usize },  // byte / sbyte（窄整型并入；box 语义按 element_type）
    I32   { block: VarGcRef, len: usize },  // int / uint / short / ushort
    I64   { block: VarGcRef, len: usize },  // long / ulong
    Chars { block: VarGcRef, len: usize },  // char（scalar，与 String.ToCharArray 对齐）
    F64   { block: VarGcRef, len: usize },  // double / float
    /// add-struct-heap-inline (P3b, D1-a): a **value-struct array** `Point[]` — the
    /// C# inline `struct[]` model. `len` elements' bytes are packed back-to-back in
    /// the `bytes` block (`len * elem_size`); reference leaves live in the parallel
    /// `refs` block (`len * layout.ref_count()`, element `i`'s refs at
    /// `[i*rc, (i+1)*rc)`). `layout` = the element struct type's byte+reference layout
    /// (shared `Arc` — type metadata, not per-instance data, stays out of GC).
    /// Element access goes through a `Value::StructRefHeap` handle (route α), not
    /// `get_boxed`/`set_boxed` (those have no array `GcRef` to build a handle from).
    StructBytes {
        elem_size: usize,
        len: usize,
        bytes: VarGcRef,
        refs: VarGcRef,
        layout: std::sync::Arc<StructTypeLayout>,
    },
    /// add-escape-analysis-stack-alloc / unify-gc-heap PR-3: an **escape-analysis
    /// stack array** — a non-escaping array whose storage lives in the per-frame
    /// stack arena (`ctx.stack_arena`), **not** the GC heap. Boxed `Value`s inline
    /// in an arena-owned `Vec` (mirrors `StackObject`'s off-GC `Box` backing +
    /// `StackClosure`'s arena env — escape-analysis products deliberately bypass GC).
    /// Its elements are scanned directly as GC roots by the stack-arena root scanner,
    /// so no GC block / `mark_backing` is needed. Only the stack-alloc construction
    /// path (`ArrayObj::stack_typed`) produces this; heap arrays never carry it.
    StackVec(Vec<Value>),
}

impl ArrayObj {
    // ── block payload accessors (unify-gc-heap PR-3) ────────────────────────
    // Reinterpret a backing block's inline payload as a `&[T]` / `&mut [T]`.
    //
    // SAFETY (shared): the caller holds the `ArrayObj` under a `borrow()`
    // (shared region lock), so the block is alive and no writer aliases it; the
    // block stores exactly `len` `T`s (allocated `len*size_of::<T>()` bytes,
    // 8-aligned payload, `align_of::<T>() <= 8`). The returned slice's lifetime is
    // tied to the accessor's `&self`, and the block outlives the header (freed
    // only when the header is swept), so the borrow is sound.
    #[inline]
    unsafe fn slice_of<T>(block: &VarGcRef, len: usize) -> &[T] {
        Self::debug_block_bounds::<T>(block, len, "slice_of");
        // SAFETY: see method contract; payload derived from the raw header ptr (D8).
        unsafe { std::slice::from_raw_parts(block.payload_as_ptr::<T>(), len) }
    }

    /// unify-gc-heap PR-3 safety guard: verify a `len`-element `T` view fits inside the
    /// backing block (alive + `len*size_of::<T>() <= payload_size`). Turns a would-be
    /// out-of-bounds / use-after-free block read into a clear panic instead of a raw SIGSEGV.
    #[inline]
    fn debug_block_bounds<T>(block: &VarGcRef, len: usize, ctx: &str) {
        // Debug-only (per-access; off in release to keep the array hot path lean).
        debug_assert!(block.is_live(),
            "unify-gc-heap PR-3: {ctx}<{}> on a stale/tombstoned block (len={len})", std::any::type_name::<T>());
        debug_assert!(
            len.checked_mul(std::mem::size_of::<T>()).is_some_and(|need| need <= block.payload_size()),
            "unify-gc-heap PR-3: {ctx}<{}> OOB — len={len} elems > block payload {}", std::any::type_name::<T>(), block.payload_size());
    }
    // SAFETY (exclusive): additionally the caller holds `borrow_mut()` (exclusive
    // region lock) so this `&mut [T]` uniquely aliases the block payload.
    #[inline]
    #[allow(clippy::mut_from_ref)]
    unsafe fn slice_of_mut<T>(block: &VarGcRef, len: usize) -> &mut [T] {
        Self::debug_block_bounds::<T>(block, len, "slice_of_mut");
        // SAFETY: see method contract; exclusive access + payload from raw header ptr (D8).
        unsafe { std::slice::from_raw_parts_mut(block.payload_as_ptr::<T>(), len) }
    }

    /// Allocate a `Value` block (`BlockType::ArrayValue`) and **move** `elems` into
    /// it. Returns the handle + element count. The block payload is zero-initialized
    /// by the allocator (`I64(0)` — a POD `Value`); `ptr::write` overwrites each slot
    /// without dropping, so every slot ends up an initialized moved `Value`.
    fn alloc_boxed(heap: &dyn MagrGC, elems: Vec<Value>) -> (VarGcRef, usize) {
        let len = elems.len();
        let block = heap.alloc_var_block(len * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `len` Values; write each moved value into its slot.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in elems.into_iter().enumerate() {
            // SAFETY: `base[i]` is one of `len` slots; `write` moves without dropping (POD zero).
            unsafe { base.add(i).write(v); }
        }
        (block, len)
    }

    /// Allocate a `Value` block and **clone** `src` into it (deep-copy / null-fill path).
    fn alloc_values_clone(heap: &dyn MagrGC, src: &[Value]) -> VarGcRef {
        let block = heap.alloc_var_block(src.len() * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `src.len()` Values.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in src.iter().enumerate() {
            // SAFETY: slot `i` in a `src.len()`-slot block; `write` moves the clone in.
            unsafe { base.add(i).write(v.clone()); }
        }
        block
    }

    /// Allocate a `Value` block of `n` slots all initialized to `Null` (struct[]
    /// reference side-table default — the allocator's zero-init is `I64(0)`, not `Null`).
    fn alloc_values_null(heap: &dyn MagrGC, n: usize) -> VarGcRef {
        let block = heap.alloc_var_block(n * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh block sized for `n` Values.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for i in 0..n {
            // SAFETY: slot `i` of `n`; write Null over the POD zero without dropping.
            unsafe { base.add(i).write(Value::Null); }
        }
        block
    }

    /// Allocate a packed POD block (`BlockType::ArrayPrim`) and copy `data` into it.
    fn alloc_packed<T: Copy>(heap: &dyn MagrGC, data: &[T]) -> VarGcRef {
        let block = heap.alloc_var_block(std::mem::size_of_val(data), BlockType::ArrayPrim);
        if !data.is_empty() {
            debug_assert!(std::mem::size_of_val(data) <= block.payload_size(),
                "unify-gc-heap PR-3: alloc_packed OOB write — {} bytes > block payload {}", std::mem::size_of_val(data), block.payload_size());
            // SAFETY: block payload sized `size_of_val(data)`, 8-aligned ≥ align_of::<T>();
            // src/dst are distinct, non-overlapping regions of `data.len()` `T`s.
            unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), block.payload_as_ptr::<T>(), data.len()); }
        }
        block
    }

    /// Untyped array (element type unknown) — for Rust-synthesized arrays.
    #[inline]
    pub fn new(heap: &dyn MagrGC, elems: Vec<Value>) -> Self {
        let (block, len) = Self::alloc_boxed(heap, elems);
        Self { element_type: Arc::from(""), backing: ArrayBacking::Boxed { block, len } }
    }
    /// Array with a known element type (from `ArrayNew` / `ArrayNewLit`).
    /// **Step 1b-ii**: primitive element types get a packed value-type backing
    /// (C# model); everything else stays `Boxed`. Unknown/FQN element types fall
    /// back to `Boxed` (safe — no packing, correct behaviour).
    #[inline]
    pub fn typed(heap: &dyn MagrGC, element_type: &str, elems: Vec<Value>) -> Self {
        Self { element_type: Arc::from(element_type), backing: Self::pack_backing(heap, element_type, elems) }
    }

    /// add-escape-analysis-stack-alloc / unify-gc-heap PR-3: build a **stack array**
    /// (escape-analysis non-escaping array) whose elements live in a plain arena-owned
    /// `Vec` — **no GC allocation** (the whole point of stack-alloc). Stored in
    /// `ctx.stack_arena`; its `Value` elements are scanned as GC roots. Unlike
    /// [`Self::typed`], this needs no heap and never packs (short-lived frame-local
    /// storage; boxed `Value`s keep the interp read/write path uniform).
    #[inline]
    pub fn stack_typed(element_type: &str, elems: Vec<Value>) -> Self {
        Self { element_type: Arc::from(element_type), backing: ArrayBacking::StackVec(elems) }
    }

    /// FFI return fast-path (packed-primitive-arrays Step 3): build a `byte[]`
    /// straight from an owned `Vec<u8>` — no per-byte `Value::I64` boxing, no
    /// re-pack scan. The mirror of `as_bytes()` on the ingest side. This is the
    /// "简化 extern call" return path: native call → `&[u8]` → `byte[]` directly.
    pub fn from_bytes(heap: &dyn MagrGC, bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let block = Self::alloc_packed(heap, &bytes);
        Self { element_type: Arc::from("byte"), backing: ArrayBacking::Bytes { block, len } }
    }

    /// add-struct-array-codegen (P3b follow-up): build a value-struct array `Point[len]`
    /// with `StructBytes` backing — `len` elements packed back-to-back (`len*elem_size`
    /// bytes, zero-initialized = default struct) + a `Null`-filled reference side-table
    /// (`len*ref_count`). `layout` = the element struct type's byte+reference layout.
    /// Element access goes through a `Value::StructRefHeap` handle (see `array_get`).
    pub fn struct_backed(heap: &dyn MagrGC, element_type: &str, len: usize, layout: std::sync::Arc<StructTypeLayout>) -> Self {
        let elem_size = layout.size;
        let ref_count = layout.ref_count();
        // bytes: POD packed struct bytes, zero-init = default struct (allocator zeroes).
        let bytes = heap.alloc_var_block(len * elem_size, BlockType::ArrayStruct);
        // refs: reference side-table, Null-initialized (zero-init would be I64(0), wrong default).
        let refs = Self::alloc_values_null(heap, len * ref_count);
        Self {
            element_type: Arc::from(element_type),
            backing: ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout },
        }
    }

    /// Select a packed value-type backing for a primitive `element_type`,
    /// unboxing `elems` into it. Conservative + sign-safe: only widths that
    /// round-trip losslessly through `get_boxed`/`set_boxed` are packed.
    fn pack_backing(heap: &dyn MagrGC, element_type: &str, elems: Vec<Value>) -> ArrayBacking {
        match element_type {
            // byte[] → contiguous u8: the FFI zero-copy + 24× memory win.
            "byte" | "u8" => {
                let v: Vec<u8> = elems.iter().map(|x| if let Value::I64(n) = x { *n as u8 } else { 0 }).collect();
                ArrayBacking::Bytes { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            "char" => {
                let v: Vec<char> = elems.iter().map(|x| if let Value::Char(c) = x { *c } else { '\0' }).collect();
                ArrayBacking::Chars { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            "bool" => {
                let v: Vec<bool> = elems.iter().map(|x| matches!(x, Value::Bool(true))).collect();
                ArrayBacking::Bool { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            // fits i32 signed range (i8/i16/i32 and u16 ≤ 65535).
            "sbyte" | "i8" | "short" | "i16" | "int" | "i32" | "ushort" | "u16" => {
                let v: Vec<i32> = elems.iter().map(|x| if let Value::I64(n) = x { *n as i32 } else { 0 }).collect();
                ArrayBacking::I32 { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            // 64-bit (uint/u32 fit i64; u64 keeps existing i64-store semantics).
            "long" | "i64" | "uint" | "u32" | "ulong" | "u64" | "isize" | "usize" => {
                let v: Vec<i64> = elems.iter().map(|x| if let Value::I64(n) = x { *n } else { 0 }).collect();
                ArrayBacking::I64 { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            "double" | "float" | "f32" | "f64" => {
                let v: Vec<f64> = elems.iter().map(|x| if let Value::F64(f) = x { *f } else { 0.0 }).collect();
                ArrayBacking::F64 { block: Self::alloc_packed(heap, &v), len: v.len() }
            }
            // object / string / nested arrays / structs / unknown FQN → reference array.
            _ => {
                let (block, len) = Self::alloc_boxed(heap, elems);
                ArrayBacking::Boxed { block, len }
            }
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match &self.backing {
            ArrayBacking::Boxed { len, .. }
            | ArrayBacking::Bool { len, .. }
            | ArrayBacking::Bytes { len, .. }
            | ArrayBacking::I32 { len, .. }
            | ArrayBacking::I64 { len, .. }
            | ArrayBacking::Chars { len, .. }
            | ArrayBacking::F64 { len, .. }
            | ArrayBacking::StructBytes { len, .. } => *len,
            ArrayBacking::StackVec(v) => v.len(),
        }
    }
    #[inline]
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    /// Bounds-checked read as owned `Value` (packed-safe `Vec::get` analogue).
    #[inline]
    pub fn get(&self, i: usize) -> Option<Value> {
        if i < self.len() { Some(self.get_boxed(i)) } else { None }
    }
    #[inline]
    pub fn first(&self) -> Option<Value> { self.get(0) }

    /// Read element `i` as a `Value` (boxes packed primitives). Caller ensures
    /// `i < len()`. SAFETY of block reads: see [`Self::slice_of`] (held under `&self`
    /// = shared region lock).
    #[inline]
    pub fn get_boxed(&self, i: usize) -> Value {
        match &self.backing {
            // SAFETY (each arm): shared borrow of a live block of exactly `len` `T`s.
            ArrayBacking::Boxed { block, len } => (unsafe { Self::slice_of::<Value>(block, *len) })[i].clone(),
            ArrayBacking::Bool { block, len }  => Value::Bool((unsafe { Self::slice_of::<bool>(block, *len) })[i]),
            ArrayBacking::Bytes { block, len } => Value::I64((unsafe { Self::slice_of::<u8>(block, *len) })[i] as i64),
            ArrayBacking::I32 { block, len }   => Value::I64((unsafe { Self::slice_of::<i32>(block, *len) })[i] as i64),
            ArrayBacking::I64 { block, len }   => Value::I64((unsafe { Self::slice_of::<i64>(block, *len) })[i]),
            ArrayBacking::Chars { block, len } => Value::Char((unsafe { Self::slice_of::<char>(block, *len) })[i]),
            ArrayBacking::F64 { block, len }   => Value::F64((unsafe { Self::slice_of::<f64>(block, *len) })[i]),
            // Escape-analysis stack array: boxed Values inline in the arena Vec.
            ArrayBacking::StackVec(v) => v[i].clone(),
            // add-struct-heap-inline (P3b): reading a struct[] element as a generic
            // `Value` yields a **boxed copy** (value semantics — the read is a snapshot;
            // mutating the box does not touch the array). In-place `arr[i].x = v` /
            // `arr[i].x` leaf access instead goes through a `Value::StructRefHeap`
            // handle at the exec layer (it has the array `GcRef`; `get_boxed` does not).
            // add-boxed-struct-identity (P4b): boxing a struct[] element now requires a
            // heap allocation (the box is a shared `ScriptObject`), which this
            // `&self` accessor cannot do. The value path never reaches here — interp
            // `array_get` + jit array-get materialize a `StructRefHeap` handle for
            // `StructBytes` backing (see exec_array.rs / jit/helpers/array.rs), and any
            // real struct→object boxing goes through `__box_struct` (heap-aware). This
            // arm is an invariant guard; if a materialization path ever needs a boxed
            // struct[] element, route it through a `ctx`-carrying helper, not `get_boxed`.
            ArrayBacking::StructBytes { .. } => {
                debug_assert!(false,
                    "get_boxed on a StructBytes backing: struct[] element boxing needs a heap-aware path, not get_boxed");
                Value::Null
            }
        }
    }
    /// Write `Value` into element `i` (unboxes into packed primitives). Caller
    /// ensures `i < len()`. SAFETY of block writes: [`Self::slice_of_mut`] (held under
    /// `&mut self` = exclusive region lock).
    #[inline]
    pub fn set_boxed(&mut self, i: usize, val: Value) {
        match &mut self.backing {
            // SAFETY (each arm): exclusive borrow of a live block of exactly `len` `T`s.
            ArrayBacking::Boxed { block, len } => { let s = unsafe { Self::slice_of_mut::<Value>(block, *len) }; s[i] = val; }
            ArrayBacking::Bool { block, len }  => { let s = unsafe { Self::slice_of_mut::<bool>(block, *len) }; s[i] = matches!(val, Value::Bool(true)); }
            ArrayBacking::Bytes { block, len } => { let s = unsafe { Self::slice_of_mut::<u8>(block, *len) }; s[i] = if let Value::I64(n) = val { n as u8 } else { 0 }; }
            ArrayBacking::I32 { block, len }   => { let s = unsafe { Self::slice_of_mut::<i32>(block, *len) }; s[i] = if let Value::I64(n) = val { n as i32 } else { 0 }; }
            ArrayBacking::I64 { block, len }   => { let s = unsafe { Self::slice_of_mut::<i64>(block, *len) }; s[i] = if let Value::I64(n) = val { n } else { 0 }; }
            ArrayBacking::Chars { block, len } => { let s = unsafe { Self::slice_of_mut::<char>(block, *len) }; s[i] = if let Value::Char(c) = val { c } else { '\0' }; }
            ArrayBacking::F64 { block, len }   => { let s = unsafe { Self::slice_of_mut::<f64>(block, *len) }; s[i] = if let Value::F64(f) = val { f } else { 0.0 }; }
            // Escape-analysis stack array: store the boxed Value directly in the arena Vec.
            ArrayBacking::StackVec(v) => v[i] = val,
            // add-struct-heap-inline (P3b): writing a whole struct[] element from a
            // **boxed** source copies its bytes + reference leaves into the element slot.
            // A frame-scoped `StructRef` source needs `ctx.struct_arena` → handled at the
            // exec-layer `ArraySet` (this generic setter only sees `&mut self`).
            ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } => {
                if let Value::BoxedStruct(b) = &val {
                    // add-boxed-struct-identity (P4b): read the source box's blob out of
                    // its shared `ScriptObject` (borrow needs no `ctx`).
                    let bo = b.borrow();
                    let rc = layout.ref_count();
                    let bstart = i * *elem_size;
                    let n = bo.bytes.len().min(*elem_size);
                    // SAFETY: exclusive block payloads: `bytes` holds `len*elem_size` u8,
                    // `refs` holds `len*rc` Values.
                    let bslice = unsafe { Self::slice_of_mut::<u8>(bytes, *len * *elem_size) };
                    bslice[bstart..bstart + n].copy_from_slice(&bo.bytes[..n]);
                    let rslice = unsafe { Self::slice_of_mut::<Value>(refs, *len * rc) };
                    let rn = bo.refs.len().min(rc);
                    for k in 0..rn { rslice[i * rc + k] = bo.refs[k].clone(); }
                } else {
                    debug_assert!(false,
                        "struct[] set_boxed needs a BoxedStruct source (StructRef → exec-level ArraySet), got {val:?}");
                }
            }
        }
    }

    /// unify-gc-heap PR-3: copy one `struct[]` element's packed bytes + reference leaves
    /// into slot `i` of a `StructBytes` backing. Used by the exec-layer struct-array literal
    /// packer (`pack_struct_elem`), which resolves `BoxedStruct` / `StructRef` sources into
    /// `(bytes, refs)` first (it can't reach the private block accessors). No-op on other
    /// backings. Caller holds `&mut self` = exclusive block access.
    pub fn write_struct_elem(&mut self, i: usize, src_bytes: &[u8], src_refs: &[Value]) {
        if let ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } = &mut self.backing {
            let rc = layout.ref_count();
            let bstart = i * *elem_size;
            let n = src_bytes.len().min(*elem_size);
            // SAFETY: exclusive block payloads: `bytes` = `len*elem_size` u8, `refs` = `len*rc` Values.
            let bslice = unsafe { Self::slice_of_mut::<u8>(bytes, *len * *elem_size) };
            bslice[bstart..bstart + n].copy_from_slice(&src_bytes[..n]);
            let rslice = unsafe { Self::slice_of_mut::<Value>(refs, *len * rc) };
            let rn = src_refs.len().min(rc);
            for k in 0..rn { rslice[i * rc + k] = src_refs[k].clone(); }
        }
    }

    /// unify-gc-heap PR-3: the `StructBytes` element type layout (element `elem_size`
    /// = `layout.size`, `ref_count`, `ref_index`). `None` for non-struct[] backings.
    /// Returns a cloned `Arc` so the caller can drop the shared borrow before taking a
    /// `&mut` block slice (`struct_bytes_mut` / `struct_refs_mut`).
    #[inline]
    pub fn struct_layout(&self) -> Option<std::sync::Arc<StructTypeLayout>> {
        match &self.backing {
            ArrayBacking::StructBytes { layout, .. } => Some(layout.clone()),
            _ => None,
        }
    }
    /// unify-gc-heap PR-3: the whole packed-bytes region of a `StructBytes` array
    /// (`len*elem_size` bytes) for struct[] leaf prim decode. `None` otherwise.
    #[inline]
    pub fn struct_bytes(&self) -> Option<&[u8]> {
        match &self.backing {
            // SAFETY: shared borrow of a live ArrayStruct block of `len*elem_size` bytes.
            ArrayBacking::StructBytes { bytes, len, elem_size, .. } =>
                Some(unsafe { Self::slice_of::<u8>(bytes, *len * *elem_size) }),
            _ => None,
        }
    }
    /// Mutable packed-bytes region of a `StructBytes` array (struct[] leaf prim encode).
    #[inline]
    pub fn struct_bytes_mut(&mut self) -> Option<&mut [u8]> {
        match &mut self.backing {
            // SAFETY: exclusive borrow of a live ArrayStruct block of `len*elem_size` bytes.
            ArrayBacking::StructBytes { bytes, len, elem_size, .. } =>
                Some(unsafe { Self::slice_of_mut::<u8>(bytes, *len * *elem_size) }),
            _ => None,
        }
    }
    /// Mutable reference side-table of a `StructBytes` array (`len*ref_count` Values) —
    /// struct[] reference-leaf writes. `None` otherwise. (Reads use `gc_refs()`.)
    #[inline]
    pub fn struct_refs_mut(&mut self) -> Option<&mut [Value]> {
        match &mut self.backing {
            // SAFETY: exclusive borrow of a live ArrayValue block of `len*ref_count` Values.
            ArrayBacking::StructBytes { refs, len, layout, .. } =>
                Some(unsafe { Self::slice_of_mut::<Value>(refs, *len * layout.ref_count()) }),
            _ => None,
        }
    }

    /// Materialise all elements as a `Vec<Value>` (for sites needing a boxed
    /// snapshot — reflection, conversions). Boxes packed primitives.
    pub fn to_boxed_vec(&self) -> Vec<Value> {
        (0..self.len()).map(|i| self.get_boxed(i)).collect()
    }

    /// add-struct-heap-inline (P3b): every heap reference this array holds, for the
    /// GC mark traversal. A `Boxed` array's elements are all refs; a `StructBytes`
    /// (value-struct) array's refs are the inline elements' reference leaves in the
    /// side-table (packed primitives in `bytes` hold none). Packed-primitive arrays
    /// return `&[]`. The returned slice borrows the backing block for `&self`'s
    /// lifetime (block outlives the header).
    #[inline]
    pub fn gc_refs(&self) -> &[Value] {
        match &self.backing {
            // SAFETY: shared borrow of a live ArrayValue block of `len` Values.
            ArrayBacking::Boxed { block, len } => unsafe { Self::slice_of::<Value>(block, *len) },
            // SAFETY: shared borrow of a live ArrayValue block of `len*ref_count` Values.
            ArrayBacking::StructBytes { refs, len, layout, .. } =>
                unsafe { Self::slice_of::<Value>(refs, *len * layout.ref_count()) },
            // Stack array: elements are boxed Values in the arena Vec, scanned as roots.
            ArrayBacking::StackVec(v) => v,
            _ => &[],
        }
    }

    /// unify-gc-heap PR-3: mark this array's backing block(s) live during the GC
    /// mark phase. Called from `Value::trace_children`'s array-borrowing arms right
    /// after the `ArrayObj` header (region_array) is marked — without this the
    /// element blocks in `region_var` would be swept out from under a live array.
    #[inline]
    pub fn mark_backing(&self) {
        match &self.backing {
            ArrayBacking::Boxed { block, .. }
            | ArrayBacking::Bool { block, .. }
            | ArrayBacking::Bytes { block, .. }
            | ArrayBacking::I32 { block, .. }
            | ArrayBacking::I64 { block, .. }
            | ArrayBacking::Chars { block, .. }
            | ArrayBacking::F64 { block, .. } => { block.mark(); }
            ArrayBacking::StructBytes { bytes, refs, .. } => { bytes.mark(); refs.mark(); }
            // Stack array: no GC block — the arena Vec is scanned as a root, nothing to mark.
            ArrayBacking::StackVec(_) => {}
        }
    }

    /// unify-gc-heap PR-3: an independent heap-allocated copy (value-semantic array
    /// clone — `__array_clone`). Allocates fresh backing block(s) in `heap` and copies
    /// element data in (cloning `Value`s), so the copy shares nothing mutable with the
    /// original. Replaces the removed `#[derive(Clone)]` (which would have aliased the
    /// backing block).
    pub fn deep_copy(&self, heap: &dyn MagrGC) -> Self {
        let backing = match &self.backing {
            ArrayBacking::Boxed { block, len } => {
                let src = unsafe { Self::slice_of::<Value>(block, *len) };
                ArrayBacking::Boxed { block: Self::alloc_values_clone(heap, src), len: *len }
            }
            ArrayBacking::Bool { block, len } =>
                ArrayBacking::Bool { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<bool>(block, *len) }), len: *len },
            ArrayBacking::Bytes { block, len } =>
                ArrayBacking::Bytes { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<u8>(block, *len) }), len: *len },
            ArrayBacking::I32 { block, len } =>
                ArrayBacking::I32 { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<i32>(block, *len) }), len: *len },
            ArrayBacking::I64 { block, len } =>
                ArrayBacking::I64 { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<i64>(block, *len) }), len: *len },
            ArrayBacking::Chars { block, len } =>
                ArrayBacking::Chars { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<char>(block, *len) }), len: *len },
            ArrayBacking::F64 { block, len } =>
                ArrayBacking::F64 { block: Self::alloc_packed(heap, unsafe { Self::slice_of::<f64>(block, *len) }), len: *len },
            ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } => {
                let rc = layout.ref_count();
                let bsrc = unsafe { Self::slice_of::<u8>(bytes, *len * *elem_size) };
                let rsrc = unsafe { Self::slice_of::<Value>(refs, *len * rc) };
                ArrayBacking::StructBytes {
                    elem_size: *elem_size,
                    len: *len,
                    bytes: Self::alloc_packed(heap, bsrc),
                    refs: Self::alloc_values_clone(heap, rsrc),
                    layout: layout.clone(),
                }
            }
            // A stack array being deep-copied escapes into the heap → materialize its boxed
            // elements into a fresh GC `Boxed` block (never hit in practice: `__array_clone`
            // only sees heap `Value::Array`, but keep the copy heap-correct if it ever does).
            ArrayBacking::StackVec(v) => {
                let (block, len) = Self::alloc_boxed(heap, v.clone());
                ArrayBacking::Boxed { block, len }
            }
        };
        Self { element_type: self.element_type.clone(), backing }
    }

    /// Zero-copy packed byte slice for FFI (`Some` iff `byte[]`). Step 3 uses
    /// this to hand native code a contiguous `&[u8]` — no per-byte marshal.
    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match &self.backing {
            // SAFETY: shared borrow of a live ArrayPrim block of `len` bytes.
            ArrayBacking::Bytes { block, len } => Some(unsafe { Self::slice_of::<u8>(block, *len) }),
            _ => None,
        }
    }

    /// JIT packed-numeric fast path: `I32`/`I64`/`F64` backings are contiguous
    /// fixed-width slots (4 / 8 / 8 bytes) the JIT can index with a native
    /// stride-N load/store — no 24-byte `Value` round-trip, no per-element tag.
    /// Pairs with [`Self::packed_elem_width`]: the ptr is the buffer base, the
    /// width tells the JIT the slot size (4 → `int[]` sign-extends into the i64
    /// payload; 8 → raw `long[]`/`double[]` copy). `None` for `Boxed`/`Bytes`/
    /// `Bool`/`Chars` — the JIT set-path detects width 0 and falls back to the
    /// `jit_array_set` helper, so those backings never index off this ptr.
    ///
    /// unify-gc-heap PR-3: the ptr is now the GC block's inline payload (non-moving,
    /// fixed-size) instead of a `Vec` buffer — the JIT may cache it across the
    /// function (blocks don't relocate; A' is a non-moving allocator).
    #[inline]
    pub fn packed_num_ptr(&self) -> Option<*const u8> {
        match &self.backing {
            // SAFETY: block payload ptr from the raw header (D8); JIT reads `len` slots only.
            ArrayBacking::I32 { block, .. }
            | ArrayBacking::I64 { block, .. }
            | ArrayBacking::F64 { block, .. }
            // jit-inline-char-arrays: `char` is a 4-byte scalar (Rust `char` == u32);
            // the JIT loads it width-4 and boxes into `Value::Char`.
            | ArrayBacking::Chars { block, .. } => {
                debug_assert!(block.is_live(), "unify-gc-heap PR-3: packed_num_ptr on a stale/tombstoned block");
                Some(unsafe { block.payload_as_ptr::<u8>() } as *const u8)
            }
            _ => None,
        }
    }

    /// Packed slot width in bytes for the JIT fast path: 4 (`I32`/`Chars`), 8
    /// (`I64`/`F64`), or 0 for a non-packed backing (`Boxed`/`Bytes`/`Bool`).
    /// The **runtime** authority the JIT ArraySet inline consults so a narrowing
    /// store (`int[i] = <i64 value>`) writes the right slot size rather than
    /// trusting the value register's width. Width 0 → route to the helper.
    #[inline]
    pub fn packed_elem_width(&self) -> i64 {
        match &self.backing {
            ArrayBacking::I32 { .. } | ArrayBacking::Chars { .. } => 4,
            ArrayBacking::I64 { .. } | ArrayBacking::F64 { .. } => 8,
            _ => 0,
        }
    }

    /// Iterate all elements as owned `Value`s (boxes packed primitives).
    /// Packed-safe replacement for the old `Deref`→`Vec<Value>` `.iter()`.
    #[inline]
    pub fn iter_boxed(&self) -> impl Iterator<Item = Value> + '_ {
        (0..self.len()).map(move |i| self.get_boxed(i))
    }

    /// Heap bytes for element storage (`len × sizeof(element)`) — the packed-array
    /// memory win shows up here (byte[] 1B vs Boxed 24B/elem). unify-gc-heap PR-3:
    /// counts the GC block payload(s); arrays are fixed-size so `len == capacity`.
    #[inline]
    pub fn elem_storage_bytes(&self) -> usize {
        use std::mem::size_of;
        match &self.backing {
            ArrayBacking::Boxed { len, .. } => len * size_of::<Value>(),
            ArrayBacking::Bool { len, .. }  => *len,
            ArrayBacking::Bytes { len, .. } => *len,
            ArrayBacking::I32 { len, .. }   => len * 4,
            ArrayBacking::I64 { len, .. }   => len * 8,
            ArrayBacking::Chars { len, .. } => len * 4,
            ArrayBacking::F64 { len, .. }   => len * 8,
            // Packed struct bytes + the reference side-table (16B/handle in a Value).
            ArrayBacking::StructBytes { elem_size, len, layout, .. } =>
                len * elem_size + len * layout.ref_count() * size_of::<Value>(),
            ArrayBacking::StackVec(v) => v.len() * size_of::<Value>(),
        }
    }
}

#[cfg(test)]
impl ArrayObj {
    /// Test-only: a `Boxed` array whose element block is a **leaked** standalone GC block
    /// (never in a region, never swept) — for heap-less unit tests that need a heap-backed
    /// array without wiring an `ArcMagrGC`. Mirrors `VarGcRef::leak_for_test` (used by these
    /// same tests for closures). Never run under Miri's leak checker.
    pub(crate) fn new_leaked(elems: Vec<Value>) -> Self {
        let len = elems.len();
        let block = VarGcRef::leak_block_for_test(len * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh leaked block sized for `len` Values; move each in over the POD zero.
        let base = unsafe { block.payload_as_ptr::<Value>() };
        for (i, v) in elems.into_iter().enumerate() { unsafe { base.add(i).write(v); } }
        Self { element_type: Arc::from(""), backing: ArrayBacking::Boxed { block, len } }
    }

    /// Test-only: a zero-/`Null`-initialized `StructBytes` array with **leaked** byte + ref
    /// blocks (elements written via `write_struct_elem`). For heap-less struct[] unit tests.
    pub(crate) fn struct_backed_leaked(element_type: &str, len: usize, layout: std::sync::Arc<StructTypeLayout>) -> Self {
        let elem_size = layout.size;
        let rc = layout.ref_count();
        let bytes = VarGcRef::leak_block_for_test(len * elem_size, BlockType::ArrayStruct);
        let refs = VarGcRef::leak_block_for_test(len * rc * std::mem::size_of::<Value>(), BlockType::ArrayValue);
        // SAFETY: fresh leaked ref block sized for `len*rc` Values; Null-init each slot.
        let rbase = unsafe { refs.payload_as_ptr::<Value>() };
        for i in 0..len * rc { unsafe { rbase.add(i).write(Value::Null); } }
        Self { element_type: Arc::from(element_type), backing: ArrayBacking::StructBytes { elem_size, len, bytes, refs, layout } }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, u8)]
pub enum Value {
    I64(i64)        = 0,
    F64(f64)        = 1,
    Bool(bool)      = 2,
    Char(char)      = 3,
    /// Immutable string primitive.  `s.Length` → virtual field dispatch in FieldGet.
    ///
    /// review.md C1+C3 (2026-05-27): `Arc<str>` instead of `String`. Saves
    /// 8 B/instance (Arc<str> = 16 B vs String = 24 B; no `cap` word) AND
    /// turns clone from O(n) byte copy into O(1) atomic refcount — the
    /// hot-path win for string-heavy interp / format / concat loops.
    /// Arc not Rc because `Value: Send + Sync` (see
    /// `gc/arc_heap_tests/send_sync.rs::assert_send_sync::<Value>()`).
    Str(Str)                    = 4,
    Null                        = 5,
    /// Heap-allocated dynamic array with reference semantics.
    /// add-reflection-array-element-type (2026-06-11): payload is `ArrayObj`
    /// (element type name + elems) instead of a bare `Vec<Value>`, so the array
    /// carries its element type at runtime (non-erased reflection). `ArrayObj`
    /// derefs to the element `Vec<Value>`, so element access is unchanged.
    Array(GcRef<ArrayObj>)      = 6,
    /// Heap-allocated managed class instance with reference semantics.
    Object(GcRef<ScriptObject>) = 7,
    /// Spec C4 — borrowed view of a `String` / `Array<u8>` for native FFI.
    /// Created by `PinPtr`, released by `UnpinPtr`. The `ptr` is an
    /// untyped raw address — consumers must know the source `kind` to
    /// interpret it. Field access (`.ptr` / `.len`) goes through the
    /// regular `FieldGet` instruction.
    ///
    /// review.md C1 step 1 (2026-05-27): payload boxed to shrink the
    /// inline `Value` size — `PinnedView` is created on the rare
    /// `PinPtr` opcode and immediately consumed by the next native
    /// call, so the heap-alloc cost is dominated by the FFI it enables.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `PinnedViewData`).
    PinnedView { idx: u32, frame_id: u32 } = 8,
    /// Function reference value. Currently used by L2 no-capture lambda
    /// literals (see docs/design/language/closure.md §6). Indirect call dispatches
    /// to the named function in the loaded module.
    ///
    /// review.md C1 chunk 2 (2026-05-27): `Box<str>` instead of `String`.
    /// Saves 8 B/instance (Box<str> = 16 B vs String = 24 B; no `cap` word).
    /// FuncRef names are write-once at creation and read-only thereafter
    /// (immutable identity → no append/grow operation needed).
    ///
    /// unify-object-byte-layout PR-5 (2026-08-15): `Str` (8 B thin pointer)
    /// instead of `Box<str>` (16 B fat pointer). `Box<str>` was the *last*
    /// 16 B payload keeping `Value` at 24 B; swapping it for the vstr thin
    /// pointer drops the max payload to 8 B → `Value` = 16 B (see the
    /// `size_of::<Value>() == 16` static assert below). Length is read from
    /// the `StrHeader`, so `name.len()` stays O(1).
    FuncRef(Str) = 9,
    /// L3 capturing closure value: pairs a heap-allocated env (Vec<Value>)
    /// with the lifted function's qualified name. CallIndirect on a Closure
    /// passes `env` as the callee's first implicit parameter and copies user
    /// args after it. See docs/design/language/closure.md §6 + impl-closure-l3-core.
    ///
    /// review.md C1 chunk 5 (2026-05-27): payload boxed (the last and
    /// biggest cold-path variant — 40 B inline = GcRef(16 B) + String(24 B)).
    /// Boxing drops Value enum to ~24 B; capturing closures pay one heap
    /// alloc per `MkClos` but that's dwarfed by the env's own GC alloc.
    ///
    /// unify-gc-heap PR-2 (2026-08-15): `VarGcRef` (8B) instead of `Box<ClosureData>`. The
    /// `ClosureData` now lives in the GC variable-length region (`region_var`) — a single GC
    /// heap instead of a `Box` outside GC. `ClosureData` is immutable after creation, so the
    /// block needs no per-entry lock; access is a lock-free `&ClosureData` (kept alive by
    /// reachability, like `GcRef`). Cloning a closure `Value` now shares the same heap closure
    /// (handle copy) instead of deep-cloning the box. See `Value::closure_data`.
    Closure(VarGcRef) = 10,
    /// 2026-05-02 impl-closure-l3-escape-stack: 栈分配的 capturing closure 值。
    /// `env_idx` 索引创建该 closure 的 frame 的 `env_arena: Vec<Vec<Value>>`；
    /// CallIndirect 时由 dispatch 端通过当前帧的 arena 解 env。compiler 经
    /// escape 分析证明 closure 不离开创建 frame 时才发射该 variant；逃逸
    /// 场景仍走 `Value::Closure`。详见
    /// `docs/spec/archive/2026-05-02-impl-closure-l3-escape-stack/`。
    ///
    /// review.md C1 chunk 3 (2026-05-27): payload boxed to shrink the
    /// inline `Value` size — StackClosure is created on the rare
    /// non-escaping closure path and only consumed by the next
    /// `CallIndirect` before the creating frame returns.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `StackClosureData`).
    StackClosure { idx: u32, frame_id: u32 } = 11,
    /// Spec impl-ref-out-in-runtime: `ref` / `out` / `in` 参数运行时表达。
    /// 持有该 Value 的寄存器在 frame.get/set 时被透明 deref（单点 dispatch，
    /// 见 `interp/mod.rs::Frame::get`）。引用永远不离开调用栈帧（前置 spec
    /// design Decision 9 + R1），因此 Stack kind 的 frame_idx 不会 stale。
    ///
    /// review.md C1 chunk 4 (2026-05-27): payload boxed because RefKind
    /// is 32 B (Field variant) — biggest cold-path payload after
    /// Closure. Refs only live in registers for a single call's
    /// duration, so the box alloc is a tiny fraction of the call cost.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `RefKind`).
    Ref { idx: u32, frame_id: u32 } = 12,
    // discriminant 13 retired by unify Phase 2 R3（装箱统一）：基元装箱不再走
    // `Value::Boxed(Box<BoxedPrim>)`——整数标量装进堆 `ScriptObject` 的 `struct_bytes` 并以
    // `Value::BoxedStruct` 承载（与 struct 装箱同一模型 + 引用身份）。判别号 13 留空（14-18 号不
    // 重编，`#[repr(C,u8)]` 显式判别 + JIT 原始布局不受影响）。
    /// add-escape-analysis-stack-alloc: 逃逸分析证明不逃逸的对象，interp 在**每线程
    /// context 的栈 arena**（`VmContext::stack_obj_arena`）里分配，绕过 GC、随创建帧
    /// 退出 LIFO 截断释放。句柄（非堆指针）：
    ///   * `idx` — arena 内条目下标（`ctx.stack_obj_arena[idx]`，任何帧都能直取——ctor
    ///     子帧因此天然可解 `this`，无需跨帧机制）。
    ///   * `frame_id` — 创建帧的单调 id（诊断）：解引用时校验 arena 槽的 frame_id 与之
    ///     相符 + idx 在界内，不符/越界 = 逃逸分析误判、栈句柄活过创建帧 → 明确报错
    ///     （而非静默 use-after-free）。帧退出 truncate 后槽被后续帧复用 → frame_id 不符即抓。
    /// JIT 从不产生本变体（D2：JIT 忽略 stack_alloc、照常堆分配），故 JIT 值路径永不遇到。
    StackObject { idx: u32, frame_id: u32 } = 14,
    /// add-escape-analysis-stack-alloc: 不逃逸数组的栈 arena 句柄（`VmContext::stack_arr_arena`）。
    /// 语义同 `StackObject`。
    StackArray { idx: u32, frame_id: u32 } = 15,
    /// add-struct-value-semantics Phase A: blob 值类型（多字段 struct）句柄。`idx` 索引 per-context
    /// 字节 arena（`VmContext::struct_arena`）里的 blob 条目；`frame_id` = 创建帧单调 id（staleness
    /// guard，同 `StackObject`）。未装箱 struct 值以此句柄在寄存器间流转；字节 blob 存 arena。
    /// blob 内引用叶子由 arena 的 root scanner 按 TypeDesc 引用位图扫描（trace_children 视为叶子，
    /// 避免双计）。
    StructRef { idx: u32, frame_id: u32 } = 16,
    /// add-struct-object-boxing (PR2a): 装箱的 blob 值 struct。`object o = someStruct` 擦除到
    /// `object`/接口时，把帧作用域 arena blob **拷进堆稳定表示**（脱离帧生命周期，修裸拷 `StructRef`
    /// 句柄逃逸帧的 use-after-free）。载荷拥有 `bytes`（基元叶子字节快照）+ `refs`（引用叶子作真 Value
    /// → GC 扫描 + 内存安全，镜像 `struct_arena::StructSlot` 去掉 `frame_id`）+ `type_name`（供
    /// `GetType`/`is`/`as`）。装箱经 `__box_struct` builtin（复用 Builtin opcode，无格式 bump）；`(P)o`
    /// 拆箱把 blob 拷回当前帧 arena `StructRef`。unboxed struct 仍无 vtable——对象协议由本变体的 VM 分支
    /// （身份）+ 编译器合成值方法（Equals 等，PR2b）承载。
    ///
    /// **add-boxed-struct-identity (P4b, 路 B2)**: 装箱 = 一个 struct 类型的共享 `ScriptObject`
    /// （`type_desc.is_struct()`，struct blob 存进对象的 `struct_bytes`/`struct_refs`，`slots` 空）。
    /// 载荷从值语义 `Box<BoxedStructData>` 改为**共享堆句柄** `GcRef<ScriptObject>` → 对齐 C# 引用身份
    /// （`object b = a` 别名同盒、反射 `SetValue` 写穿、传参改盒可见）。复用 `region_object` + 全部 GC 机制
    /// （GC 里与 `Value::Object` 同路标记/追踪；仅 is/as/GetType/vcall/Equals 保持 boxed 值类型特判）。
    BoxedStruct(GcRef<ScriptObject>) = 17,
    /// add-struct-heap-inline (P3b, route α): a transient handle to a value-struct
    /// **inlined in a heap array element** (`arr[i]`). Unlike an object field (whose
    /// composite byte offset the compiler bakes → base = `Value::Object`), a struct[]
    /// element's byte offset depends on the runtime index, so `arr[i]` materializes
    /// this handle and a following `StructFieldGetPrim/SetPrim` reads/writes a leaf of
    /// element `index` (routing byte/ref access through the array's `StructBytes`
    /// backing). Payload boxed (8 B pointer) so `Value` stays 24 B. GC follows `arr`.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `StructArrayElem`).
    StructRefHeap { idx: u32, frame_id: u32 } = 18,
}

// unify-object-byte-layout PR-5 (2026-08-15): `Value` is the interpreter's
// register-file cell and the JIT strides its register file by
// `size_of::<Value>()` (`jit/translate.rs` `VALUE_STRIDE`/`STRIDE`), so its
// size is an ABI contract, not an incidental detail. After PR-3 (GcRef 8 B) +
// PR-4 (Str 8 B) + this PR (FuncRef → Str 8 B), every payload is ≤ 8 B, so
// `#[repr(C, u8)]` gives tag(1 B, padded to 8) + 8 B payload = 16 B. This
// assert fails to compile the moment a payload grows past 8 B (e.g. a new
// fat-pointer / two-word variant), forcing it to be boxed before it can
// silently grow the register file / JIT stride back to 24 B.
const _: () = assert!(std::mem::size_of::<Value>() == 16);

/// add-struct-heap-inline (P3b): payload of [`Value::StructRefHeap`] — a value-struct
/// array element identity (`arr[index]`). Holds the array `GcRef` (so the handle keeps
/// the array alive + GC can reach the element's reference leaves) + the element index.
#[derive(Debug, Clone)]
pub struct StructArrayElem {
    pub arr: GcRef<ArrayObj>,
    pub index: u32,
}

// add-boxed-struct-identity (P4b, 路 B2): `BoxedStructData` 已删——装箱 struct 的 blob 现内联在其
// 共享 `ScriptObject` 的 `struct_bytes`/`struct_refs`（`type_desc.is_struct()` 的对象）。装箱经
// `corelib::convert::builtin_box_struct`（alloc struct 类型 ScriptObject）；拆箱经
// `interp::exec_struct::unbox_struct`（读对象 struct_bytes/refs → arena StructRef）。

// add-primitive-value-boxing → unify Phase 2 R3: `BoxedPrim` 已删——基元装箱统一到堆
// `ScriptObject`（整数标量存 `struct_bytes`）+ `Value::BoxedStruct`，见
// `ScriptObject::boxed_prim_i64` / `corelib::convert::box_prim_to_heap`。

/// Spec impl-ref-out-in-runtime: 描述 `Value::Ref` 指向的底层位置类型。
#[derive(Debug, Clone)]
pub enum RefKind {
    /// 指向 caller 调用栈第 `frame_idx` 层 frame 的 reg[`slot`]。
    /// `frame_idx` 是 `VmContext.frame_state_at` 列表索引。
    Stack { frame_idx: u32, slot: u32 },
    /// 指向 caller 数组对象的 `idx` 元素。GcRef 持有数组，让 GC 跟随。
    Array { gc_ref: GcRef<ArrayObj>, idx: usize },
    /// 指向 caller 对象的命名字段。
    Field { gc_ref: GcRef<ScriptObject>, field_name: String },
}

/// Origin of a [`Value::PinnedView`]. Recorded for diagnostics; both kinds
/// share the same wire form (raw bytes + length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSourceKind {
    Str,
    ArrayU8,
}

/// Payload of [`Value::PinnedView`] — boxed (review.md C1 step 1,
/// 2026-05-27) so the inline `Value` doesn't pay for the 24-byte raw
/// FFI view triple. `PinPtr` constructs one; `UnpinPtr` and any
/// `FieldGet` reading `.ptr` / `.len` borrow through the box.
#[derive(Debug, Clone)]
pub struct PinnedViewData {
    pub ptr:  u64,
    pub len:  u64,
    pub kind: PinSourceKind,
}

/// Payload of [`Value::StackClosure`] — boxed (review.md C1 chunk 3,
/// 2026-05-27) so the inline `Value` doesn't pay for the env-idx + fn
/// name pair. `MkClos` with stack-alloc=1 constructs one; `CallIndirect`
/// is the sole consumer.
#[derive(Debug, Clone)]
pub struct StackClosureData {
    pub env_idx: u32,
    pub fn_name: String,
}

/// Payload of [`Value::Closure`] — lives in the GC variable-length region
/// (`region_var`, `BlockType::Closure`). `MkClos` (heap-alloc path) constructs
/// one; `CallIndirect`, `__delegate_target`, `__delegate_fn_name`,
/// `__delegate_eq` and the GC scanner consume.
///
/// **unify-gc-heap PR-5**: `fn_name` migrated `String` → GC [`Str`] (8B handle),
/// so `ClosureData` now owns **no heap memory outside the GC** — both fields are
/// trivially-droppable (`GcRef`/`Str` have no-op/`Copy` drops). The block is a
/// POD leaf for finalization → the `BlockType::Closure` drop-glue arm is gone
/// (see `gc::arc_heap::var_drop_glue`). Both edges (`env` array, `fn_name`
/// string) are traced by [`Value::trace_children`].
#[derive(Debug, Clone)]
pub struct ClosureData {
    pub env: GcRef<ArrayObj>,
    pub fn_name: Str,
}

/// unify-gc-heap PR-2: read the [`ClosureData`] behind a closure's `VarGcRef` handle (the
/// payload of a `Value::Closure`). Mirrors [`Value::closure_data`] for call sites that have
/// already destructured `Value::Closure(vref)`.
///
/// Safe-signatured (like `Value::closure_data`) on the **liveness invariant**: `vref` must be a
/// live closure handle — true at every call site, which obtains it from a reachable
/// `Value::Closure`. The `ClosureData` is immutable after creation, so the shared borrow needs
/// no lock.
#[inline]
pub fn closure_data_of(vref: &VarGcRef) -> &ClosureData {
    // SAFETY: `vref` names an alive `Closure` block (caller invariant); the payload is exactly
    // one immutable `ClosureData`, valid for the returned borrow.
    unsafe { &*vref.payload_as_ptr::<ClosureData>() }
}

impl Value {
    /// interp-typed-superinstr (2026-08-01): read the `I64` payload **without**
    /// a discriminant check. The interpreter's typed super-instructions call
    /// this only when the compiler-emitted `reg_types[r] == IrType::I64` — the
    /// **same invariant** the JIT's raw-slot arithmetic already trusts
    /// (`jit::translate::is_i64_typed`). `debug_assert` catches an invariant
    /// violation in debug builds; in release the `unreachable_unchecked` lets
    /// LLVM drop the tag branch entirely.
    ///
    /// # Safety
    /// Undefined behavior if `self` is not `Value::I64`. Callers must have
    /// verified the register's static type is `I64` (via `reg_types`).
    #[inline(always)]
    pub unsafe fn as_i64_unchecked(&self) -> i64 {
        match self {
            Value::I64(x) => *x,
            // reg_types guaranteed I64; any other variant is a compiler bug.
            _ => {
                debug_assert!(false, "as_i64_unchecked on non-I64: {self:?}");
                std::hint::unreachable_unchecked()
            }
        }
    }

    /// interp-typed-superinstr (2026-08-01): read the `Bool` payload without a
    /// discriminant check. See [`Value::as_i64_unchecked`] for the safety
    /// contract (here the invariant is `reg_types[r] == IrType::Bool`).
    ///
    /// # Safety
    /// Undefined behavior if `self` is not `Value::Bool`.
    #[inline(always)]
    pub unsafe fn as_bool_unchecked(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => {
                debug_assert!(false, "as_bool_unchecked on non-Bool: {self:?}");
                std::hint::unreachable_unchecked()
            }
        }
    }

    /// **add-write-barriers (2026-05-21)**: returns `true` iff writing
    /// this value into a heap slot must dispatch a GC write barrier.
    /// Heap-ref variants: `Object` / `Array` / `Closure` (Closure.env is a
    /// `GcRef<Vec<Value>>`) / `Ref` with `RefKind::Array` or `RefKind::Field`
    /// (the inner `gc_ref` is a real heap edge). All primitives, plus
    /// `FuncRef` (string-keyed func table) / `PinnedView` (raw ptr) /
    /// `StackClosure` (stack arena env) / `Ref::Stack` (stack location)
    /// return `false` — none of them create a strong heap → heap edge
    /// that card-marking or SATB collectors would care about.
    ///
    /// Mirrors the variant selection of [`Value::trace_children`] —
    /// `is_heap_ref` is the predicate, `trace_children` is the traversal.
    /// unify-gc-heap PR-2: access the [`ClosureData`] behind a `Value::Closure`. Returns
    /// `None` for non-closures. The `ClosureData` lives in the GC `region_var`; the block is
    /// alive as long as this closure `Value` is reachable, so the borrow is sound (same
    /// reachability model as `GcRef`). `ClosureData` is immutable after creation → no lock.
    #[inline]
    pub fn closure_data(&self) -> Option<&ClosureData> {
        match self {
            // SAFETY: a live `Value::Closure` names an alive `Closure` block (reachability);
            // the payload is exactly one immutable `ClosureData`, valid for `&self`'s borrow.
            Value::Closure(vref) => Some(unsafe { &*vref.payload_as_ptr::<ClosureData>() }),
            _ => None,
        }
    }

    #[inline]
    pub fn is_heap_ref(&self) -> bool {
        match self {
            Value::Object(_) | Value::Array(_) | Value::Closure(_) => true,
            // unify-gc-heap PR-4: strings are GC blocks now — storing one into a heap
            // slot (object ref field / array element / struct ref leaf) is a heap edge
            // that needs a write barrier (generational card / concurrent mark-queue),
            // so the string block is found + kept marked. `FuncRef` carries a `Str`.
            Value::Str(_) | Value::FuncRef(_) => true,
            // add-boxed-struct-identity (P4b, 路 B2): 装箱 struct 现是共享 `ScriptObject` 句柄 →
            // 与 `Value::Object` 同为强堆边（存进堆槽需写屏障）。
            Value::BoxedStruct(_) => true,
            // make-value-copy: `Ref` / `StructRefHeap` are now transient-arena handles
            // (like `StructRef` / `StackObject`) — their payload's GcRefs are kept marked
            // by the arena root scan, and the handles never escape the creating frame into
            // a heap slot, so no write barrier is needed here → fall through to `false`.
            _ => false,
        }
    }

    /// **add-mark-sweep-collector P1 (2026-05-21)** / **unify-gc-heap PR-5**: visit every
    /// direct GC-reference child `Value` reachable from `self`. **Single source** for both
    /// the GC mark phase and read-only graph enumeration (heapsnapshot / retention query) —
    /// they differ only on two mark-phase side effects, both gated by `for_marking`:
    ///
    /// - **`for_marking = true`** (mark phase, from `arc_heap`'s mark loop): additionally
    ///   marks the variable-length element backings in place (`mark_backing`, so the
    ///   `region_var` element block stays live this cycle), and surfaces a closure's env
    ///   *array header* (`Value::Array`) + its `fn_name` GC string as children so the mark
    ///   loop marks those blocks too.
    /// - **`for_marking = false`** (enumeration, via `ArcMagrGC::scan_object_refs`): a pure
    ///   read — no mark side effects; descends **directly** into a closure's captured refs
    ///   (the env header and `fn_name` string are internal, not surfaced as graph nodes).
    ///
    /// Primitives / stack-arena handles / struct-blob refs yield no children (their storage
    /// is scanned directly by the external root scanner, so walking here would double-count).
    /// [`Value::is_heap_ref`] is the matching predicate; this is the traversal.
    #[inline]
    pub fn visit_gc_children(&self, for_marking: bool, visit: &mut dyn FnMut(&Value)) {
        match self {
            Value::Object(rc) => {
                let obj = rc.borrow();
                // unify-object-byte-layout: side-table reference leaves (closure/func/
                // string + inline-struct interior refs) live in `refs`; PR-3 chunk 2b
                // additionally inlines direct object/array refs as 8B pointers in `bytes`,
                // scanned via `trace_inline_refs`.
                for r in &obj.refs { visit(r); }
                obj.trace_inline_refs(visit);
            }
            Value::Array(rc) => {
                let arr = rc.borrow();
                if for_marking { arr.mark_backing(); }  // unify-gc-heap PR-3: keep the element block(s) alive
                for elem in arr.gc_refs() { visit(elem); }  // add-struct-heap-inline (P3b): incl struct[] refs
            }
            Value::Closure(vref) => {
                // unify-gc-heap PR-2/PR-5: the closure's `ClosureData` is a GC block in region_var.
                // SAFETY: a reachable closure names an alive block; payload is one ClosureData.
                let data = unsafe { &*vref.payload_as_ptr::<ClosureData>() };
                if for_marking {
                    // Push the env array *header* (so the mark loop marks its region_array entry
                    // and re-traces its elements — one indirection past the pre-PR-2 behaviour)
                    // and the `fn_name` GC string (PR-5, a leaf), so both blocks stay live.
                    visit(&Value::Array(data.env.clone()));
                    visit(&Value::Str(data.fn_name));
                } else {
                    // Enumeration: descend into the captured refs directly — the env header and
                    // fn_name string are the closure's internals, not distinct graph nodes.
                    let arr = data.env.borrow();
                    for elem in arr.gc_refs() { visit(elem); }
                }
            }
            // add-boxed-struct-identity (P4b, 路 B2): 装箱 struct 是共享 `ScriptObject` → 与 Object
            // 同路追踪其 struct_refs 引用叶子（slots 空）。对象本身由 mark 循环的 BoxedStruct 臂标记。
            Value::BoxedStruct(gc) => { let obj = gc.borrow(); for r in &obj.refs { visit(r); } }
            // make-value-copy: `Ref` / `StructRefHeap` are transient-arena handles — leaves
            // here, exactly like `StructRef` / `StackObject`. Their payload's GcRefs (a
            // Ref's Array/Field target, a StructRefHeap's backing array) are scanned
            // *directly* by `TransientArena::scan_roots` (a GC root), so tracing through
            // the handle here would double-count.
            // Primitives — no children.
            // add-escape-analysis-stack-alloc: StackObject / StackArray are
            // leaves for the child-traversal — their slots/elems live in the
            // frame arena and are scanned directly as GC roots by the external
            // root scanner (mirrors StackClosure's env_arena handling), so
            // walking them here would double-count. A stack handle appearing in
            // a heap object's slot would be an escape-analysis bug; the debug
            // asserts in the store paths (FieldSet/ArraySet/StaticSet) catch it.
            // add-struct-value-semantics: StructRef is a leaf here — its blob's
            // reference leaves are scanned directly by the struct-arena root
            // scanner (mirrors StackObject), so walking here would double-count.
            Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_)
            | Value::Str(_) | Value::Null | Value::FuncRef(_)
            | Value::PinnedView { .. } | Value::StackClosure { .. }
            | Value::Ref { .. } | Value::StructRefHeap { .. }
            | Value::StackObject { .. } | Value::StackArray { .. }
            | Value::StructRef { .. } => {}
        }
    }

    /// GC mark-phase traversal — thin wrapper over [`Value::visit_gc_children`] with
    /// `for_marking = true`. `#[inline]` so the constant flag folds away on the hot
    /// mark loop (identical codegen to the pre-convergence dedicated match).
    #[inline]
    pub fn trace_children(&self, visit: &mut dyn FnMut(&Value)) {
        self.visit_gc_children(true, visit);
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I64(a),  Value::I64(b))  => a == b,
            (Value::F64(a),  Value::F64(b))  => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Str(a),  Value::Str(b))  => a == b,
            (Value::Null,    Value::Null)    => true,
            // Array/Object equality is reference equality (same as C# reference semantics)
            (Value::Array(a),  Value::Array(b))  => GcRef::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => GcRef::ptr_eq(a, b),
            // make-value-copy: `PinnedView` / `Ref` (and `StructRefHeap` / `StackClosure`)
            // are now transient-arena handles — compare by `{idx, frame_id}` handle
            // identity (same as `StackObject`). These are internal transient values; user
            // code has no by-value-equality dependency on them (they never reach a
            // user-visible `==` — an escape sink would have materialized the heap form).
            (Value::PinnedView { idx: i1, frame_id: g1 },
             Value::PinnedView { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::Ref { idx: i1, frame_id: g1 },
             Value::Ref { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::StackClosure { idx: i1, frame_id: g1 },
             Value::StackClosure { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::StructRefHeap { idx: i1, frame_id: g1 },
             Value::StructRefHeap { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            // add-primitive-value-boxing → unify Phase 2 R3: 装箱整数 vs 裸整数 —— 透明拆箱按值比较
            // （保留 add-primitive-value-boxing 的混合相等语义）。装箱整数盒是 `BoxedStruct`（整数标量
            // 存 struct_bytes）；非整数盒（多字段 struct 装箱）`boxed_prim_i64` 返 None → 落 `_=>false`
            // （struct 盒 ≠ 裸基元，正确）。装箱整数 vs 装箱整数由下方 BoxedStruct/BoxedStruct 臂按
            // struct_bytes 比（同 wrapper + 同字节 → 值相等），无需单列。
            (Value::BoxedStruct(a), Value::I64(n)) | (Value::I64(n), Value::BoxedStruct(a)) => {
                a.borrow().boxed_prim_i64() == Some(*n)
            }
            // add-struct-object-boxing (PR2a, provisional，design D5)：装箱 struct 值相等——同类型 ∧
            // 字节相等 ∧ 引用叶子逐 Value 相等（refs 的 Value::eq 处理 string 内容 / object 引用）。
            // add-boxed-struct-identity (P4b): 载荷现是共享 `ScriptObject`——先 ptr_eq（同盒必等，且避免
            // 对同一 GcRef 二次 borrow 死锁），否则 borrow 两盒比 struct_bytes/struct_refs（保持值相等语义）。
            (Value::BoxedStruct(a), Value::BoxedStruct(b)) => {
                if GcRef::ptr_eq(a, b) {
                    true
                } else {
                    let (ao, bo) = (a.borrow(), b.borrow());
                    ao.type_desc.name == bo.type_desc.name
                        && ao.bytes == bo.bytes
                        && ao.refs == bo.refs
                }
            }
            // add-escape-analysis-stack-alloc: 栈句柄引用相等 —— 同 (frame_idx, idx,
            // frame_id) = 同一栈对象/数组（Eq 操作数在逃逸分析里是 neutral，故栈句柄
            // 可作 `p1==p2` / `p==null` 操作数；`==null` 落 `_ => false` = 正确「非 null」）。
            (Value::StackObject { idx: i1, frame_id: g1 },
             Value::StackObject { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::StackArray { idx: i1, frame_id: g1 },
             Value::StackArray { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            _ => false,
        }
    }
}

/// Execution mode for a module or function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecMode {
    /// Tree-walking / bytecode interpreter — fast startup, no warmup cost.
    Interp,
    /// Just-in-time compilation — best steady-state throughput.
    Jit,
    /// Ahead-of-time compilation — best for predictable, startup-sensitive code.
    Aot,
}

impl Default for ExecMode {
    fn default() -> Self {
        ExecMode::Interp
    }
}

// ── Backward compatibility alias ─────────────────────────────────────────────

/// Deprecated alias kept so external code using `ObjectData` by name continues
/// to compile during the transition.  New code should use `ScriptObject`.
#[deprecated(note = "use ScriptObject instead")]
pub type ObjectData = ScriptObject;
