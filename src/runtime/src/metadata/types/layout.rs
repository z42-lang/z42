//! 对象 / struct 字节布局：StructTypeLayout / ObjectLayout / FieldAccess / compose·synthesize。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
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
    /// **逐字段**布局，按字段声明位置索引（`fields[i]` = 第 i 个字段）。
    ///
    /// 来自 zbc 1.45 的 TYPE 段 struct 字段表（`ClassDesc.struct_field_table`，change
    /// `type-section-flags2-and-struct-fields` / #903）。空 = 该类型没有携带字段表
    /// （旧产物、或 `resolve_layout` 的 size-only 兜底）。
    ///
    /// 🔴 **它是给谁用的**（change `symbolic-struct-field-access` P0）：让 struct 字段访问
    /// 可以带**字段序号**而不是编译期烘焙的**字节偏移**。关键性质是
    /// **序号实例化不变、偏移随实例化变** —— `Pair<A,B>` 的 `First`/`Second` 永远是 0/1，
    /// 而它们的偏移随 `A`/`B` 而变。于是解析 = `fields[i].offset`，
    /// **O(1) 数组下标，无哈希、无字符串、不需要 IC**。
    ///
    /// ⚠️ 本字段目前是**休眠元数据：没有任何消费方**（形态同
    /// `unify-object-byte-layout (PR-1)` 当年的做法）。接通它是 P2 的事。
    /// 之所以先单独落地：P0 纯附加、零行为变化，可独立 GREEN；而 P2 要改指令编码、
    /// 得在此之上做。
    pub fields: Box<[StructFieldLayout]>,
}

/// `StructTypeLayout::fields` 的一项 —— 一个字段的偏移 / 宽 / 叶子种类。
///
/// 与 zbc 侧的 `bytecode::StructFieldEntry` 一一对应（那是 wire 形态，这是运行期形态）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructFieldLayout {
    /// blob 内字节偏移。
    pub offset: u32,
    /// 字节宽。
    pub size: u32,
    /// `StructLeafKind`：0=Prim / 1=ArcString / 2=GcRef / 3=Struct / 4=GcRefArray / 5=GcRefClosure。
    pub kind: u8,
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

    /// 第 `i` 个字段的字节偏移（`None` = 没有字段表，或 `i` 越界）。
    ///
    /// symbolic-struct-field-access P0：这是「符号化」那一步要的唯一原语 ——
    /// 指令带序号，运行期在这里换成偏移。**序号实例化不变，偏移随实例化变。**
    #[inline]
    pub fn field_offset(&self, i: usize) -> Option<u32> {
        self.fields.get(i).map(|f| f.offset)
    }

    /// 第 `i` 个字段的 (偏移, 宽, 叶子种类)。
    #[inline]
    pub fn field_at(&self, i: usize) -> Option<StructFieldLayout> {
        self.fields.get(i).copied()
    }

    /// 字段数（0 = 没有携带字段表 —— **不是**「这个 struct 没有字段」）。
    #[inline]
    pub fn field_count(&self) -> usize {
        self.fields.len()
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
/// - `ref_slot` = index into the 16 B `ScriptObject::refs` side table for a field kept
///   there (a type-parameter field `T F;`, or every reference of a synthesized layout),
///   else `-1`.
///
/// Which kind of cell the field is follows from `tag` + `ref_slot` ([`FieldAccess::cell`]):
/// a reference tag with no side-table slot is an 8 B self-describing reference word in
/// `bytes` (`ref_word`).
#[derive(Debug, Clone, Copy, Default)]
pub struct FieldAccess {
    pub offset: u32,
    pub width: u32,
    pub tag: u8,
    pub ref_slot: i32,
}

/// Where a direct field's value lives (see [`FieldAccess::cell`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldCell {
    /// Primitive at its natural width in `bytes` (relaxed atomic).
    Prim,
    /// 8 B self-describing reference word in `bytes` (release store / acquire load).
    Ref,
    /// 16 B `Value` in the `refs` side table (type-parameter fields).
    Value,
    /// An inline value-struct root — not a `FieldGet` target (`StructFieldGetPrim`).
    Struct,
}

impl FieldAccess {
    #[inline(always)]
    pub fn cell(&self) -> FieldCell {
        if self.ref_slot >= 0 {
            return FieldCell::Value;
        }
        match self.tag {
            TAG_OBJECT | TAG_ARRAY | TAG_STR => FieldCell::Ref,
            TAG_UNKNOWN => FieldCell::Struct,
            _ => FieldCell::Prim,
        }
    }
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
    /// **Side-table** reference bitmap: byte offsets of the reference leaves that live
    /// as 16 B `Value`s in `ScriptObject::refs` — type-parameter fields (`T F;`, any
    /// `Value` incl. raw primitives under erasure) and every inline-struct interior
    /// reference leaf (struct blobs keep 16 B leaves until arrays move to 8 B cells, R3).
    /// Synthesized layouts keep every reference here.
    pub ref_offsets: Box<[u32]>,
    pub ref_kinds: Box<[u8]>,
    /// Byte offsets of the direct reference fields stored as an 8 B self-describing
    /// reference word in `bytes` (`ref_word`: kind in the low 3 bits, `0` = `null`) —
    /// every `string` / class / array / interface / `object` / delegate field. GC reads
    /// each word and decodes it (`ScriptObject::visit_refs`). Empty for synthesized layouts.
    pub ref_cells: Box<[u32]>,
    /// unify-object-byte-layout (PR-2, D12): resolved per-field access table (offset /
    /// width / exact tag / refs-slot), parallel by index with `TypeDesc::fields`.
    /// Filled at load time from the composed offsets + the merged fields' `type_tag`
    /// strings. Empty when composed without field info (e.g. `compose_object_layout`
    /// called with `&[]` in unit tests that only check structural offsets).
    pub field_access: Box<[FieldAccess]>,
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
/// Refined direct-field reference kinds emitted by the compiler's object block
/// (`StructLayout._refineDirectRefKind`): a concrete class stays `GCREF`, an array is
/// `GCREF_ARRAY`, and everything else — `object`, interfaces, delegates, unresolved type
/// parameters — is `GCREF_CLOSURE`. All of them get an 8 B reference word except a
/// `GCREF_CLOSURE` field whose declared type is a type parameter (`compose_object_layout`).
pub const STRUCT_LEAF_GCREF_ARRAY: u8 = 4;   // array `T[]`
pub const STRUCT_LEAF_GCREF_CLOSURE: u8 = 5; // object / interface / delegate / type parameter

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

/// Compose a class's **own-only** `ObjectLayoutDesc` (the zbc object block, offsets from
/// 0) with its base class's already-composed `ObjectLayout` into the merged runtime layout,
/// mirroring `merge_with_base`'s `fields = base.fields ++ own`. The own region begins at
/// `align_up(base.size, 8)` — the unified 8B inheritance boundary (matches the compiler's
/// independent base-shift when it bakes inline-struct leaf offsets); both must agree
/// byte-for-byte, backstopped by self-host byte-identity.
///
/// `base` is `None` for a root class (or a cross-zpkg base not yet resolved — the fixup
/// pass recomposes once it resolves). The base part (offsets, reference words, side table)
/// is taken over unchanged; the own part is shifted by `base_shift`.
///
/// Cell assignment for the own reference leaves:
/// - a **direct** reference field → an 8 B reference word in `bytes` (`ref_cells`), unless
///   it is a type-parameter field: kind `GCREF_CLOSURE` and a declared type that names one
///   of `type_params` (`T`, `T?`). Those can hold any `Value` (a raw `I64` under erasure)
///   and stay 16 B in the side table;
/// - an inline-struct interior leaf → the side table.
///
/// `merged_fields` = the class's full merged field list (`base.fields ++ own`, same order as
/// the composed offsets); it supplies each field's declared type for the exact primitive
/// tag and the type-parameter test. Pass `&[]` to skip the access table (structural-only
/// unit tests).
pub fn compose_object_layout(
    base: Option<&ObjectLayout>,
    own: &crate::metadata::bytecode::ObjectLayoutDesc,
    merged_fields: &[FieldSlot],
    type_params: &[String],
) -> ObjectLayout {
    let base_shift: u32 = match base {
        Some(b) => ((b.size as u32) + 7) & !7,
        None    => 0,
    };
    let base_fields = base.map_or(0, |b| b.field_offsets.len());

    let mut field_offsets = Vec::with_capacity(base_fields + own.field_offsets.len());
    let mut field_sizes   = Vec::with_capacity(base_fields + own.field_sizes.len());
    let mut field_kinds   = Vec::with_capacity(base_fields + own.field_kinds.len());
    let mut ref_offsets   = Vec::new();
    let mut ref_kinds     = Vec::new();
    let mut ref_cells     = Vec::new();
    if let Some(b) = base {
        field_offsets.extend_from_slice(&b.field_offsets);
        field_sizes.extend_from_slice(&b.field_sizes);
        field_kinds.extend_from_slice(&b.field_kinds);
        ref_offsets.extend_from_slice(&b.ref_offsets);
        ref_kinds.extend_from_slice(&b.ref_kinds);
        ref_cells.extend_from_slice(&b.ref_cells);
    }
    for &off in own.field_offsets.iter() { field_offsets.push(off + base_shift); }
    field_sizes.extend_from_slice(&own.field_sizes);
    field_kinds.extend_from_slice(&own.field_kinds);

    // The own direct field whose reference leaf sits at `off`, if it gets an 8 B cell.
    let own_ref_cell_at = |off: u32| -> bool {
        own.field_offsets.iter().zip(own.field_kinds.iter()).enumerate()
            .find(|(_, (&o, &k))| o == off && is_ref_leaf_kind(k))
            .is_some_and(|(j, (_, &k))| {
                let declared = merged_fields.get(base_fields + j).map(|f| &*f.type_tag);
                !(k == STRUCT_LEAF_GCREF_CLOSURE && declared.is_some_and(|t| names_type_param(t, type_params)))
            })
    };
    for (&off, &rk) in own.ref_offsets.iter().zip(own.ref_kinds.iter()) {
        if own_ref_cell_at(off) {
            ref_cells.push(off + base_shift);
        } else {
            ref_offsets.push(off + base_shift);
            ref_kinds.push(rk);
        }
    }

    let field_access: Box<[FieldAccess]> = if merged_fields.is_empty() {
        Box::new([])
    } else {
        let mut acc = Vec::with_capacity(field_offsets.len());
        for i in 0..field_offsets.len() {
            let off = field_offsets[i];
            let width = field_sizes.get(i).copied().unwrap_or(0);
            // Classify from the compiler's authoritative `field_kinds` (StructLeafKind), not
            // the field's `type_tag` string (which may be an unresolved alias).
            let kind = field_kinds.get(i).copied().unwrap_or(STRUCT_LEAF_PRIM);
            let type_tag = merged_fields.get(i).map(|f| f.type_tag.as_ref());
            // A reference field kept in the side table carries its slot; `-1` = 8 B cell.
            // (The base part of `ref_offsets` already records the base's own decisions.)
            let slot = ref_offsets.iter().position(|&o| o == off).map_or(-1, |ri| ri as i32);
            let (tag, ref_slot) = match kind {
                STRUCT_LEAF_STRUCT => (TAG_UNKNOWN, -1),
                STRUCT_LEAF_ARCSTRING => (TAG_STR, slot),
                STRUCT_LEAF_GCREF_ARRAY => (TAG_ARRAY, slot),
                STRUCT_LEAF_GCREF | STRUCT_LEAF_GCREF_CLOSURE => (TAG_OBJECT, slot),
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
        ref_cells:     ref_cells.into(),
        field_access,
    }
}

/// Whether a direct field's `StructLeafKind` is a reference leaf.
#[inline]
fn is_ref_leaf_kind(k: u8) -> bool {
    matches!(k, STRUCT_LEAF_ARCSTRING | STRUCT_LEAF_GCREF | STRUCT_LEAF_GCREF_ARRAY | STRUCT_LEAF_GCREF_CLOSURE)
}

/// Whether a declared field type names one of the declaring class's type parameters
/// (`T`, or the nullable `T?`).
#[inline]
fn names_type_param(type_tag: &str, type_params: &[String]) -> bool {
    let t = type_tag.strip_suffix('?').unwrap_or(type_tag);
    type_params.iter().any(|p| p == t)
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
///
/// ⚠️ **「a struct field never occurs in a layout-less type」是前置条件，不是这里能检查的事**
/// —— 本函数只拿到 `&[FieldSlot]`，没有类型注册表，`tag_from_type_name` 对任何非基元名都给
/// `TAG_OBJECT`，**无从区分 struct 与 class**。所以这条不变式必须由**写端**保证，而它确实成立
/// （2026-09-27 核实，此前这里只断言、没给理由）：
///
/// - zbc writer 的对象布局块 gate 是 `(cd.Flags & 116) == 0`，`116 = 4|16|32|64`
///   = struct｜interface｜enum｜delegate ⇒ **每个普通 class 一律带布局块**，走不到本函数；
/// - 格式是 strict-pin（只接受一个 minor）⇒ 「zbc < 1.34 的旧模块没有布局块」这条路不存在；
/// - 泛型 class 的**实例化**描述符也填了对象布局块（`ClassDescBuilder.GenericInst` 的
///   `_instClassDesc`，并按需置 `class_flags` bit7 = 有内联 struct 字段）。
///
/// 如果哪天违反了，症状是 struct 字段**之后**的每个字段偏移与编译器烘焙进指令的偏移不一致
/// （这里当 8B 引用、编译器整块内联），且内部引用叶子不在 `ref_offsets` 里 ⇒
/// `inline struct ref leaf at byte offset N not in object layout`。**要加检查，得加在写端**
/// 或调用方（那里有注册表），不是这里。
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
        // Synthesized layouts keep every reference in the 16 B side table: without the
        // compiler's `field_kinds` a type-parameter field is indistinguishable from a
        // reference field, and only the side table holds any `Value`.
        ref_cells:     Box::new([]),
        field_access:  field_access.into(),
    }
}
