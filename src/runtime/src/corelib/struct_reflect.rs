//! add-boxed-struct-identity (P4b): struct field layout replication for reflection.
//!
//! Reflection reads a value struct's fields **by name**, which needs each field's byte
//! offset + type tag. The runtime only carries a value struct's `StructTypeLayout`
//! (`size` + an *unnamed* reference-leaf bitmap), not a field-name → offset map. This
//! module recovers that map by **replicating the compiler's `StructLayout._compute`**
//! (`src/compiler/z42c.semantics/src/StructLayout.z42:279`) over the type's ordered
//! `TypeDesc.fields` (each `FieldSlot{name, type_tag}` — `type_tag` is the exact declared
//! type string the codegen fed to `Tag.FromName`).
//!
//! Design decision D1 (方案 B, User-approved): Rust replication rather than a zbc format
//! bump that ships a per-field offset table. Format-neutral + warm-verifiable. The
//! replication risk is contained by **three-layer validation** (`validate_against`) that
//! cross-checks the computed layout against the authoritative delivered `struct_layout`.
//!
//! Mirror sources (single source of truth stays in the compiler; keep in sync):
//! - `canon`         ← `Z42Type.Canon`      (`Z42Type.z42:15`)
//! - `size_of`/`align_of`/`leaf_kind` ← `StructLayout._sizeOf/_alignOf/_kindOf` (`StructLayout.z42:332/343/322`)
//! - `tag_from_name` ← `Tag.FromName`       (`z42.ir/.../ZbcFormat.z42:75`)

use crate::metadata::types::{self as ty, StructTypeLayout, TypeDesc};
use anyhow::{bail, Result};
use std::sync::Arc;

/// Resolves a type name to its `TypeDesc` (for nested-struct recursion + is-struct checks).
/// Production caller passes `&|n| ctx.try_lookup_type(n)`; tests pass a map-backed closure.
pub type TypeResolver<'a> = dyn Fn(&str) -> Option<Arc<TypeDesc>> + 'a;

/// A resolved top-level field of a value struct: where its data lives + how to decode it.
#[derive(Debug, Clone)]
pub struct FieldLeaf {
    pub name: String,
    /// Byte offset of this field within the struct blob.
    pub byte_off: u32,
    /// Byte size of this field (primitive width, 16 for a reference leaf, or the nested
    /// struct's total size).
    pub size: u32,
    /// zbc `Tag` for a primitive leaf (drives `decode_prim`/`encode_prim`). Mirrors the tag
    /// the codegen baked via `Tag.FromName`. Informational for reference / struct fields.
    pub tag: u8,
    /// This field's declared type is itself a value struct → its value is a nested blob.
    pub is_struct: bool,
    /// This field is a reference leaf (string / object / array) — its value lives in the
    /// blob's `refs` side-table, indexed by `ref_offsets.position(byte_off)`.
    pub is_ref: bool,
    /// Declared field type string (for nested-struct boxing / diagnostics).
    pub type_name: String,
}

/// The full computed layout of a value struct: its top-level fields (for name lookup) +
/// the flattened reference-leaf offsets (for validation against the delivered layout).
#[derive(Debug, Clone)]
pub struct ComputedLayout {
    pub leaves: Vec<FieldLeaf>,
    /// All reference-leaf byte offsets (including those flattened out of nested structs),
    /// in the compiler's bitmap order.
    pub ref_offsets: Vec<u32>,
    pub size: u32,
    pub align: u32,
}

impl ComputedLayout {
    /// Find a top-level field by name.
    pub fn field(&self, name: &str) -> Option<&FieldLeaf> {
        self.leaves.iter().find(|l| l.name == name)
    }

    /// Index of a reference leaf at `byte_off` in the blob's `refs` side-table (bitmap
    /// order). Linear scan — reference leaves per struct are few.
    pub fn ref_index(&self, byte_off: u32) -> Option<usize> {
        self.ref_offsets.iter().position(|&o| o == byte_off)
    }

    /// **Three-layer validation** against the authoritative delivered `StructTypeLayout`.
    /// Any mismatch means the Rust replication drifted from the compiler's layout — bail
    /// with a catchable error rather than silently misreading bytes.
    ///
    /// 1. total size equal;
    /// 2. the sorted set of computed reference-leaf offsets equals the delivered
    ///    `ref_offsets` (catches size/alignment drift — a wrong field size cascades into
    ///    every following offset and the total size);
    /// 3. every leaf's ref/prim classification agrees with the delivered bitmap (a leaf we
    ///    call a reference must be in `ref_offsets`; a primitive must not be).
    pub fn validate_against(&self, delivered: &StructTypeLayout, type_name: &str) -> Result<()> {
        if self.size as usize != delivered.size {
            bail!(
                "struct field reflection: computed size {} != delivered layout size {} for `{type_name}` \
                 (Rust layout replication drifted — see corelib/struct_reflect.rs)",
                self.size, delivered.size
            );
        }
        let mut mine: Vec<u32> = self.ref_offsets.clone();
        mine.sort_unstable();
        let mut theirs: Vec<u32> = delivered.ref_offsets.to_vec();
        theirs.sort_unstable();
        if mine != theirs {
            bail!(
                "struct field reflection: computed reference-leaf offsets {mine:?} != delivered {theirs:?} \
                 for `{type_name}` (layout replication drift)"
            );
        }
        // Per-leaf classification cross-check (top-level leaves only; nested ref leaves are
        // covered by the flattened set check above).
        for l in &self.leaves {
            if l.is_struct { continue; }
            let in_bitmap = theirs.binary_search(&l.byte_off).is_ok();
            if l.is_ref != in_bitmap {
                bail!(
                    "struct field reflection: field `{}` of `{type_name}` classified {} but delivered bitmap says {} \
                     (layout replication drift)",
                    l.name,
                    if l.is_ref { "reference" } else { "primitive" },
                    if in_bitmap { "reference" } else { "primitive" }
                );
            }
        }
        Ok(())
    }
}

/// Replicate the compiler's `StructLayout._compute` for `type_name`, returning its
/// field-name → (offset, size, tag, kind) map + flattened reference-leaf offsets.
/// Recurses into nested struct fields (their reference leaves flatten into the parent,
/// offset-shifted). `depth` guards against a (compiler-rejected) self-referential cycle.
pub fn compute(resolve: &TypeResolver, type_name: &str) -> Result<ComputedLayout> {
    compute_inner(resolve, type_name, 0)
}

fn compute_inner(resolve: &TypeResolver, type_name: &str, depth: u32) -> Result<ComputedLayout> {
    if depth > 64 {
        bail!("struct field reflection: nesting too deep / cyclic value struct `{type_name}`");
    }
    let td = resolve(type_name)
        .ok_or_else(|| anyhow::anyhow!("struct field reflection: unknown type `{type_name}`"))?;

    let mut offset: u32 = 0;
    let mut max_align: u32 = 1;
    let mut leaves: Vec<FieldLeaf> = Vec::with_capacity(td.fields.len());
    let mut ref_offsets: Vec<u32> = Vec::new();

    for f in &td.fields {
        let ftype = &*f.type_tag;
        // A nested struct field's declared type is often a **short** name (`Point`, not
        // `Demo.Point`); resolve it relative to the declaring type's namespace so the
        // recursion + nested boxing use the fully-qualified name.
        let resolved = resolve_named(resolve, type_name, ftype);
        let is_struct = resolved.as_ref().map(|(t, _)| t.is_struct()).unwrap_or(false);
        let size: u32;
        let align: u32;
        if is_struct {
            let fq = resolved.unwrap().1;
            let nested = compute_inner(resolve, &fq, depth + 1)?;
            let a = if nested.align == 0 { 1 } else { nested.align };
            offset = align_up(offset, a);
            leaves.push(FieldLeaf {
                name: f.name.to_string(),
                byte_off: offset,
                size: nested.size,
                tag: ty::TAG_UNKNOWN,
                is_struct: true,
                is_ref: false,
                type_name: fq,
            });
            for r in &nested.ref_offsets {
                ref_offsets.push(offset + r);
            }
            size = nested.size;
            align = a;
        } else {
            let c = canon(ftype);
            let is_ref = leaf_is_ref(c);
            size = size_of(c, is_ref);
            align = if is_ref { 8 } else { size };
            offset = align_up(offset, align);
            let tag = tag_from_name(ftype);
            leaves.push(FieldLeaf {
                name: f.name.to_string(),
                byte_off: offset,
                size,
                tag,
                is_struct: false,
                is_ref,
                type_name: ftype.to_string(),
            });
            if is_ref {
                ref_offsets.push(offset);
            }
        }
        offset += size;
        if align > max_align {
            max_align = align;
        }
    }

    Ok(ComputedLayout {
        leaves,
        ref_offsets,
        size: align_up(offset, max_align),
        align: max_align,
    })
}

/// add-object-inline-struct-reflection (P4b-B): replicate the compiler's **class**
/// inline layout (`StructLayout._computeInlineLayout`,
/// `src/compiler/z42c.semantics/src/StructLayout.z42:203`) for a non-struct class.
///
/// Unlike a value struct (`compute`), a heap class only byte-packs its **value-struct**
/// fields into the object's `struct_bytes`/`struct_refs`; every non-struct field keeps a
/// real slot (a struct field also keeps a dead placeholder slot — see P3b stage 3). So
/// this walks the class's ordered `TypeDesc.fields`, skips non-struct fields, and places
/// each struct field at its object-relative byte offset (natural alignment), flattening
/// its reference leaves into the object-relative `ref_offsets` (indices into the object's
/// `struct_refs` side-table). Fields not present here are ordinary slots.
///
/// The result is validated by the caller against the authoritative delivered composed
/// `inline_layout` (`TypeDesc::inline_layout`) via `validate_against` — same three-layer
/// drift check as the value-struct path.
pub fn compute_class_inline(resolve: &TypeResolver, class_name: &str) -> Result<ComputedLayout> {
    let td = resolve(class_name).ok_or_else(|| {
        anyhow::anyhow!("class inline layout: unknown class `{class_name}`")
    })?;

    let mut offset: u32 = 0;
    let mut max_align: u32 = 1;
    let mut leaves: Vec<FieldLeaf> = Vec::new();
    let mut ref_offsets: Vec<u32> = Vec::new();

    for f in &td.fields {
        let ftype = &*f.type_tag;
        // Only value-struct fields are inlined; primitives / references stay in slots.
        let (sub_td, fq) = match resolve_named(resolve, class_name, ftype) {
            Some(pair) if pair.0.is_struct() => pair,
            _ => continue,
        };
        let _ = sub_td;
        let nested = compute_inner(resolve, &fq, 1)?;
        let align = if nested.align == 0 { 1 } else { nested.align };
        offset = align_up(offset, align);
        leaves.push(FieldLeaf {
            name: f.name.to_string(),
            byte_off: offset,
            size: nested.size,
            tag: ty::TAG_UNKNOWN,
            is_struct: true,
            is_ref: false,
            type_name: fq,
        });
        for r in &nested.ref_offsets {
            ref_offsets.push(offset + r);
        }
        offset += nested.size;
        if align > max_align {
            max_align = align;
        }
    }

    Ok(ComputedLayout {
        leaves,
        ref_offsets,
        size: align_up(offset, max_align),
        align: max_align,
    })
}

/// If `field_name` on class `owner_fq` is a value-struct field, return its fully-qualified
/// struct type name; otherwise `None` (primitive / reference field → ordinary slot). Used by
/// reflection to decide between the object-inline byte path and the plain-slot path.
pub fn struct_field_fq(
    resolve: &TypeResolver, owner_fq: &str, field_name: &str,
) -> Option<String> {
    let td = resolve(owner_fq)?;
    let f = td.fields.iter().find(|f| &*f.name == field_name)?;
    resolve_named(resolve, owner_fq, &f.type_tag)
        .filter(|(t, _)| t.is_struct())
        .map(|(_, fq)| fq)
}

/// Resolve a field's declared type name to `(TypeDesc, fully-qualified name)`. Tries the
/// name as-is (already FQ, or the loader handles it), then the declaring type's namespace +
/// the short name (`Demo.Line`'s field `Point` → `Demo.Point`). Returns `None` for
/// primitives / unresolved names (→ treated as non-struct by the caller).
fn resolve_named(
    resolve: &TypeResolver, current_type_fq: &str, name: &str,
) -> Option<(Arc<TypeDesc>, String)> {
    if let Some(td) = resolve(name) {
        return Some((td, name.to_string()));
    }
    if let Some(idx) = current_type_fq.rfind('.') {
        let fq = format!("{}.{}", &current_type_fq[..idx], name);
        if let Some(td) = resolve(&fq) {
            return Some((td, fq));
        }
    }
    None
}

// ── Mirror of the compiler's canon / sizing / tag functions ──────────────────

/// Mirror of `Z42Type.Canon` — strip a trailing `?` and normalize numeric aliases to
/// canonical `i*/u*/f*` spellings. Used for size/alignment/kind (never for the decode tag,
/// which mirrors `Tag.FromName` directly — see module docs).
fn canon(n: &str) -> &str {
    let s = n.strip_suffix('?').unwrap_or(n);
    match s {
        "byte" => "u8",
        "sbyte" => "i8",
        "short" => "i16",
        "ushort" => "u16",
        "int" => "i32",
        "uint" => "u32",
        "long" => "i64",
        "ulong" => "u64",
        "float" => "f32",
        "double" => "f64",
        other => other,
    }
}

fn is_prim(c: &str) -> bool {
    matches!(
        c,
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64" | "bool" | "char"
    )
}

/// Mirror of `StructLayout._kindOf` for a non-struct field: `string` and every non-primitive
/// (array / class / interface / func / unknown) is a 16-byte reference leaf; primitives are
/// byte-packed.
fn leaf_is_ref(canon: &str) -> bool {
    !is_prim(canon)
}

/// Mirror of `StructLayout._sizeOf`: reference = 16; `char` = 4 (Unicode scalar); else by width.
fn size_of(canon: &str, is_ref: bool) -> u32 {
    if is_ref {
        return 16;
    }
    match canon {
        "i8" | "u8" | "bool" => 1,
        "i16" | "u16" => 2,
        "i32" | "u32" | "f32" | "char" => 4,
        "i64" | "u64" | "f64" => 8,
        _ => 8, // unknown primitive fallback (mirrors the compiler)
    }
}

/// Mirror of `Tag.FromName` (`ZbcFormat.z42:75`) — the exact function the codegen used to
/// bake each leaf's tag. Fed the *declared* type string (not canon), so the decode signedness
/// / width matches what was encoded. `string`/array/class → `Object` (a reference tag; the
/// precise reference tag is immaterial — reference leaves route through the `refs` side-table).
fn tag_from_name(t: &str) -> u8 {
    match t {
        "void" => ty::TAG_UNKNOWN,
        "bool" => ty::TAG_BOOL,
        "i8" => ty::TAG_I8,
        "i16" => ty::TAG_I16,
        "i32" | "int" => ty::TAG_I32,
        "i64" | "long" => ty::TAG_I64,
        "u8" => ty::TAG_U8,
        "u16" => ty::TAG_U16,
        "u32" => ty::TAG_U32,
        "u64" => ty::TAG_U64,
        "f32" | "float" => ty::TAG_F32,
        "f64" | "double" => ty::TAG_F64,
        "char" => ty::TAG_CHAR,
        "str" => ty::TAG_STR,
        _ => ty::TAG_OBJECT,
    }
}

/// Round `off` up to a multiple of `align` (mirror of `StructLayout._alignUp`).
fn align_up(off: u32, align: u32) -> u32 {
    if align <= 1 {
        return off;
    }
    let rem = off % align;
    if rem == 0 {
        off
    } else {
        off + (align - rem)
    }
}

#[cfg(test)]
#[path = "struct_reflect_tests.rs"]
mod struct_reflect_tests;
