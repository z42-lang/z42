//! TypeDesc / TypeDescCold + impl（运行时类型描述符，≈ CoreCLR MethodTable）。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

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
    pub id: crate::metadata::tokens::TypeId,
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
    pub field_index: crate::metadata::name_index::NameIndex,
    /// Virtual method table: slot → (simple_method_name, qualified_func_name).
    /// Derived class overrides replace base entries at the same slot index.
    /// Same cross-zpkg fixup semantics as `fields`.
    pub vtable: Vec<(String, String)>,
    /// `method_name → vtable slot index` — linear scan (review.md C5 P1,
    /// 2026-06-01). Same rationale as `field_index`.
    pub vtable_index: crate::metadata::name_index::NameIndex,
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
    /// fix-value-type-object-methods ③: per-entry `is_static` flag, index-aligned
    /// with `own_methods`. Used by `merge_with_base` to keep **static** methods out
    /// of the instance vtable — a static method (e.g. `Type.GetType(string)`) whose
    /// simple name (`GetType`) collides with an inherited Object method must NOT
    /// override that vtable slot, else instance `t.GetType()` dispatches to the
    /// static extern (receiver mis-passed as the arg → null). Empty (older modules /
    /// synthetic descriptors) → all treated non-static (prior behavior). Reflection
    /// (`GetMethods`) still sees statics via the full `own_methods`.
    pub own_static_flags: Box<[bool]>,
    /// Generic type parameter names: ["T"], ["K", "V"]. Empty for non-generic classes.
    pub type_params: Box<[String]>,
    /// Concrete type arguments for an instantiated generic class: ["int"], ["string", "int"].
    /// Empty for non-generic classes and uninstantiated generic definitions.
    pub type_args: Box<[String]>,
    /// L3-G3a: constraint bundle per type parameter (aligned by index with `type_params`).
    /// Empty for non-generic classes; inner bundle may be empty for unconstrained params.
    pub type_param_constraints: Box<[crate::metadata::bytecode::ConstraintBundle]>,
    /// C3 add-attribute-reflection: user attributes applied to this class
    /// (carried from the zbc TYPE section). Each is (attribute-type qualified
    /// name, factory-func qualified name). `__type_custom_attributes` calls each
    /// factory once and caches the resulting instances on the Type object.
    pub custom_attributes: Box<[crate::metadata::bytecode::AttributeRef]>,
    /// add-reflection-static-fields (zbc 1.13): the class's static fields
    /// (separate from hot `TypeDesc::fields`, the instance layout). Reflection
    /// only — surfaced by `Type.GetFields()` with `FieldInfo.IsStatic = true`.
    pub static_fields: Box<[crate::metadata::bytecode::FieldDesc]>,
    /// add-field-attribute-reflection (zbc 1.14): per-field user-attribute refs,
    /// indexed by field name (instance + static fields with attributes).
    /// `__field_custom_attributes` resolves a field's factories here.
    /// Reflection only; empty for classes with no field attributes.
    pub field_attributes: Box<[(Box<str>, Box<[crate::metadata::bytecode::AttributeRef]>)]>,
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
    pub iface_methods: Box<[crate::metadata::bytecode::IfaceMethodSig]>,
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
    pub object_layout: Option<std::sync::Arc<crate::metadata::bytecode::ObjectLayoutDesc>>,
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
    #[inline] pub fn own_static_flags(&self)       -> &[bool]                                   { self.cold_slice(|c| &c.own_static_flags) }
    #[inline] pub fn type_params(&self)            -> &[String]                                 { self.cold_slice(|c| &c.type_params) }
    #[inline] pub fn type_args(&self)              -> &[String]                                 { self.cold_slice(|c| &c.type_args) }
    #[inline] pub fn type_param_constraints(&self) -> &[crate::metadata::bytecode::ConstraintBundle]      { self.cold_slice(|c| &c.type_param_constraints) }
    /// C3 add-attribute-reflection: user attributes applied to this class.
    #[inline] pub fn custom_attributes(&self)      -> &[crate::metadata::bytecode::AttributeRef]          { self.cold_slice(|c| &c.custom_attributes) }
    /// add-reflection-static-fields: the class's static fields (reflection only).
    #[inline] pub fn static_fields(&self)          -> &[crate::metadata::bytecode::FieldDesc]             { self.cold_slice(|c| &c.static_fields) }
    /// add-interface-member-reflection: the interface's declared method signatures.
    #[inline] pub fn iface_methods(&self)          -> &[crate::metadata::bytecode::IfaceMethodSig]        { self.cold_slice(|c| &c.iface_methods) }
    /// add-field-attribute-reflection: per-field attr refs (field name → refs).
    #[inline] pub fn field_attributes(&self)       -> &[(Box<str>, Box<[crate::metadata::bytecode::AttributeRef]>)] { self.cold_slice(|c| &c.field_attributes) }
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
    #[inline] pub fn is_struct(&self)             -> bool { self.class_flags & crate::metadata::bytecode::CLASS_FLAG_STRUCT != 0 }
    /// add-record-value-semantics: whether this type is a `[Record]` (Type.IsRecord). Used by the
    /// boxed-struct vcall arm to step aside from the native ToString intercept so a record struct's
    /// compiler-synthesized `<Type>.ToString` (record format) is reached instead of the type name.
    #[inline] pub fn is_record(&self)             -> bool { self.class_flags & crate::metadata::bytecode::CLASS_FLAG_RECORD != 0 }
    /// add-enum-type-metadata: whether this type is an enum (Type.IsEnum).
    #[inline] pub fn is_enum(&self)                -> bool { self.class_flags & crate::metadata::bytecode::CLASS_FLAG_ENUM != 0 }
    #[inline] pub fn is_delegate(&self)            -> bool { self.class_flags & crate::metadata::bytecode::CLASS_FLAG_DELEGATE != 0 }

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
        // stabilize-instance-dispatch-keys (H4): return the FULL method key (do NOT strip
        // `$…`). Under primary/non-primary keying one class can have `F` (primary, bare =
        // canonical slot) + `F$1$string` + `F$2$i32$i32` (non-primary, full keys). Stripping
        // to the simple name "F" collapsed all three onto one method-table/vtable slot →
        // last-wins → a resolved call `o.F(5)` mis-dispatched to whichever sibling won the
        // "F" slot (observed: F(int) call → F(string) body). Keeping the full key gives each
        // overload its own slot; the bare primary keeps "F" for polymorphic/canonical
        // dispatch, and base/derived overrides align by identical full key.
        after_prefix
    }
}

// ── NativeData — native backing for built-in class types ────────────────────
//
// Analogous to CoreCLR's inline data in String/Array objects.
// Provides a native backing store for classes that wrap VM primitives.
