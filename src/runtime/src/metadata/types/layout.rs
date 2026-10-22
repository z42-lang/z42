//! 对象 / struct 字节布局：StructTypeLayout / ObjectLayout / FieldAccess / compose·synthesize。refactor-split-metadata-types：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::ArrRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// Cold side-table for `TypeDesc`. Holds inheritance fixup inputs +
/// generics metadata. Touched only by loader fixup, reflection /
/// `DefaultOf` opcode, and constraint verification — never by hot
/// dispatch.
/// add-struct-value-semantics: reference-leaf kind in a value-struct's reference
/// bitmap (mirrors the compiler's `StructLeafKind` for the two reference kinds;
/// primitive leaves are never listed). Every reference leaf of a heap-resident blob is an
/// 8 B reference word (`struct_blob`); the kind is retained for diagnostics.
pub const STRUCT_REF_ARC_STRING: u8 = 1;
pub const STRUCT_REF_GCREF: u8 = 2;

/// add-struct-value-semantics: runtime byte + reference layout of a value-struct
/// type, delivered by the zbc TYPE-section struct block (A-use). `size` = the
/// byte-blob size; `ref_offsets` / `ref_kinds` = the byte offset + kind of each
/// reference leaf (parallel arrays, bitmap order). On the heap (boxed struct, inline field,
/// `struct[]` element) each reference leaf is an 8 B reference word at its offset; in the frame
/// arena it is a `Value` in the blob's `refs`, in `ref_offsets` order (`struct_blob`). Pure-primitive structs have
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
    /// `unify-object-byte-layout (PR-1)` 的先例）。接通它是 P2 的事。
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
    /// `StructLeafKind`：0=Prim / 1=ArcString / 2=ArrRef / 3=Struct / 4=GcRefArray / 5=GcRefClosure。
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
/// - `offset` / `width` = the field's byte window in `ScriptObject::bytes` (for a
///   type-parameter field: its tag word).
/// - `tag` = the **exact** `ty::TAG_*` recovered from the field's declared
///   `type_tag` string (via `tag_from_name`) — `field_kinds` (coarse `StructLeafKind`)
///   can't drive `decode_prim`, so the precise tag comes from the type string, the
///   same source `default_value_for` uses. `TAG_UNKNOWN` for struct-typed roots
///   (never reached by `FieldGet`; accessed via `StructFieldGetPrim`).
/// - `cell` = which kind of cell holds the value ([`FieldCell`]).
/// - `aux` = the cell's second coordinate: the payload word's byte offset for a
///   [`FieldCell::TypeParam`] cell, `0` otherwise.
#[derive(Debug, Clone, Copy)]
pub struct FieldAccess {
    pub offset: u32,
    pub width: u32,
    pub tag: u8,
    pub cell: FieldCell,
    pub aux: u32,
}

/// Where a direct field's value lives (see [`FieldAccess::cell`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldCell {
    /// Primitive at its natural width in `bytes` (relaxed atomic).
    Prim,
    /// 8 B self-describing reference word in `bytes` (release store / acquire load).
    Ref,
    /// Type-parameter field (`T F;`, `T? F;`) — and every reference field of a synthesized
    /// layout: a 16 B cell in `bytes` — the self-describing tag word at `offset`, the primitive
    /// payload word at `aux` (`tparam_cell`).
    TypeParam,
    /// An inline value-struct root — not a `FieldGet` target (`StructFieldGetPrim`).
    Struct,
}

impl FieldAccess {
    /// A primitive leaf of `width` bytes and exact tag `tag`.
    pub const fn prim(offset: u32, width: u32, tag: u8) -> Self {
        Self { offset, width, tag, cell: FieldCell::Prim, aux: 0 }
    }
    /// An 8 B reference word.
    pub const fn ref_word(offset: u32, tag: u8) -> Self {
        Self { offset, width: 8, tag, cell: FieldCell::Ref, aux: 0 }
    }
    /// A type-parameter cell: tag word at `offset`, payload word at `payload`.
    pub const fn type_param(offset: u32, payload: u32) -> Self {
        Self { offset, width: 8, tag: TAG_OBJECT, cell: FieldCell::TypeParam, aux: payload }
    }
    /// An inline value-struct root.
    pub const fn struct_root(offset: u32, width: u32) -> Self {
        Self { offset, width, tag: TAG_UNKNOWN, cell: FieldCell::Struct, aux: 0 }
    }

    #[inline(always)]
    pub fn cell(&self) -> FieldCell {
        self.cell
    }

    /// The access of a resolved [`FieldLoc`] — the inverse of [`FieldLoc::of`]. A type-parameter
    /// cell's second coordinate is not in the locator (it carries the field slot instead), so it is
    /// read from `slot_access` (`ScriptObject`'s layout lookup).
    #[inline(always)]
    pub fn of_loc(loc: FieldLoc, slot_access: impl FnOnce(usize) -> Option<FieldAccess>) -> Option<FieldAccess> {
        let off = loc.offset as u32;
        match loc.kind {
            FieldLoc::KIND_REF => Some(FieldAccess::ref_word(off, TAG_OBJECT)),
            FieldLoc::KIND_TPARAM => slot_access(loc.offset as usize),
            FieldLoc::KIND_STRUCT => Some(FieldAccess::struct_root(off, 0)),
            tag => Some(FieldAccess::prim(off, loc.width as u32, tag)),
        }
    }
}

/// A direct field's cell, resolved to what a field access needs: the cell's byte offset in the
/// field area and its kind — what a `FieldIC` entry caches next to the receiver's `TypeId`
/// (`metadata::resolver::ic`). A cache hit is one compare and one cell access: no name lookup, no
/// layout walk.
///
/// `kind` is the primitive's exact `ty::TAG_*` (`TAG_BOOL ..= TAG_CHAR`, its byte width in `width`) or
/// one of the `KIND_*` codes (`width` 8). A type-parameter cell has two coordinates (tag word, payload word), so its
/// locator carries the **field slot** in `offset` and the access is looked up from the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldLoc {
    pub offset: u16,
    pub width: u8,
    pub kind: u8,
}

impl FieldLoc {
    /// An 8 B reference word at `offset`.
    pub const KIND_REF: u8 = 0x40;
    /// A type-parameter cell; `offset` is the field slot.
    pub const KIND_TPARAM: u8 = 0x41;
    /// An inline value-struct root (`FieldGet` reads `null`, `FieldSet` ignores it).
    pub const KIND_STRUCT: u8 = 0x42;
    /// The kind byte of an empty IC entry (`UNRESOLVED` payload) — never a real kind.
    pub const KIND_NONE: u8 = 0xFF;
    /// Bit position of `offset` in the packed form (`kind` is the low byte, `width` the next).
    pub const OFFSET_SHIFT: u32 = 16;

    /// The locator of field `slot` whose access is `fa`; `None` when its coordinate does not fit
    /// 16 bits (a field area past 64 KiB — such a site just stays uncached).
    #[inline]
    pub fn of(fa: &FieldAccess, slot: usize) -> Option<FieldLoc> {
        let (coord, kind) = match fa.cell {
            FieldCell::Prim => (fa.offset as usize, fa.tag),
            FieldCell::Ref => (fa.offset as usize, Self::KIND_REF),
            FieldCell::TypeParam => (slot, Self::KIND_TPARAM),
            FieldCell::Struct => (0, Self::KIND_STRUCT),
        };
        let width = match fa.cell {
            // A layout whose width disagrees with its tag stays on the layout path.
            FieldCell::Prim if prim_cell_width(fa.tag) != Some(fa.width) => return None,
            FieldCell::Prim => fa.width as u8,
            _ => 8,
        };
        Some(FieldLoc { offset: u16::try_from(coord).ok()?, width, kind })
    }

    /// Packed into the 32-bit IC payload: `offset << 16 | width << 8 | kind`.
    #[inline(always)]
    pub const fn to_bits(self) -> u32 {
        ((self.offset as u32) << Self::OFFSET_SHIFT) | ((self.width as u32) << 8) | self.kind as u32
    }

    #[inline(always)]
    pub const fn from_bits(bits: u32) -> FieldLoc {
        FieldLoc { offset: (bits >> Self::OFFSET_SHIFT) as u16, width: (bits >> 8) as u8, kind: bits as u8 }
    }
}

/// Byte width of a primitive cell of exact tag `tag`; `None` for a non-primitive tag. The
/// infallible twin of `codec::prim_width` for the field-access hot path.
#[inline(always)]
pub const fn prim_cell_width(tag: u8) -> Option<u32> {
    Some(match tag {
        TAG_BOOL | TAG_I8 | TAG_U8 => 1,
        TAG_I16 | TAG_U16 => 2,
        TAG_I32 | TAG_U32 | TAG_F32 | TAG_CHAR => 4,
        TAG_I64 | TAG_U64 | TAG_F64 => 8,
        _ => return None,
    })
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
/// - `ref_cells` = every 8 B reference word of the instance: direct reference fields and
///   inline-struct interior reference leaves (base first, then own shifted by the base region).
/// - `tparam_cells` = the tag words of the 16 B type-parameter cells.
#[derive(Debug, Default)]
pub struct ObjectLayout {
    pub size: usize,
    pub field_offsets: Box<[u32]>,
    pub field_sizes: Box<[u32]>,
    pub field_kinds: Box<[u8]>,
    /// Byte offsets of every 8 B self-describing reference word in `bytes` (`ref_word`: kind in
    /// the low 3 bits, `0` = `null`): each direct `string` / class / array / interface /
    /// `object` / delegate field and each reference leaf of an inline value struct (the struct
    /// keeps its standalone blob layout — `struct_blob`). GC reads each word and decodes it
    /// (`ScriptObject::visit_refs`). Empty for synthesized layouts.
    pub ref_cells: Box<[u32]>,
    /// Byte offsets of the **tag words** of the type-parameter cells (`T F;`, `T? F;`; base
    /// cells first, then own) — each the field's own 8 B slot of the compiler layout. Cell
    /// `i`'s payload word sits past the compiler region, at
    /// [`ObjectLayout::tparam_payload_offset`]`(i)`. See `tparam_cell`.
    pub tparam_cells: Box<[u32]>,
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
    /// Byte offset of type-parameter cell `i`'s payload word: past the compiler's region
    /// (`size`, 8-aligned), one 8 B word per cell. Outside `size`, so it never moves a
    /// field the compiler laid out and a subclass's own region (which starts at
    /// `align_up(base.size, 8)`) still lines up with the compiler's offsets.
    #[inline]
    pub fn tparam_payload_offset(&self, i: usize) -> u32 {
        Self::payload_base(self.size) + 8 * i as u32
    }

    #[inline]
    fn payload_base(size: usize) -> u32 {
        ((size as u32) + 7) & !7
    }

    /// Length of an instance's byte region: the compiler's layout plus one payload word
    /// per type-parameter cell.
    #[inline]
    pub fn bytes_len(&self) -> usize {
        if self.tparam_cells.is_empty() {
            return self.size;
        }
        Self::payload_base(self.size) as usize + 8 * self.tparam_cells.len()
    }

    /// Whether the 8 B cell at `byte_off` is a reference word (a direct reference field or an
    /// inline-struct reference leaf). Linear scan — reference cells per object are few.
    #[inline]
    pub fn is_ref_cell(&self, byte_off: u32) -> bool {
        self.ref_cells.contains(&byte_off)
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
///   and get a 16 B type-parameter cell (`tparam_cells`: tag word at the field's slot,
///   payload word appended past `size`);
/// - an inline-struct interior leaf → an 8 B reference word too (the inline struct keeps the
///   blob layout of its standalone form, `struct_blob`).
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
    let mut ref_cells     = Vec::new();
    let mut tparam_cells  = Vec::new();
    if let Some(b) = base {
        field_offsets.extend_from_slice(&b.field_offsets);
        field_sizes.extend_from_slice(&b.field_sizes);
        field_kinds.extend_from_slice(&b.field_kinds);
        ref_cells.extend_from_slice(&b.ref_cells);
        tparam_cells.extend_from_slice(&b.tparam_cells);
    }
    for &off in own.field_offsets.iter() { field_offsets.push(off + base_shift); }
    field_sizes.extend_from_slice(&own.field_sizes);
    field_kinds.extend_from_slice(&own.field_kinds);

    // The cell of the own direct reference field whose leaf sits at `off` (`None` = not a
    // direct field: an inline-struct interior leaf).
    let own_direct_cell = |off: u32| -> Option<FieldCell> {
        own.field_offsets.iter().zip(own.field_kinds.iter()).enumerate()
            .find(|(_, (&o, &k))| o == off && is_ref_leaf_kind(k))
            .map(|(j, (_, &k))| {
                let declared = merged_fields.get(base_fields + j).map(|f| &*f.type_tag);
                if k == STRUCT_LEAF_GCREF_CLOSURE && declared.is_some_and(|t| names_type_param(t, type_params)) {
                    FieldCell::TypeParam
                } else {
                    FieldCell::Ref
                }
            })
    };
    for &off in own.ref_offsets.iter() {
        match own_direct_cell(off) {
            Some(FieldCell::TypeParam) => tparam_cells.push(off + base_shift),
            _ => ref_cells.push(off + base_shift),
        }
    }
    let size = (base_shift + own.size) as usize;

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
            if kind == STRUCT_LEAF_STRUCT {
                acc.push(FieldAccess::struct_root(off, width));
                continue;
            }
            if !is_ref_leaf_kind(kind) {
                acc.push(FieldAccess::prim(off, width, resolve_prim_tag(type_tag.unwrap_or(""), width)));
                continue;
            }
            let tag = match kind {
                STRUCT_LEAF_ARCSTRING => TAG_STR,
                STRUCT_LEAF_GCREF_ARRAY => TAG_ARRAY,
                _ => TAG_OBJECT,
            };
            // A reference field is a type-parameter cell (a base composed by synthesis keeps
            // every reference in one) or an 8 B reference word. The base part of
            // `tparam_cells` records the base's own decisions.
            acc.push(if let Some(i) = tparam_cells.iter().position(|&o| o == off) {
                FieldAccess::type_param(off, ObjectLayout::payload_base(size) + 8 * i as u32)
            } else {
                FieldAccess::ref_word(off, tag)
            });
        }
        acc.into()
    };

    ObjectLayout {
        size,
        field_offsets: field_offsets.into(),
        field_sizes:   field_sizes.into(),
        field_kinds:   field_kinds.into(),
        ref_cells:     ref_cells.into(),
        tparam_cells:  tparam_cells.into(),
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
/// get a 16 B type-parameter cell: the tag word in an 8 B slot, the payload word past `size`
/// (`tparam_cell` — it holds any `Value`). Internally consistent (all of
/// `field_value` / `set_field_value` / GC read the same table); never cross-checked
/// against compiler output, so its exact packing only needs to be self-consistent.
///
/// ⚠️ **「a struct field never occurs in a layout-less type」是前置条件，不是这里能检查的事**
/// —— 本函数只拿到 `&[FieldSlot]`，没有类型注册表，`tag_from_type_name` 对任何非基元名都给
/// `TAG_OBJECT`，**无从区分 struct 与 class**。所以这条不变式必须由**写端**保证，而它确实成立
/// （核实，此前这里只断言、没给理由）：
///
/// - zbc writer 的对象布局块 gate 是 `(cd.Flags & 116) == 0`，`116 = 4|16|32|64`
///   = struct｜interface｜enum｜delegate ⇒ **每个普通 class 一律带布局块**，走不到本函数；
/// - 格式是 strict-pin（只接受一个 minor）⇒ 「zbc < 1.34 的旧模块没有布局块」这条路不存在；
/// - 泛型 class 的**实例化**描述符也填了对象布局块（`ClassDescBuilder.GenericInst` 的
///   `_instClassDesc`，并按需置 `class_flags` bit7 = 有内联 struct 字段）。
///
/// 如果哪天违反了，症状是 struct 字段**之后**的每个字段偏移与编译器烘焙进指令的偏移不一致
/// （这里当 8B 引用、编译器整块内联），且内部引用叶子不在 `ref_cells` 里 ⇒
/// `inline struct ref leaf at byte offset N not in object layout`。**要加检查，得加在写端**
/// 或调用方（那里有注册表），不是这里。
pub fn synthesize_object_layout(fields: &[FieldSlot]) -> ObjectLayout {
    let mut cursor: u32 = 0;
    let mut field_offsets = Vec::with_capacity(fields.len());
    let mut field_sizes   = Vec::with_capacity(fields.len());
    let mut field_kinds   = Vec::with_capacity(fields.len());
    let mut tparam_cells  = Vec::new();
    let mut prims: Vec<(u32, u32, u8)> = Vec::with_capacity(fields.len());
    for f in fields {
        let tag = tag_from_type_name(&f.type_tag);
        let is_ref = is_ref_tag(tag);
        let width: u32 = if is_ref { 8 } else { prim_width(tag).unwrap_or(8) as u32 };
        let align = width.max(1);
        let off = (cursor + (align - 1)) & !(align - 1);
        cursor = off + width;
        field_offsets.push(off);
        field_sizes.push(width);
        if is_ref {
            tparam_cells.push(off);
            field_kinds.push(if tag == TAG_STR { STRUCT_LEAF_ARCSTRING } else { STRUCT_LEAF_GCREF_CLOSURE });
        } else {
            field_kinds.push(STRUCT_LEAF_PRIM);
            prims.push((off, width, tag));
        }
    }
    let size = ((cursor + 7) & !7) as usize;
    // Without the compiler's `field_kinds` a type-parameter field is indistinguishable from a
    // reference field, so every reference gets a type-parameter cell (it holds any `Value`).
    let field_access: Vec<FieldAccess> = field_offsets.iter().zip(field_sizes.iter()).map(|(&off, &w)| {
        match prims.iter().find(|p| p.0 == off) {
            Some(&(_, _, tag)) => FieldAccess::prim(off, w, tag),
            None => {
                let i = tparam_cells.iter().position(|&o| o == off).unwrap_or(0);
                FieldAccess::type_param(off, ObjectLayout::payload_base(size) + 8 * i as u32)
            }
        }
    }).collect();
    ObjectLayout {
        size,
        field_offsets: field_offsets.into(),
        field_sizes:   field_sizes.into(),
        field_kinds:   field_kinds.into(),
        ref_cells:     Box::new([]),
        tparam_cells:  tparam_cells.into(),
        field_access:  field_access.into(),
    }
}
