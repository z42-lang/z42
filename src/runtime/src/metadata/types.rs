use std::sync::Arc;

use crate::gc::GcRef;

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
pub fn default_value_for(type_tag: &str) -> Value {
    match type_tag {
        "int" | "long" | "short" | "byte" | "sbyte" | "ushort" | "uint" | "ulong"
        | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
        | "isize" | "usize" => Value::I64(0),
        "double" | "float" | "f32" | "f64" => Value::F64(0.0),
        "bool" => Value::Bool(false),
        "char" => Value::Char('\0'),
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
#[derive(Debug)]
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
#[derive(Debug, Default)]
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
    /// Field storage indexed by slot (see `TypeDesc.field_index`).
    ///
    /// review.md E2.P6 (2026-06-02): `Box<[Value]>` instead of `Vec<Value>` —
    /// slot count is fixed at `alloc_object` time (= `TypeDesc.fields.len()`)
    /// and never grows. Saves 8 B/object vs `Vec` (no `capacity` word).
    /// Mutation via `obj.slots[i] = v` still works; `&mut [Value]` indexing
    /// is unchanged.
    pub slots: Box<[Value]>,
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
#[derive(Debug, Clone)]
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
/// `Boxed` = reference array (`object[]`/`string[]`/nested), GC-scanned. The
/// primitive backings are packed value-type arrays (inline `Vec<T>`, no
/// per-element boxing, GC skips them). box/unbox happens only at the ArrayGet/
/// ArraySet boundary because interp registers are `Value` (Step 4 removes even
/// that for the JIT via unboxed access).
#[derive(Debug, Clone)]
pub enum ArrayBacking {
    Boxed(Vec<Value>),
    Bool(Vec<bool>),
    Bytes(Vec<u8>),      // byte / sbyte（窄整型并入；box 语义按 element_type）
    I32(Vec<i32>),       // int / uint / short / ushort
    I64(Vec<i64>),       // long / ulong
    Chars(Vec<char>),    // char（scalar，与 String.ToCharArray 对齐）
    F64(Vec<f64>),       // double / float
}

impl ArrayObj {
    /// Untyped array (element type unknown) — for Rust-synthesized arrays.
    #[inline]
    pub fn new(elems: Vec<Value>) -> Self {
        Self { element_type: Arc::from(""), backing: ArrayBacking::Boxed(elems) }
    }
    /// Array with a known element type (from `ArrayNew` / `ArrayNewLit`).
    /// **Step 1b-ii**: primitive element types get a packed value-type backing
    /// (C# model); everything else stays `Boxed`. Unknown/FQN element types fall
    /// back to `Boxed` (safe — no packing, correct behaviour).
    #[inline]
    pub fn typed(element_type: &str, elems: Vec<Value>) -> Self {
        Self { element_type: Arc::from(element_type), backing: Self::pack_backing(element_type, elems) }
    }

    /// FFI return fast-path (packed-primitive-arrays Step 3): build a `byte[]`
    /// straight from an owned `Vec<u8>` — no per-byte `Value::I64` boxing, no
    /// re-pack scan. The mirror of `as_bytes()` on the ingest side. This is the
    /// "简化 extern call" return path: native call → `&[u8]` → `byte[]` directly.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { element_type: Arc::from("byte"), backing: ArrayBacking::Bytes(bytes) }
    }

    /// Select a packed value-type backing for a primitive `element_type`,
    /// unboxing `elems` into it. Conservative + sign-safe: only widths that
    /// round-trip losslessly through `get_boxed`/`set_boxed` are packed.
    fn pack_backing(element_type: &str, elems: Vec<Value>) -> ArrayBacking {
        match element_type {
            // byte[] → contiguous u8: the FFI zero-copy + 24× memory win.
            "byte" | "u8" =>
                ArrayBacking::Bytes(elems.iter().map(|v| if let Value::I64(n) = v { *n as u8 } else { 0 }).collect()),
            "char" =>
                ArrayBacking::Chars(elems.iter().map(|v| if let Value::Char(c) = v { *c } else { '\0' }).collect()),
            "bool" =>
                ArrayBacking::Bool(elems.iter().map(|v| matches!(v, Value::Bool(true))).collect()),
            // fits i32 signed range (i8/i16/i32 and u16 ≤ 65535).
            "sbyte" | "i8" | "short" | "i16" | "int" | "i32" | "ushort" | "u16" =>
                ArrayBacking::I32(elems.iter().map(|v| if let Value::I64(n) = v { *n as i32 } else { 0 }).collect()),
            // 64-bit (uint/u32 fit i64; u64 keeps existing i64-store semantics).
            "long" | "i64" | "uint" | "u32" | "ulong" | "u64" | "isize" | "usize" =>
                ArrayBacking::I64(elems.iter().map(|v| if let Value::I64(n) = v { *n } else { 0 }).collect()),
            "double" | "float" | "f32" | "f64" =>
                ArrayBacking::F64(elems.iter().map(|v| if let Value::F64(f) = v { *f } else { 0.0 }).collect()),
            // object / string / nested arrays / structs / unknown FQN → reference array.
            _ => ArrayBacking::Boxed(elems),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match &self.backing {
            ArrayBacking::Boxed(v) => v.len(),
            ArrayBacking::Bool(v)  => v.len(),
            ArrayBacking::Bytes(v) => v.len(),
            ArrayBacking::I32(v)   => v.len(),
            ArrayBacking::I64(v)   => v.len(),
            ArrayBacking::Chars(v) => v.len(),
            ArrayBacking::F64(v)   => v.len(),
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
    /// `i < len()`.
    #[inline]
    pub fn get_boxed(&self, i: usize) -> Value {
        match &self.backing {
            ArrayBacking::Boxed(v) => v[i].clone(),
            ArrayBacking::Bool(v)  => Value::Bool(v[i]),
            ArrayBacking::Bytes(v) => Value::I64(v[i] as i64),
            ArrayBacking::I32(v)   => Value::I64(v[i] as i64),
            ArrayBacking::I64(v)   => Value::I64(v[i]),
            ArrayBacking::Chars(v) => Value::Char(v[i]),
            ArrayBacking::F64(v)   => Value::F64(v[i]),
        }
    }
    /// Write `Value` into element `i` (unboxes into packed primitives). Caller
    /// ensures `i < len()`.
    #[inline]
    pub fn set_boxed(&mut self, i: usize, val: Value) {
        match &mut self.backing {
            ArrayBacking::Boxed(v) => v[i] = val,
            ArrayBacking::Bool(v)  => v[i] = matches!(val, Value::Bool(true)),
            ArrayBacking::Bytes(v) => v[i] = if let Value::I64(n) = val { n as u8 } else { 0 },
            ArrayBacking::I32(v)   => v[i] = if let Value::I64(n) = val { n as i32 } else { 0 },
            ArrayBacking::I64(v)   => v[i] = if let Value::I64(n) = val { n } else { 0 },
            ArrayBacking::Chars(v) => v[i] = if let Value::Char(c) = val { c } else { '\0' },
            ArrayBacking::F64(v)   => v[i] = if let Value::F64(f) = val { f } else { 0.0 },
        }
    }

    /// Materialise all elements as a `Vec<Value>` (for sites needing a boxed
    /// snapshot — reflection, conversions). Boxed backing clones; packed boxes.
    pub fn to_boxed_vec(&self) -> Vec<Value> {
        match &self.backing {
            ArrayBacking::Boxed(v) => v.clone(),
            _ => (0..self.len()).map(|i| self.get_boxed(i)).collect(),
        }
    }
    /// The boxed element slice iff this is a reference array — GC scans only
    /// this; packed primitive backings hold no heap refs (`None`).
    #[inline]
    pub fn boxed_slice(&self) -> Option<&[Value]> {
        match &self.backing { ArrayBacking::Boxed(v) => Some(v), _ => None }
    }
    #[inline]
    pub fn boxed_slice_mut(&mut self) -> Option<&mut Vec<Value>> {
        match &mut self.backing { ArrayBacking::Boxed(v) => Some(v), _ => None }
    }

    /// Zero-copy packed byte slice for FFI (`Some` iff `byte[]`). Step 3 uses
    /// this to hand native code a contiguous `&[u8]` — no per-byte marshal.
    #[inline]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match &self.backing { ArrayBacking::Bytes(v) => Some(v), _ => None }
    }

    /// JIT packed-numeric fast path: `I32`/`I64`/`F64` backings are contiguous
    /// fixed-width slots (4 / 8 / 8 bytes) the JIT can index with a native
    /// stride-N load/store — no 24-byte `Value` round-trip, no per-element tag.
    /// Pairs with [`Self::packed_elem_width`]: the ptr is the buffer base, the
    /// width tells the JIT the slot size (4 → `int[]` sign-extends into the i64
    /// payload; 8 → raw `long[]`/`double[]` copy). `None` for `Boxed`/`Bytes`/
    /// `Bool`/`Chars` — the JIT set-path detects width 0 and falls back to the
    /// `jit_array_set` helper, so those backings never index off this ptr.
    #[inline]
    pub fn packed_num_ptr(&self) -> Option<*const u8> {
        match &self.backing {
            ArrayBacking::I32(v) => Some(v.as_ptr() as *const u8),
            ArrayBacking::I64(v) => Some(v.as_ptr() as *const u8),
            ArrayBacking::F64(v) => Some(v.as_ptr() as *const u8),
            // jit-inline-char-arrays: `char` is a 4-byte scalar (Rust `char` ==
            // u32); the JIT loads it width-4 and boxes into `Value::Char`.
            ArrayBacking::Chars(v) => Some(v.as_ptr() as *const u8),
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
            ArrayBacking::I32(_) | ArrayBacking::Chars(_) => 4,
            ArrayBacking::I64(_) | ArrayBacking::F64(_) => 8,
            _ => 0,
        }
    }

    /// Iterate all elements as owned `Value`s (boxes packed primitives).
    /// Packed-safe replacement for the old `Deref`→`Vec<Value>` `.iter()`.
    #[inline]
    pub fn iter_boxed(&self) -> impl Iterator<Item = Value> + '_ {
        (0..self.len()).map(move |i| self.get_boxed(i))
    }

    #[inline]
    pub fn clear(&mut self) {
        match &mut self.backing {
            ArrayBacking::Boxed(v) => v.clear(),
            ArrayBacking::Bool(v)  => v.clear(),
            ArrayBacking::Bytes(v) => v.clear(),
            ArrayBacking::I32(v)   => v.clear(),
            ArrayBacking::I64(v)   => v.clear(),
            ArrayBacking::Chars(v) => v.clear(),
            ArrayBacking::F64(v)   => v.clear(),
        }
    }
    #[inline]
    pub fn capacity(&self) -> usize {
        match &self.backing {
            ArrayBacking::Boxed(v) => v.capacity(),
            ArrayBacking::Bool(v)  => v.capacity(),
            ArrayBacking::Bytes(v) => v.capacity(),
            ArrayBacking::I32(v)   => v.capacity(),
            ArrayBacking::I64(v)   => v.capacity(),
            ArrayBacking::Chars(v) => v.capacity(),
            ArrayBacking::F64(v)   => v.capacity(),
        }
    }
    /// Heap bytes for element storage (`capacity × sizeof(element)`) — the
    /// packed-array memory win shows up here (byte[] 1B vs Boxed 24B/elem).
    #[inline]
    pub fn elem_storage_bytes(&self) -> usize {
        use std::mem::size_of;
        match &self.backing {
            ArrayBacking::Boxed(v) => v.capacity() * size_of::<Value>(),
            ArrayBacking::Bool(v)  => v.capacity(),
            ArrayBacking::Bytes(v) => v.capacity(),
            ArrayBacking::I32(v)   => v.capacity() * 4,
            ArrayBacking::I64(v)   => v.capacity() * 8,
            ArrayBacking::Chars(v) => v.capacity() * 4,
            ArrayBacking::F64(v)   => v.capacity() * 8,
        }
    }
}

#[derive(Debug, Clone)]
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
    Str(Arc<str>)               = 4,
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
    PinnedView(Box<PinnedViewData>) = 8,
    /// Function reference value. Currently used by L2 no-capture lambda
    /// literals (see docs/design/language/closure.md §6). Indirect call dispatches
    /// to the named function in the loaded module.
    ///
    /// review.md C1 chunk 2 (2026-05-27): `Box<str>` instead of `String`.
    /// Saves 8 B/instance (Box<str> = 16 B vs String = 24 B; no `cap` word).
    /// FuncRef names are write-once at creation and read-only thereafter
    /// (immutable identity → no append/grow operation needed).
    FuncRef(Box<str>) = 9,
    /// L3 capturing closure value: pairs a heap-allocated env (Vec<Value>)
    /// with the lifted function's qualified name. CallIndirect on a Closure
    /// passes `env` as the callee's first implicit parameter and copies user
    /// args after it. See docs/design/language/closure.md §6 + impl-closure-l3-core.
    ///
    /// review.md C1 chunk 5 (2026-05-27): payload boxed (the last and
    /// biggest cold-path variant — 40 B inline = GcRef(16 B) + String(24 B)).
    /// Boxing drops Value enum to ~24 B; capturing closures pay one heap
    /// alloc per `MkClos` but that's dwarfed by the env's own GC alloc.
    Closure(Box<ClosureData>) = 10,
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
    StackClosure(Box<StackClosureData>) = 11,
    /// Spec impl-ref-out-in-runtime: `ref` / `out` / `in` 参数运行时表达。
    /// 持有该 Value 的寄存器在 frame.get/set 时被透明 deref（单点 dispatch，
    /// 见 `interp/mod.rs::Frame::get`）。引用永远不离开调用栈帧（前置 spec
    /// design Decision 9 + R1），因此 Stack kind 的 frame_idx 不会 stale。
    ///
    /// review.md C1 chunk 4 (2026-05-27): payload boxed because RefKind
    /// is 32 B (Field variant) — biggest cold-path payload after
    /// Closure. Refs only live in registers for a single call's
    /// duration, so the box alloc is a tiny fraction of the call cost.
    Ref(Box<RefKind>) = 12,
    /// add-primitive-value-boxing: 基元值装箱——把裸基元（`inner`）连同其**精确基元
    /// struct 类型名**（`class`，如 `Std.Int64`）装进堆值，使 `object`/接口 槽保留精确
    /// 类型（强类型 `is`/`as`/`GetType`/`vcall`）。仅在 prim→object/接口 转换点由 `Box`
    /// 指令创建；`Unbox`（object→prim）取回 `inner`。算术/方法体永远拿拆箱后的 `inner`，
    /// 故热路径零影响。Box<…> = 8B 指针，不撑大 Value。
    Boxed(Box<BoxedPrim>) = 13,
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
}

/// add-primitive-value-boxing: 装箱基元载荷。`class` = FQ 基元 struct 名（`Std.Int32`/
/// `Std.Int64`/`Std.Byte`/…），供 `is`/`as`/`GetType`/`vcall` 走真 type_desc；`inner` =
/// 裸基元值（I64/F64/Bool/Char/Str）。
#[derive(Debug, Clone)]
pub struct BoxedPrim {
    pub class: std::sync::Arc<str>,
    pub inner: Value,
}

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

/// Payload of [`Value::Closure`] — boxed (review.md C1 chunk 5,
/// 2026-05-27) so the inline `Value` doesn't carry the 40-byte
/// GcRef + String pair. `MkClos` (heap-alloc path) constructs one;
/// `CallIndirect`, `__delegate_target`, `__delegate_fn_name`,
/// `__delegate_eq` and the GC scanner consume.
#[derive(Debug, Clone)]
pub struct ClosureData {
    pub env: GcRef<ArrayObj>,
    pub fn_name: String,
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
    #[inline]
    pub fn is_heap_ref(&self) -> bool {
        match self {
            Value::Object(_) | Value::Array(_) | Value::Closure(_) => true,
            Value::Ref(kind) => matches!(
                kind.as_ref(),
                RefKind::Array { .. } | RefKind::Field { .. }
            ),
            _ => false,
        }
    }

    /// **add-mark-sweep-collector P1 (2026-05-21)**: visit every direct
    /// child `Value` reachable from `self`. Used by the mark phase BFS
    /// to extend reachability through reference-bearing variants
    /// (Object slots, Array elements, Closure env, Ref::Array/Field
    /// inner `GcRef`).
    ///
    /// Primitives (I64 / F64 / Bool / Char / Str / Null / FuncRef /
    /// PinnedView / StackClosure / Ref::Stack) yield no children.
    /// Mirrors `ArcMagrGC::scan_object_refs` (will become its
    /// authoritative source once trial-deletion is removed in P3).
    pub fn trace_children(&self, visit: &mut dyn FnMut(&Value)) {
        match self {
            Value::Object(rc) => {
                let obj = rc.borrow();
                for slot in &obj.slots { visit(slot); }
            }
            Value::Array(rc) => {
                let arr = rc.borrow();
                if let Some(s) = arr.boxed_slice() { for elem in s { visit(elem); } }
            }
            Value::Closure(c) => {
                let arr = c.env.borrow();
                if let Some(s) = arr.boxed_slice() { for elem in s { visit(elem); } }
            }
            Value::Ref(kind) => match kind.as_ref() {
                RefKind::Stack { .. } => {}
                RefKind::Array { gc_ref, .. } => {
                    let arr = gc_ref.borrow();
                    if let Some(s) = arr.boxed_slice() { for elem in s { visit(elem); } }
                }
                RefKind::Field { gc_ref, .. } => {
                    let obj = gc_ref.borrow();
                    for slot in &obj.slots { visit(slot); }
                }
            },
            // add-primitive-value-boxing: 装箱基元——inner 为裸基元（无 GcRef），
            // 保守追踪一层（若未来 inner 承载堆值也安全）。
            Value::Boxed(b) => visit(&b.inner),
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
            | Value::PinnedView(_) | Value::StackClosure(_)
            | Value::StackObject { .. } | Value::StackArray { .. }
            | Value::StructRef { .. } => {}
        }
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
            (Value::PinnedView(a), Value::PinnedView(b)) => {
                a.ptr == b.ptr && a.len == b.len && a.kind == b.kind
            }
            // Spec impl-ref-out-in-runtime: Ref 比较按 RefKind 字段；
            // Array/Field kind 用 GcRef::ptr_eq（同 Object/Array 引用语义）；
            // Stack kind 比 frame_idx + slot（指向同一栈位置）。
            (Value::Ref(a), Value::Ref(b)) => match (&**a, &**b) {
                (RefKind::Stack { frame_idx: f1, slot: s1 },
                 RefKind::Stack { frame_idx: f2, slot: s2 }) => f1 == f2 && s1 == s2,
                (RefKind::Array { gc_ref: g1, idx: i1 },
                 RefKind::Array { gc_ref: g2, idx: i2 }) => GcRef::ptr_eq(g1, g2) && i1 == i2,
                (RefKind::Field { gc_ref: g1, field_name: n1 },
                 RefKind::Field { gc_ref: g2, field_name: n2 }) => GcRef::ptr_eq(g1, g2) && n1 == n2,
                _ => false,
            },
            // add-primitive-value-boxing: 装箱基元按 inner 值相等（class 精确性由 is/as 保证，
            // 值相等只看载荷）。boxed vs 未装箱基元也按 inner 比较（透明拆箱）。
            (Value::Boxed(a), Value::Boxed(b)) => a.inner == b.inner,
            (Value::Boxed(a), other) => &a.inner == other,
            (other, Value::Boxed(b)) => other == &b.inner,
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
