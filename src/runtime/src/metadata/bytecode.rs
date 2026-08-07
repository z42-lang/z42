use super::tokens::TypeId;
use super::types::{ExecMode, TypeDesc};
use super::bytecode_serde::{typed_reg_serde, typed_reg_vec_serde, typed_reg_opt_serde};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Top-level bytecode module.
/// Loaded from `.zbc` binary (or legacy `.z42ir.json`).
#[derive(Debug, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub string_pool: Vec<String>,
    #[serde(default)]
    pub classes: Vec<ClassDesc>,
    pub functions: Vec<Function>,
    /// Pre-built type descriptor registry — populated by the loader after
    /// deserialisation, not stored on disk.  Maps fully-qualified class name
    /// to the corresponding `TypeDesc` (field layout + vtable).
    #[serde(skip)]
    pub type_registry: HashMap<String, Arc<TypeDesc>>,
    /// Phase 3 S1 (`tokenize-ir-and-zbc-bump`, 2026-05-09): parallel
    /// by-`TypeId` view of the type registry. Index `i` holds the `Arc<TypeDesc>`
    /// whose `id == TypeId(i as u32)`. Built alongside `type_registry` by
    /// `build_type_registry` (intra-module classes) and extended by
    /// `register_lazy_type` (cross-zpkg lazy load).
    ///
    /// In S1 this is observability infrastructure — consumers still go
    /// through `type_registry` (HashMap by-name). S4 will switch hot paths
    /// to `type_by_id()` once IR fields are tokenised.
    #[serde(skip)]
    pub type_registry_vec: Vec<Arc<TypeDesc>>,
    /// Pre-built function name → index mapping for O(1) call dispatch.
    /// Populated by the loader after deserialisation.
    #[serde(skip)]
    pub func_index: HashMap<String, usize>,
    /// 2026-05-02 add-method-group-conversion (D1b): number of FuncRef cache
    /// slots required by `LoadFnCached` instructions. VM allocates a parallel
    /// `Vec<Value>` of this size on `VmContext` at module load.
    #[serde(default)]
    pub func_ref_cache_slots: u32,

    /// review.md C3 / Part 5 P3 Phase 1 (2026-06-03,
    /// add-string-literal-interning-phase1): per-pool-slot interned `Arc<str>`.
    /// Populated by the loader after deserialize (parallel to
    /// `string_pool`). `ConstStr` instructions clone from here (atomic
    /// refcount increment, zero heap allocation) instead of cloning the
    /// underlying `String` + converting to `Arc<str>` (two allocations).
    /// Stdlib hot literals like `"Length"` / `"ToString"` are now interned
    /// per-module to a single `Arc<str>`. Empty until populated by loader.
    #[serde(skip)]
    pub interned_strings: Vec<std::sync::Arc<str>>,
}

impl Module {
    /// Phase 3 S1 (`tokenize-ir-and-zbc-bump`, 2026-05-09): O(1) by-`TypeId`
    /// type lookup. Invariant: `type_registry_vec[id.0] == registry[name]`
    /// where `name` is the FQ class name of that TypeDesc, maintained by
    /// `loader::build_type_registry` and `Module::register_lazy_type`.
    #[inline]
    pub fn type_by_id(&self, id: TypeId) -> Option<&Arc<TypeDesc>> {
        if !id.is_resolved() { return None; }
        self.type_registry_vec.get(id.0 as usize)
    }

    /// Append a lazily-loaded TypeDesc to both views (Vec and HashMap),
    /// assigning the next available `TypeId.0`. Returns the assigned id.
    /// Used by `lazy_loader` for cross-zpkg type resolution.
    ///
    /// Caller responsibility: the input `Arc<TypeDesc>` may carry its own
    /// `id` (from another module's build_type_registry); this method
    /// **rebuilds** the Arc with the freshly-allocated module-local id so
    /// downstream `td.id` checks remain consistent. If the type is already
    /// present (by-name match), returns the existing id without modification.
    pub fn register_lazy_type(&mut self, td: Arc<TypeDesc>) -> TypeId {
        if let Some(existing) = self.type_registry.get(&td.name) {
            return existing.id;
        }
        let new_id = TypeId(self.type_registry_vec.len() as u32);
        // Rebuild with the new module-local id (TypeDesc.id is a single u32 —
        // cheap to clone the rest by Arc-internals walking).
        let cold = td.cold.as_deref().map(|c| Box::new(crate::metadata::types::TypeDescCold {
            own_fields:             c.own_fields.clone(),
            own_methods:            c.own_methods.clone(),
            type_params:            c.type_params.clone(),
            type_args:              c.type_args.clone(),
            type_param_constraints: c.type_param_constraints.clone(),
            custom_attributes:      c.custom_attributes.clone(),
            static_fields:          c.static_fields.clone(),
            field_attributes:       c.field_attributes.clone(),
            interfaces:             c.interfaces.clone(),
            enum_members:           c.enum_members.clone(),
            iface_methods:          c.iface_methods.clone(),
        }));
        let rebuilt = Arc::new(TypeDesc {
            name: td.name.clone(),
            id: new_id,
            base_name: td.base_name.clone(),
            class_flags: td.class_flags,
            fields: td.fields.clone(),
            field_index: td.field_index.clone(),
            vtable: td.vtable.clone(),
            vtable_index: td.vtable_index.clone(),
            cold,
        });
        self.type_registry.insert(rebuilt.name.clone(), rebuilt.clone());
        self.type_registry_vec.push(rebuilt);
        new_id
    }
}

/// Class descriptor — field layout for object allocation.
///
/// review.md E5 follow-up (2026-05-27): three immutable-after-construction
/// fields stored as `Box<[T]>` (16 B) instead of `Vec<T>` (24 B) — saves
/// 8 B/field/ClassDesc. TypeDesc still owns growable `Vec`s because the
/// cross-zpkg fixup pass rebuilds them.
/// add-reflection-type-flags (zbc 1.12): bit layout for the TYPE-section class
/// flags byte (`ClassDesc::class_flags` / `TypeDesc::class_flags`). Must match
/// ZbcWriter.BuildTypeSection (1=abstract, 2=sealed, 4=struct, 8=record).
pub const CLASS_FLAG_ABSTRACT: u8 = 1 << 0;
pub const CLASS_FLAG_SEALED: u8 = 1 << 1;
pub const CLASS_FLAG_STRUCT: u8 = 1 << 2;
pub const CLASS_FLAG_RECORD: u8 = 1 << 3;
/// add-reflection-interface-class-predicates (zbc 1.19): set on the minimal
/// TYPE entry emitted for an `interface`. Backs `Type.IsInterface`; excluded
/// from `Type.IsClass`.
pub const CLASS_FLAG_INTERFACE: u8 = 1 << 4;
/// add-enum-type-metadata (zbc 1.22): set on the TYPE entry emitted for an
/// `enum`. Backs `Type.IsEnum`; when set, the class record carries a trailing
/// enum-member block (member_count:u16 + (name_idx:u32, value:i64)×n).
pub const CLASS_FLAG_ENUM: u8 = 1 << 5;

/// add-delegate-metadata (unify P1-e, zbc 1.26): the class record describes a
/// `delegate` (delegate-as-class: TYPE entry + synthesized `<FQ>.Invoke` dead
/// stub carries the signature). Backs `Type.IsDelegate`. No extra payload.
pub const CLASS_FLAG_DELEGATE: u8 = 1 << 6;

/// SIGS `method_flags` bits (add-method-modifiers, unify P1-c). Backs
/// `MethodInfo.IsVirtual` (authoritative) / `IsAbstract`. `static` is NOT here
/// — it stays in the dedicated `is_static` byte (single source of truth).
pub const METHOD_FLAG_VIRTUAL: u8 = 1 << 0;
pub const METHOD_FLAG_ABSTRACT: u8 = 1 << 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClassDesc {
    pub name: String,
    #[serde(default)]
    pub base_class: Option<String>,
    pub fields: Box<[FieldDesc]>,
    /// Generic type parameter names: ["T"], ["K", "V"]. Empty for non-generic classes.
    #[serde(default)]
    pub type_params: Box<[String]>,
    /// L3-G3a: constraint bundle per type parameter. When non-empty must align with
    /// `type_params` by index. Absent entries in old zbc deserialise as empty box.
    #[serde(default)]
    pub type_param_constraints: Box<[ConstraintBundle]>,
    /// C3 add-attribute-reflection: user attributes applied to this class.
    /// Each points at a synthesized factory function the runtime calls (lazily,
    /// cached) to build the attribute instance for `Type.GetCustomAttributes()`.
    #[serde(default)]
    pub attributes: Box<[AttributeRef]>,
    /// add-reflection-type-flags (zbc 1.12): class-shape flags (see
    /// CLASS_FLAG_* above). Threaded into `TypeDesc::class_flags` for
    /// `Type.IsAbstract` / `Type.IsSealed` reflection.
    #[serde(default)]
    pub class_flags: u8,
    /// add-reflection-static-fields (zbc 1.13): the class's static fields
    /// (separate from `fields`, which is the instance layout). Threaded into
    /// `TypeDescCold::static_fields`; surfaced by `Type.GetFields()` with
    /// `FieldInfo.IsStatic = true`.
    #[serde(default)]
    pub static_fields: Box<[FieldDesc]>,
    /// add-reflection-get-interfaces (zbc 1.17): the interface names this class
    /// directly declares (bare; e.g. "IFoo"). Threaded into
    /// `TypeDescCold::interfaces`; surfaced by `Type.GetInterfaces()` (which
    /// base-walks for inherited interfaces).
    #[serde(default)]
    pub interfaces: Box<[String]>,
    /// add-enum-type-metadata (zbc 1.22): enum member (name, i64 value) pairs,
    /// present only when `class_flags & CLASS_FLAG_ENUM`. Threaded into
    /// `TypeDesc::enum_members`; surfaced by `Type.IsEnum` / `Enum.GetNames` /
    /// `Enum.GetValues` / `Enum.GetName`. Empty for non-enum classes.
    #[serde(default)]
    pub enum_members: Box<[(String, i64)]>,
    /// add-interface-member-reflection (surfaces the zbc 1.28 interface method
    /// block, previously parsed-and-discarded): the interface's directly-declared
    /// method signatures, present only when `class_flags & CLASS_FLAG_INTERFACE`.
    /// Threaded into `TypeDesc::iface_methods`; surfaced by `Type.GetMethods()`.
    /// Empty for non-interface classes.
    #[serde(default)]
    pub iface_methods: Box<[IfaceMethodSig]>,
}

/// add-interface-member-reflection: one interface-declared method signature,
/// recovered from the zbc 1.28 interface method block. Interface methods have no
/// backing `Function` (no body), so reflection builds their `MethodInfo` straight
/// from this signature (name / return type / parameter types).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfaceMethodSig {
    /// Source-level method name (may carry a `$N$types` dispatch-mangle suffix;
    /// reflection strips it for the user-facing `MethodInfo.Name`).
    pub name: String,
    /// Return type name (e.g. "double" / "void" / "geometry.Point").
    pub ret_type: String,
    /// Parameter type names, in declaration order (no `this`).
    pub param_types: Box<[String]>,
}

/// C3 add-attribute-reflection: one applied attribute — the attribute class's
/// qualified name plus the qualified name of the compiler-synthesized
/// `() => new T(args)` factory function (resolved against the func index).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeRef {
    pub type_name: String,
    pub factory_func: String,
}

/// Resolved constraint bundle for one generic type parameter. (L3-G3a, L3-G2.5 bare-tp)
/// Mirrors the C# `GenericConstraintBundle` on the semantic layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConstraintBundle {
    #[serde(default)]
    pub requires_class: bool,
    #[serde(default)]
    pub requires_struct: bool,
    #[serde(default)]
    pub base_class: Option<String>,
    #[serde(default)]
    pub interfaces: Vec<String>,
    /// L3-G2.5 bare-typeparam: name of another type parameter in the same decl
    /// that this parameter must be a subtype of. None when no such constraint.
    #[serde(default)]
    pub type_param_constraint: Option<String>,
    /// L3-G2.5 ctor: `where T: new()` — type arg must have a no-arg constructor.
    #[serde(default)]
    pub requires_constructor: bool,
    /// L3-G2.5 enum: `where T: enum` — type arg must be an enum type.
    #[serde(default)]
    pub requires_enum: bool,
    /// add-generic-func-constraint (2026-05-11): function-type signature.
    /// `params` are IR type-name strings (e.g. "int", "string", "Cat"); `ret` is
    /// likewise a type name ("void" / "int" / etc.). None when no func constraint.
    #[serde(default)]
    pub func_signature: Option<FuncSigDescriptor>,
}

/// add-generic-func-constraint (2026-05-11): per-tp function signature spelled
/// as type-name strings (so zbc serialization is uniform with other constraint
/// fields that hold class/interface names).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FuncSigDescriptor {
    pub params: Vec<String>,
    pub ret: String,
}

impl ConstraintBundle {
    pub fn is_empty(&self) -> bool {
        !self.requires_class && !self.requires_struct
            && self.base_class.is_none() && self.interfaces.is_empty()
            && self.type_param_constraint.is_none()
            && !self.requires_constructor
            && !self.requires_enum
            && self.func_signature.is_none()
    }
}

/// A single field in a class descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDesc {
    pub name: String,
    #[serde(rename = "type")]
    pub type_tag: String,
    /// add-field-attribute-reflection (zbc 1.14): user attributes applied to
    /// this field. Surfaced by `FieldInfo.GetCustomAttributes()` (the loader
    /// indexes these into `TypeDescCold::field_attributes`).
    #[serde(default)]
    pub attributes: Box<[AttributeRef]>,
    /// add-member-visibility (zbc 1.23): 0=public / 1=private / 2=protected.
    /// Surfaced by `FieldInfo.IsPublic` / `IsPrivate`. Default 0 (public).
    #[serde(default)]
    pub visibility: u8,
}

/// Format a function's stack-trace display name with parameter signature.
/// Returns `<name>(<t1>,<t2>,...)` (e.g. `Demo.Greeter.greet(str)`).
/// Empty signature is `<name>()`. Used by VM frame push sites so traces
/// disambiguate overloads (1.3 split-debug-symbols Phase 4).
pub fn format_frame_name(func: &Function) -> String {
    let mut out = String::with_capacity(func.name.len() + 2 + func.param_count * 4);
    out.push_str(&func.name);
    out.push('(');
    for (i, t) in func.param_types().iter().enumerate() {
        if i > 0 { out.push(','); }
        out.push_str(t);
    }
    // When SIGS lacks per-param types (older artifacts or null source), fall
    // back to "?" placeholders matching `param_count` so the shape is
    // recognizable.
    if func.param_types().is_empty() && func.param_count > 0 {
        for i in 0..func.param_count {
            if i > 0 { out.push(','); }
            out.push('?');
        }
    }
    out.push(')');
    out
}

/// Cold (rarely-accessed) slice fields on `Function`. Boxed behind an
/// `Option` on Function so functions with no debug info, no try/catch,
/// no params, and no generics carry only an 8-byte null pointer instead
/// of six `Box<[T]>` headers (96 B inline → 8 B Option<Box>).
///
/// review.md E2.P5 (2026-05-27). Mirror of CoreCLR's split between
/// `MethodDesc` (hot, 32 B base) and `MethodDescChunk` / cold side
/// tables.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FunctionCold {
    /// 1.3 split-debug-symbols: per-parameter type names for stack-trace
    /// signature decoration. Length always equals `param_count` (zbc writer
    /// pads unknowns with "?"). Empty when param_count == 0.
    #[serde(default)]
    pub param_types: Box<[String]>,
    /// Exception handler ranges. Populated only when the function body
    /// contains `try` / `catch` / `finally`.
    #[serde(default)]
    pub exception_table: Box<[ExceptionEntry]>,
    /// Source-line mapping table (run-length encoded). Populated only when
    /// the module is built with debug symbols (DBUG section / sidecar).
    #[serde(default)]
    pub line_table: Box<[LineEntry]>,
    /// Debug info: maps register IDs to source-level variable names.
    /// Populated only with debug symbols.
    #[serde(default)]
    pub local_vars: Box<[LocalVar]>,
    /// Generic type parameter names: ["T"], ["K", "V"]. Empty for non-generic functions.
    #[serde(default)]
    pub type_params: Box<[String]>,
    /// L3-G3a: constraint bundle per type parameter (aligned by index with `type_params`).
    #[serde(default)]
    pub type_param_constraints: Box<[ConstraintBundle]>,
    /// C3b add-attribute-reflection-methods: user attributes applied to this
    /// method / top-level function (from the zbc SIGS section). Each points at a
    /// synthesized factory the runtime calls for `MethodInfo.GetCustomAttributes()`.
    #[serde(default)]
    pub custom_attributes: Box<[AttributeRef]>,
    /// add-parameter-attribute-reflection (zbc 1.15): per-parameter user
    /// attributes, aligned by index with the SIGS parameter array (length ==
    /// `param_count`, including the implicit `this` slot at index 0 for instance
    /// methods, which is empty). `loader` re-indexes these by source position
    /// (excluding `this`) for `ParameterInfo.GetCustomAttributes()`.
    #[serde(default)]
    pub param_attributes: Box<[Box<[AttributeRef]>]>,
    /// add-param-metadata (unify P1-d): per-param source name (SIGS
    /// `name_str_idx`), aligned by index with the SIGS parameter array (this-slot
    /// = "this"). Backs `ParameterInfo.Name` (authoritative over DBUG guess).
    #[serde(default)]
    pub param_names: Box<[String]>,
    /// add-param-metadata (unify P1-d): per-param default value `(kind, i64, str)`
    /// (kind 0=none/1=null/2=i64/3=f64bits/4=bool/5=str). Backs
    /// `ParameterInfo.DefaultValue`. Aligned by SIGS param index.
    #[serde(default)]
    pub param_defaults: Box<[(u8, i64, String)]>,
}

/// A single function.
///
/// review.md E2.P5 (2026-05-27): six rarely-accessed slice fields moved
/// into [`FunctionCold`] behind `Option<Box>`. Reads go through accessor
/// methods that return `&[T]` (empty slice when cold is absent). Sidecar
/// mutations (`loader.rs` debug-symbol overlay) lazy-init via
/// [`Function::cold_mut`].
/// serde default for `Function::params_from` (add-param-metadata): 0xFF = no varargs.
fn default_params_from() -> u8 { 0xFF }

#[derive(Debug, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    /// Number of parameters — they occupy registers 0..param_count-1 on entry.
    pub param_count: usize,
    /// Return type tag: "void", "str", "i32", "i64", "f64", "bool".
    pub ret_type: String,
    pub exec_mode: ExecMode,
    pub blocks: Vec<BasicBlock>,
    /// True for static class methods (no implicit `this` receiver).
    /// Instance methods have `this` as reg 0 and should not be treated as
    /// static-only entries in the StdlibCallIndex.
    #[serde(default)]
    pub is_static: bool,
    /// Member visibility (add-member-visibility, unify P1-b): 0=public /
    /// 1=private / 2=protected. Populated from the SIGS entry's `visibility:u8`
    /// at module load (mirrors `is_static`), so `MethodInfo.IsPublic` can
    /// report it via reflection. Defaults to 0 (public) for synthesized funcs.
    #[serde(default)]
    pub visibility: u8,
    /// Method modifiers (add-method-modifiers, unify P1-c): bit0=virtual /
    /// bit1=abstract. Populated from the SIGS entry's `method_flags:u8` at
    /// module load (mirrors `visibility`), so `MethodInfo.IsVirtual`
    /// (authoritative) / `IsAbstract` can report it via reflection.
    /// Defaults to 0 (non-virtual) for synthesized funcs.
    #[serde(default)]
    pub method_flags: u8,
    /// Required (logical) param count (add-param-metadata, unify P1-d): from the
    /// SIGS `min_arg:u16`. `ParameterInfo.IsOptional` = (logical pos >= min_arg).
    #[serde(default)]
    pub min_arg: u16,
    /// Params-varargs logical index (add-param-metadata): SIGS `params_from:u8`,
    /// 0xFF = none. `ParameterInfo.IsParams` = (logical pos == params_from).
    #[serde(default = "default_params_from")]
    pub params_from: u8,
    /// Total number of registers used (0 = unknown; VM falls back to dynamic sizing).
    #[serde(default)]
    pub max_reg: u32,
    /// Cold side-table (param_types / exception_table / line_table /
    /// local_vars / type_params / type_param_constraints). `None` for
    /// the common case of a non-generic function with no try/catch and
    /// no debug symbols. Reads go through accessor methods on `Function`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cold: Option<Box<FunctionCold>>,
    /// review.md C2 step 0.2 (2026-05-27): per-register static type
    /// from the C# IR's `TypedReg.Type`. Indexed by register ID;
    /// length matches `max_reg`. Empty `Box<[]>` when no REGT section
    /// is present in the zbc (legacy fixtures + writer-not-yet-updated
    /// path). JIT translate.rs reads this to specialize arithmetic /
    /// comparison / logical ops on known primitives (`I64` → emit
    /// Cranelift `iadd` instead of `jit_add` helper call).
    #[serde(default, skip)]
    pub reg_types: Box<[super::ir_type::IrType]>,
    /// Precomputed block label → index mapping. Not serialized; populated after module load.
    #[serde(skip)]
    pub block_index: std::collections::HashMap<String, usize>,
    /// perf-vm-iteration: per-block pre-resolved branch targets (indices),
    /// parallel to `blocks`. Lets `Br`/`BrCond` jump by index instead of
    /// SipHashing the label string every back-edge (~25% of interp loop time).
    /// Not serialized; populated by `loader::build_block_indices`. Empty ⇒
    /// runtime falls back to `block_index` (hand-built test functions).
    #[serde(skip)]
    pub branch_targets: Vec<BranchTargets>,
    /// interp-superinstr-fusion (2026-08-01): per-block fused-tail super-instruction
    /// (e.g. `cmp`+`BrCond` → `CmpBr`), parallel to `blocks`. Recognized once at
    /// load by `superinstr::compute_fused_tails`; the interp reads
    /// `fused_tails[block_idx]` (O(1)) to run the fused step and skip a dispatch on
    /// hot loops. `None`/empty ⇒ no fusion for that block (normal execution).
    /// Not serialized (pure runtime optimization; no zbc/format impact).
    #[serde(skip)]
    pub fused_tails: Vec<Option<super::superinstr::SuperInstr>>,
    /// perf-frame-name-precompute: the stack-frame display name (`"Fn(params)"`)
    /// + source file as `Arc<str>`, precomputed once at load (like
    /// `branch_targets`). `exec_function_body` clones these O(1) per call instead
    /// of re-running `format_frame_name` (String alloc + format) + a file clone
    /// on **every** call — that was 40–60% of call-heavy interp time (measured).
    /// `None` for hand-built test functions the loader never post-processes → the
    /// interp falls back to formatting on the fly.
    #[serde(skip)]
    pub frame_meta: Option<(std::sync::Arc<str>, std::sync::Arc<str>)>,
    /// Per-function token cache (introduce-method-token, 2026-05-08).
    /// Lazy-init by `metadata::resolver::resolve_module` after module load.
    /// `OnceLock` so `Function: Sync` is preserved (single-thread today,
    /// future multi-thread ready). Not serialized — purely runtime metadata.
    #[serde(skip)]
    pub resolved: std::sync::OnceLock<super::resolver::ResolvedTokens>,
}

impl Function {
    /// Borrow the cold side-table or return a static empty slice. Accessor
    /// methods below all delegate here.
    #[inline]
    fn cold_slice<T, F: FnOnce(&FunctionCold) -> &[T]>(&self, f: F) -> &[T] {
        match self.cold.as_ref() {
            Some(c) => f(c),
            None    => &[],
        }
    }

    #[inline] pub fn param_types(&self)             -> &[String]           { self.cold_slice(|c| &c.param_types) }
    #[inline] pub fn exception_table(&self)         -> &[ExceptionEntry]   { self.cold_slice(|c| &c.exception_table) }
    #[inline] pub fn line_table(&self)              -> &[LineEntry]        { self.cold_slice(|c| &c.line_table) }
    #[inline] pub fn local_vars(&self)              -> &[LocalVar]         { self.cold_slice(|c| &c.local_vars) }
    #[inline] pub fn type_params(&self)             -> &[String]           { self.cold_slice(|c| &c.type_params) }
    #[inline] pub fn type_param_constraints(&self)  -> &[ConstraintBundle] { self.cold_slice(|c| &c.type_param_constraints) }
    /// C3b add-attribute-reflection-methods: user attributes applied to this function.
    #[inline] pub fn custom_attributes(&self)       -> &[AttributeRef]     { self.cold_slice(|c| &c.custom_attributes) }
    /// add-parameter-attribute-reflection: per-parameter attributes (SIGS-aligned,
    /// incl. `this` slot). Empty slice when the cold side-table is absent.
    #[inline] pub fn param_attributes(&self)        -> &[Box<[AttributeRef]>] { self.cold_slice(|c| &c.param_attributes) }
    #[inline] pub fn param_names(&self)             -> &[String]           { self.cold_slice(|c| &c.param_names) }
    #[inline] pub fn param_defaults(&self)          -> &[(u8, i64, String)] { self.cold_slice(|c| &c.param_defaults) }

    /// Lazy-init the cold side-table for mutation. Used by sidecar debug-
    /// symbol overlay in `metadata::loader`.
    #[inline]
    pub fn cold_mut(&mut self) -> &mut FunctionCold {
        self.cold.get_or_insert_with(|| Box::new(FunctionCold::default()))
    }

    // ── add-offline-symbolication: code-offset ↔ (block, instr) mapping ────────
    //
    // z42 IR is a block+intra-block-instruction model with no linear bytecode
    // address, but offline symbolication (and the stripped-frame stack format
    // `at <func> +0x<offset>`) needs one stable key per site. We pack the site
    // into a single u32:
    //
    //   offset(block, instr) = (block << 16) | (instr & 0xffff)
    //
    // This is **O(1)** — critical because `update_caller_line` computes it on
    // every Call/VCall (a prefix-sum linearization was measured ~5% slower on
    // dispatch-heavy loops). The key is opaque but stable and block-major
    // monotonic; the user only treats `+0x<offset>` as a token to feed
    // `z42d symbolicate`, which unpacks it back to `(block, instr)` and looks up
    // the archived `.zsym` line table. `block`/`instr` fit u16 in every real z42
    // function (a basic block never holds 2^16 instructions, nor a function 2^16
    // blocks); `debug_assert` guards the invariant. This is the SINGLE source of
    // truth — the z42-side `z42d symbolicate` mirrors the same pack/unpack.

    /// Packed code offset for a `(block, instr)` site — `(block << 16) | instr`.
    /// O(1). `block`/`instr` come from the interp/JIT loop state and are trusted
    /// in range; the u16 bound is a `debug_assert` (see the module note).
    #[inline]
    pub fn linear_offset(&self, block: u32, instr: u32) -> u32 {
        debug_assert!(block <= 0xffff && instr <= 0xffff,
            "code offset packing overflow: block={block} instr={instr}");
        (block << 16) | (instr & 0xffff)
    }

    /// Inverse of [`linear_offset`]: unpack `(block, instr)` from a code offset.
    /// Used by `z42d symbolicate` (and tests) to map a captured `+0x<offset>`
    /// back to a site, then to a `LineEntry`.
    #[inline]
    pub fn offset_to_site(&self, offset: u32) -> (u32, u32) {
        (offset >> 16, offset & 0xffff)
    }
}

/// An entry in a function's local variable table: register `reg` holds variable `name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVar {
    pub name: String,
    pub reg:  u16,
}

/// An entry in a function's source-line mapping table.
///
/// 2026-05-10 span-column-propagate (zbc 1.1): `column` carries 1-based
/// source column from `Span.Column`. Value `0` means unknown (legacy
/// hand-rolled IR or pre-1.1 zbc never reach here — reader rejects).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEntry {
    pub block:  u32,
    pub instr:  u32,
    pub line:   u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file:   Option<String>,
    #[serde(default)]
    pub column: u32,
}

/// One row in a function's exception table.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExceptionEntry {
    pub try_start:   String,
    pub try_end:     String,
    pub catch_label: String,
    pub catch_type:  Option<String>,
    #[serde(with = "typed_reg_serde")]
    pub catch_reg:   u32,
}

/// A basic block — straight-line instructions ending in exactly one terminator.
#[derive(Debug, Serialize, Deserialize)]
pub struct BasicBlock {
    pub label: String,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

/// Register index.
pub type Reg = u32;

// ── Boxed instruction payloads (slim-instruction-enum, 2026-06-11) ───────────
// Variants carrying a `String` (name-bearing, cold) keep their payload behind a
// `Box<XxxInsn>` so the `Instruction` enum stays ≤32 B (was ~120 B). Hot
// register/scalar variants remain inline. JSON wire format is unchanged: an
// internally-tagged (`tag = "op"`) newtype variant whose inner type is a struct
// merges the tag into the struct's fields, so `Call(Box<CallInsn>)` serializes
// to the same `{"op":"call", dst, func, args}` as the old struct variant.
// See docs/design/runtime/ir.md (hot/cold boxing strategy).

/// Payload for [`Instruction::Call`].
#[derive(Debug, Serialize, Deserialize)]
pub struct CallInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub func: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::ArrayNew`] (add-reflection-array-element-type).
#[derive(Debug, Serialize, Deserialize)]
pub struct ArrayNewInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub size: Reg,
    #[serde(default)] pub elem_tag: u8,
    /// Element type's FQ name (e.g. "int" / "geometry.Point"), resolved from the
    /// string pool at decode. Stored on the array's `ArrayObj` so
    /// `arr.GetType().GetElementType()` is non-erased. Empty = absent (legacy).
    #[serde(default)] pub element_type: String,
    /// add-escape-analysis-stack-alloc (zbc 1.29): escape analysis proved this
    /// array does not escape its creating frame → interp allocates it in the
    /// frame arena (GC-skipped). JIT ignores this flag (heap-allocates) in v1.
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::ArrayNewLit`] (add-reflection-array-element-type).
#[derive(Debug, Serialize, Deserialize)]
pub struct ArrayNewLitInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_vec_serde")] pub elems: Box<[Reg]>,
    #[serde(default)] pub element_type: String,
    /// add-escape-analysis-stack-alloc (zbc 1.29): non-escaping → frame arena (interp).
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::Builtin`].
#[derive(Debug, Serialize, Deserialize)]
pub struct BuiltinInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub name: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::LoadFn`].
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadFnInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub func: String,
}

/// Payload for [`Instruction::LoadFnCached`].
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadFnCachedInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub func: String,
    pub slot_id: u32,
}

/// Payload for [`Instruction::MkClos`].
#[derive(Debug, Serialize, Deserialize)]
pub struct MkClosInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub fn_name: String,
    #[serde(with = "typed_reg_vec_serde")] pub captures: Box<[Reg]>,
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::ObjNew`].
#[derive(Debug, Serialize, Deserialize)]
pub struct ObjNewInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub class_name: String,
    pub ctor_name: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
    /// Resolved generic type-arguments for this allocation (e.g. `["int"]` for
    /// `new Foo<int>()`); empty for non-generic. `Box<[String]>` (immutable IR).
    #[serde(default)] pub type_args: Box<[String]>,
    /// add-escape-analysis-stack-alloc (zbc 1.29): escape analysis proved this
    /// object does not escape AND its ctor does not leak `this` → interp allocates
    /// it in the frame arena (GC-skipped). JIT ignores this flag (heap) in v1.
    #[serde(default)] pub stack_alloc: bool,
}

/// Payload for [`Instruction::Typeof`].
///
/// `typeof(T)` reflection. `type_name` is the FQ definition name
/// (`make_type_from_name`-resolvable); `type_args` are the FQ names of the
/// instantiation type arguments (`typeof(Box<int>)` → `["int"]`; empty for
/// non-generic / open). A non-empty list marks a *constructed* generic type.
/// add-reflection-generic-type-definition (zbc 1.18).
#[derive(Debug, Serialize, Deserialize)]
pub struct TypeofInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub type_name: String,
    #[serde(default)] pub type_args: Box<[String]>,
}

/// Payload for [`Instruction::FieldGet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FieldGetInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub field_name: String,
}

/// Payload for [`Instruction::FieldSet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FieldSetInsn {
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub field_name: String,
    #[serde(with = "typed_reg_serde")] pub val: Reg,
}

/// Payload for [`Instruction::VCall`].
#[derive(Debug, Serialize, Deserialize)]
pub struct VCallInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub method: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::IsInstance`].
#[derive(Debug, Serialize, Deserialize)]
pub struct IsInstanceInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub class_name: String,
}

/// Payload for [`Instruction::AsCast`].
#[derive(Debug, Serialize, Deserialize)]
pub struct AsCastInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub class_name: String,
}

/// Payload for [`Instruction::StaticGet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct StaticGetInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub field: String,
}

/// Payload for [`Instruction::StaticSet`].
#[derive(Debug, Serialize, Deserialize)]
pub struct StaticSetInsn {
    pub field: String,
    #[serde(with = "typed_reg_serde")] pub val: Reg,
}

/// Payload for [`Instruction::CallNative`].
#[derive(Debug, Serialize, Deserialize)]
pub struct CallNativeInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    pub module: String,
    pub type_name: String,
    pub symbol: String,
    #[serde(with = "typed_reg_vec_serde")] pub args: Box<[Reg]>,
}

/// Payload for [`Instruction::StructAlloc`] (add-struct-value-semantics Phase A).
#[derive(Debug, Serialize, Deserialize)]
pub struct StructAllocInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    /// FQ value-type name — the arena records it so GC can scan blob reference
    /// leaves by the type's ref-bitmap, and boxing can recover the precise type.
    pub type_name: String,
    /// Blob size in bytes (StructLayout.size).
    pub size: u32,
}

/// Payload for [`Instruction::LoadFieldAddr`].
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadFieldAddrInsn {
    #[serde(with = "typed_reg_serde")] pub dst: Reg,
    /// Reg holding the object (must be `Value::Object(GcRef<...>)`).
    #[serde(with = "typed_reg_serde")] pub obj: Reg,
    pub field_name: String,
}

/// SSA instructions.
/// JSON wire format: {"op": "<snake_case_name>", <named fields...>}
///
/// Register fields accept both plain integers (`42`) and TypedReg objects
/// (`{"id": 42, "type": "i32"}`) during JSON deserialization for backward
/// compatibility with both old and new compiler output.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Instruction {
    // Constants
    ConstStr  { #[serde(with = "typed_reg_serde")] dst: Reg, idx: u32 },
    ConstI32  { #[serde(with = "typed_reg_serde")] dst: Reg, val: i32 },
    ConstI64  { #[serde(with = "typed_reg_serde")] dst: Reg, val: i64 },
    ConstF64  { #[serde(with = "typed_reg_serde")] dst: Reg, val: f64 },
    ConstBool { #[serde(with = "typed_reg_serde")] dst: Reg, val: bool },
    ConstChar { #[serde(with = "typed_reg_serde")] dst: Reg, val: char },
    ConstNull { #[serde(with = "typed_reg_serde")] dst: Reg },
    Copy {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Arithmetic
    Add {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Sub {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Mul {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Div {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Rem {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    // Comparison
    Eq {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Ne {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Lt {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Le {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Gt {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Ge {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    // Logical
    And {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Or {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Not {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Unary arithmetic
    Neg {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Bitwise
    BitAnd {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    BitOr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    BitXor {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    BitNot {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    Shl {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    Shr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    // String
    StrConcat {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] a: Reg,
        #[serde(with = "typed_reg_serde")] b: Reg,
    },
    ToStr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    // Spec impl-ref-out-in-runtime: Address-load instructions producing
    // Value::Ref values. Caller emits these for `ref`/`out`/`in` arguments
    // before the Call; the Ref is passed through Call's args; callee's
    // frame.get/set transparently derefs (single dispatch point).
    LoadLocalAddr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        /// Slot in the *current* frame to point at. Codegen guarantees this
        /// is a real local register (not virtual). At runtime produces
        /// `Value::Ref { kind: RefKind::Stack { frame_idx: depth-1, slot } }`.
        #[serde(with = "typed_reg_serde")] slot: Reg,
    },
    LoadElemAddr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        /// Reg holding the array (must be `Value::Array(GcRef<...>)`).
        #[serde(with = "typed_reg_serde")] arr: Reg,
        /// Reg holding the index (must be `Value::I64`).
        #[serde(with = "typed_reg_serde")] idx: Reg,
    },
    LoadFieldAddr(Box<LoadFieldAddrInsn>),
    /// 2026-05-07 add-default-generic-typeparam (D-8b-3 Phase 2): runtime
    /// resolution of `default(T)` where T is a generic type-parameter of the
    /// receiver class. Reads `frame.regs[0]` (this) → `Object → type_desc.type_args[param_index]`,
    /// looks up the resolved type via `default_value_for(tag)`, writes Value to dst.
    /// Non-Object reg 0 / OOB index → graceful-degrade to `Value::Null`.
    DefaultOf {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        param_index: u8,
    },
    /// spec fix-numeric-cast-lowering (2026-05-13): explicit numeric type
    /// conversion. Target type comes from `dst`'s static type tag; source
    /// type is resolved at runtime from `src`'s `Value` variant.
    ///
    /// Covered:
    ///   - f64 → i*/u* (saturating, Rust `as` semantics; NaN → 0)
    ///   - i64 → f32/f64 (widening)
    ///   - i64 → i8/i16/i32 (low-bits + sign extend)
    ///   - i64 → u8/u16/u32 (low-bits + zero extend)
    ///   - char ↔ i32/i64 (Unicode scalar; invalid → error)
    /// Identity casts (fromIr == toIr) are not emitted — codegen returns the
    /// source register directly.
    Convert {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
        /// Target type tag (TypeTags constants — I8/I16/.../F64/Char etc.).
        /// Source type is determined at runtime from `src`'s Value variant.
        to_tag: u8,
    },
    // Calls
    Call(Box<CallInsn>),
    Builtin(Box<BuiltinInsn>),
    /// Push a function-reference value onto a register. The runtime resolves
    /// `func` at call site (current usage: L2 no-capture lambda lifted as a
    /// module-level function). See docs/design/language/closure.md §6.
    LoadFn(Box<LoadFnInsn>),
    /// 2026-05-02 add-method-group-conversion (D1b): cached method group
    /// conversion. First execution stores `Value::FuncRef(func)` into VmContext
    /// `func_ref_slots[slot_id]`; subsequent hits read from slot. Same fully-
    /// qualified `func` shares a `slot_id` across all call sites in a module.
    LoadFnCached(Box<LoadFnCachedInsn>),
    /// Indirect call via a register holding a `FuncRef` value. See
    /// docs/design/language/closure.md §6.
    CallIndirect {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] callee: Reg,
        #[serde(with = "typed_reg_vec_serde")] args: Box<[Reg]>,
    },
    /// L3 closure tier-C: allocate an env from `captures`, build a closure
    /// value and write it to `dst`. See docs/design/language/closure.md §6.
    /// `stack_alloc=true` (impl-closure-l3-escape-stack): VM 走 frame-local
    /// arena → `Value::StackClosure`；否则 heap → `Value::Closure`。
    MkClos(Box<MkClosInsn>),
    // Arrays
    /// Allocate a zero-initialised array of `size` elements. Each slot is
    /// filled with the per-type default value derived from `elem_tag`
    /// (zbc `TypeTags::*` byte): `Value::I64(0)` for numeric tags,
    /// `Value::Bool(false)` for bool, `Value::Char('\0')` for char,
    /// `Value::F64(0.0)` for float/double, `Value::Null` for ref/string/unknown.
    /// fix-array-default-init, 2026-05-18.
    /// add-reflection-array-element-type (zbc 1.16): boxed because the
    /// `element_type` String would blow the 32 B slim-instruction invariant.
    ArrayNew(Box<ArrayNewInsn>),
    /// Allocate an array from a literal list of element registers.
    ArrayNewLit(Box<ArrayNewLitInsn>),
    /// Load element at `idx` from array `arr` into `dst`. Panics on out-of-bounds.
    ArrayGet {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] arr: Reg,
        #[serde(with = "typed_reg_serde")] idx: Reg,
    },
    /// Store `val` into array `arr` at `idx`. Panics on out-of-bounds.
    ArraySet {
        #[serde(with = "typed_reg_serde")] arr: Reg,
        #[serde(with = "typed_reg_serde")] idx: Reg,
        #[serde(with = "typed_reg_serde")] val: Reg,
    },
    /// Load the length of array `arr` as i32 into `dst`.
    ArrayLen {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] arr: Reg,
    },
    // Objects
    /// Allocate a new object of `class_name`, calling overload-resolved
    /// ctor `ctor_name` (FQ, 含 `$N` suffix 如有) with `args`. VM 不再做
    /// `${class}.${simple}` 名字推断 — 直查 `func_index[ctor_name]`.
    ObjNew(Box<ObjNewInsn>),
    /// `typeof(T)` → a reflection `Std.Type` into `dst`, carrying structured
    /// generic instantiation args. add-reflection-generic-type-definition.
    Typeof(Box<TypeofInsn>),
    /// Load field `field_name` of object `obj` into `dst`.
    FieldGet(Box<FieldGetInsn>),
    /// Store `val` into field `field_name` of object `obj`.
    FieldSet(Box<FieldSetInsn>),
    /// Virtual dispatch: invoke `method` on runtime class of `obj`, walking base classes.
    VCall(Box<VCallInsn>),
    /// `expr is ClassName` — dst = true if obj's runtime type is class_name or a subclass.
    IsInstance(Box<IsInstanceInsn>),
    /// `expr as ClassName` — dst = obj if it is an instance of class_name (or subclass), else null.
    AsCast(Box<AsCastInsn>),
    /// Load the module-level static field `field` into `dst`.
    StaticGet(Box<StaticGetInsn>),
    /// Store `val` into the module-level static field `field`.
    StaticSet(Box<StaticSetInsn>),

    // Native interop (C1 scaffold; semantics by C2/C4/C5)
    /// Direct native symbol call. Resolved at load time; runtime behaviour
    /// arrives in spec C2.
    CallNative(Box<CallNativeInsn>),
    /// Native-type vtable indirect call. `vtable_slot` is filled by the C5
    /// source generator at compile time so no name lookup happens at runtime.
    CallNativeVtable {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] recv: Reg,
        vtable_slot: u16,
        #[serde(with = "typed_reg_vec_serde")] args: Box<[Reg]>,
    },
    /// Pin a String/Array buffer for FFI borrow. Pinned-view layout and
    /// lifetime semantics land in spec C4.
    PinPtr {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
    },
    /// Release a pinned view created by `PinPtr`.
    UnpinPtr {
        #[serde(with = "typed_reg_serde")] pinned: Reg,
    },

    // ── blob value types (add-struct-value-semantics Phase A) ────────────────
    /// Allocate a `size`-byte blob in the per-context struct arena
    /// (zero-initialized); `dst` = `Value::StructRef` handle.
    StructAlloc(Box<StructAllocInsn>),
    /// Copy a struct blob (`size` bytes) from `src` to `dst`. Pure-primitive
    /// blobs memcpy; blobs with reference leaves clone per the type's ref-bitmap.
    StructCopy {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] src: Reg,
        size: u32,
    },
    /// Read the primitive leaf at `byte_off` of struct blob `base` into `dst`.
    StructFieldGetPrim {
        #[serde(with = "typed_reg_serde")] dst: Reg,
        #[serde(with = "typed_reg_serde")] base: Reg,
        byte_off: u32,
        kind: u8,
    },
    /// Write primitive `val` into struct blob `base` at `byte_off` (in-place lvalue).
    StructFieldSetPrim {
        #[serde(with = "typed_reg_serde")] base: Reg,
        byte_off: u32,
        kind: u8,
        #[serde(with = "typed_reg_serde")] val: Reg,
    },
}

/// Block terminator.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Terminator {
    Ret {
        #[serde(with = "typed_reg_opt_serde")]
        reg: Option<Reg>,
    },
    Br { label: String },
    BrCond {
        #[serde(with = "typed_reg_serde")] cond: Reg,
        true_label: String,
        false_label: String,
    },
    /// Throw the value in `reg` as an exception.
    Throw {
        #[serde(with = "typed_reg_serde")] reg: Reg,
    },
}

/// perf-vm-iteration: a block terminator's branch target(s) pre-resolved to
/// block **indices** at load time, so `Br`/`BrCond` become direct integer jumps
/// instead of a per-branch `HashMap<String,usize>` SipHash lookup on the label.
/// Profiling showed ~25% of interp loop time was SipHashing block labels on
/// every back-edge. Populated by `loader::build_block_indices`; runtime falls
/// back to the label `HashMap` when absent (e.g. hand-built test functions).
#[derive(Debug, Clone, Copy)]
pub enum BranchTargets {
    /// Ret / Throw — not a branch (this block's terminator never indexes here).
    NoBranch,
    /// `Br` → resolved target block index.
    Br(usize),
    /// `BrCond` → (true_label idx, false_label idx).
    BrCond(usize, usize),
}

#[cfg(test)]
#[path = "bytecode_tests.rs"]
mod bytecode_tests;
