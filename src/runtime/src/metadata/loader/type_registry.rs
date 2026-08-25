use super::*;

// ── TypeDesc registry ─────────────────────────────────────────────────────────

/// Pre-build a `TypeDesc` for every class in `module.classes` and store the
/// results in `module.type_registry` (by-name HashMap) **and**
/// `module.type_registry_vec` (by-`TypeId` Vec, Phase 3 S1, 2026-05-09).
///
/// Algorithm (CoreCLR-inspired):
///   1. Topological sort: each class is processed after its base class.
///   2. Field slots: base fields first (already in base TypeDesc), then derived.
///   3. vtable: start with base vtable, override entries where derived defines
///      the same method name, append new methods at the end.
///   4. Both views populated: by-name HashMap and by-TypeId Vec[id] = Arc.
pub fn build_type_registry(module: &mut Module) {
    let order = topo_sort_classes(module);
    let mut registry: FxHashMap<String, Arc<TypeDesc>> = FxHashMap::default();
    let mut registry_vec: Vec<Arc<TypeDesc>> = Vec::with_capacity(order.len());
    // introduce-method-token 2026-05-08: assign TypeId in topo order so that
    // each TypeDesc has a stable per-module id. VCallIC / FieldIC compare
    // receiver TypeId via single u32 equality (no name hash).
    // Phase 3 S1: TypeId.0 is also the index in `registry_vec` (invariant).
    let mut next_type_id: u32 = 0;

    for class_name in &order {
        let desc = match module.classes.iter().find(|c| &c.name == class_name) {
            Some(d) => d,
            None    => continue,
        };

        // ── Own fields (this class's own declarations) ────────────────────
        // fix-cross-pkg-subclass-fields (2026-05-14): preserved separately
        // so the lazy-loader fixup pass can rebuild merged `fields` once
        // the cross-zpkg base resolves.
        let own_fields: Vec<FieldSlot> = desc.fields.iter().map(|f| FieldSlot {
            name: f.name.clone().into(),
            type_tag: f.type_tag.clone().into(),
            visibility: f.visibility,
        }).collect();

        // ── Own methods (this class's own declarations) ───────────────────
        // review.md E5.5 (2026-05-27): store qualified func names only;
        // the simple vtable-slot name is re-derived at merge time via
        // `TypeDesc::derive_simple_method_name`.
        let mut own_methods: Vec<Box<str>> = Vec::new();
        let prefix = format!("{}.", class_name);
        for func in &module.functions {
            if !func.name.starts_with(&prefix) { continue; }
            let method = &func.name[prefix.len()..];
            // Skip constructors (same name as class simple name) and __static_init__
            let simple_name = class_name.split('.').next_back().unwrap_or(class_name.as_str());
            if method == simple_name || method.starts_with("__") { continue; }
            own_methods.push(func.name.clone().into_boxed_str());
        }

        // ── Initial merged view: inherit from local-registry base if present.
        // Cross-zpkg base classes contribute nothing here — that's fixed up
        // later by `try_fixup_inheritance` once the dep is loaded.
        let (fields, field_index, vtable, vtable_index) =
            merge_with_base(&own_fields, &own_methods, class_name, desc.base_class.as_deref(), &registry);

        let type_id = crate::metadata::tokens::TypeId(next_type_id);
        next_type_id += 1;
        // add-field-attribute-reflection: index per-field attr refs (instance +
        // static fields with attributes) by field name for reflection.
        let field_attributes: Box<[(Box<str>, Box<[crate::metadata::bytecode::AttributeRef]>)]> =
            desc.fields.iter()
                .chain(desc.static_fields.iter())
                .filter(|f| !f.attributes.is_empty())
                .map(|f| (f.name.as_str().into(), f.attributes.clone()))
                .collect();
        let cold_inner = crate::metadata::types::TypeDescCold {
            own_fields:             own_fields.into(),
            own_methods:            own_methods.into(),
            type_params:            desc.type_params.clone(),
            type_args:              vec![].into(),
            type_param_constraints: desc.type_param_constraints.clone(),
            // C3 add-attribute-reflection: carry the class's user attributes.
            custom_attributes:      desc.attributes.clone(),
            // add-reflection-static-fields: carry the class's static fields.
            static_fields:          desc.static_fields.clone(),
            // add-field-attribute-reflection: per-field attr refs by name.
            field_attributes,
            // add-reflection-get-interfaces: carry the class's declared interfaces.
            interfaces:             desc.interfaces.iter().map(|s| s.as_str().into()).collect(),
            // add-enum-type-metadata: carry the enum's (name, value) members.
            enum_members:           desc.enum_members.clone(),
            // add-interface-member-reflection: carry the interface's method sigs.
            iface_methods:          desc.iface_methods.clone(),
            // add-struct-value-semantics: carry the value-struct byte+ref layout
            // (from the zbc TYPE-section struct block). `map` → shared `Arc`.
            struct_layout:          desc.struct_layout.as_ref().map(|l| {
                std::sync::Arc::new(crate::metadata::types::StructTypeLayout {
                    size:        l.size as usize,
                    ref_offsets: l.ref_offsets.clone(),
                    ref_kinds:   l.ref_kinds.clone(),
                })
            }),
            // add-struct-heap-inline (P3b): the class's composed inline-struct layout
            // (zbc 1.32 inline block). `map` → shared `Arc`, same as struct_layout.
            inline_layout:          desc.inline_layout.as_ref().map(|l| {
                std::sync::Arc::new(crate::metadata::types::StructTypeLayout {
                    size:        l.size as usize,
                    ref_offsets: l.ref_offsets.clone(),
                    ref_kinds:   l.ref_kinds.clone(),
                })
            }),
            // unify-object-byte-layout (PR-1): carry the full object field layout
            // (zbc 1.34 object block) as-is — dormant metadata, not consumed yet.
            // Empty layouts (0-field classes) correlate with empty own_fields, so they
            // drop naturally with the cold-emptiness check below (not added to it).
            object_layout:          desc.object_layout.as_ref().map(|l| std::sync::Arc::new(l.clone())),
            // unify-object-byte-layout (PR-2, task 2.0): compose the own-only object
            // layout with the base's composed layout (`base.composed ++ own`). Topo
            // order means a local base is already in `registry`; a cross-zpkg base is
            // unresolved here (→ own-only, base_shift 0) and recomposed by
            // `try_fixup_inheritance` once it resolves, mirroring `fields`. Dormant.
            composed_object_layout: match desc.object_layout.as_ref() {
                Some(own) => {
                    let base_composed = desc.base_class.as_deref()
                        .and_then(|b| registry.get(b))
                        .and_then(|b| b.composed_object_layout());
                    Some(std::sync::Arc::new(crate::metadata::types::compose_object_layout(
                        base_composed.as_deref(), own, &fields,
                    )))
                }
                // unify-object-byte-layout (PR-2): a normal reference class with fields
                // but no zbc object block (module predating zbc 1.34, or synthetic) —
                // synthesize a self-consistent byte layout from the merged fields so
                // `alloc_object` / `field_value` have a layout to consume.
                None if !fields.is_empty()
                        && (desc.class_flags & (4 | 16 | 32 | 64)) == 0 => {
                    Some(std::sync::Arc::new(
                        crate::metadata::types::synthesize_object_layout(&fields),
                    ))
                }
                None => None,
            },
        };
        let cold = if cold_inner.own_fields.is_empty()
            && cold_inner.own_methods.is_empty()
            && cold_inner.type_params.is_empty()
            && cold_inner.type_param_constraints.is_empty()
            && cold_inner.custom_attributes.is_empty()
            && cold_inner.static_fields.is_empty()
            && cold_inner.field_attributes.is_empty()
            && cold_inner.interfaces.is_empty()
            && cold_inner.enum_members.is_empty()
            && cold_inner.iface_methods.is_empty()
            // add-struct-heap-inline (P3b): keep cold if it carries an inline layout
            // (an inline-field class always has own_fields too, but guard explicitly).
            && cold_inner.inline_layout.is_none()
            // unify-object-byte-layout (PR-2): keep cold if it carries a composed
            // object layout — a derived class with 0 own fields still needs the
            // inherited layout (base region) for byte-storage field access.
            && cold_inner.composed_object_layout.is_none()
        {
            None
        } else {
            Some(Box::new(cold_inner))
        };
        let arc = Arc::new(TypeDesc {
            name: class_name.clone(),
            base_name: desc.base_class.clone(),
            fields,
            field_index,
            vtable,
            vtable_index,
            // add-reflection-type-flags (zbc 1.12): carry the class-shape flags.
            class_flags: desc.class_flags,
            // complete-class-access-control: carry the class visibility byte.
            visibility: desc.visibility,
            cold,
            id: type_id,
        });
        debug_assert_eq!(
            registry_vec.len() as u32, type_id.0,
            "type_registry_vec invariant: index == TypeId.0"
        );
        registry_vec.push(arc.clone());
        registry.insert(class_name.clone(), arc);
    }

    module.type_registry = registry;
    module.type_registry_vec = registry_vec;
}

// ── fix-cross-pkg-subclass-fields (2026-05-14) ─────────────────────────────
//
// Two-phase type loading: `build_type_registry` runs per-module and only
// resolves base-class inheritance against the local module's registry.
// Cross-zpkg subclasses (subclass in zpkg B, base in zpkg A) get an empty
// inherited slice. `try_fixup_inheritance` runs at lazy-load merge time
// from `LazyLoader::load_zpkg_file` to fill them in, using the global
// type_registry that now contains both A's and B's types.

/// Merge inherited fields/vtable from `base_class_name` (looked up in
/// `registry`) with `own_fields` / `own_methods`. Returns the four fields
/// stored on `TypeDesc`: `(fields, field_index, vtable, vtable_index)`.
///
/// If `base_class_name` is `None`, `own_*` becomes the entire merged view.
/// If `base_class_name` is `Some(b)` but `b` isn't in `registry`, the merge
/// degrades to "own only" (cross-zpkg base unresolved — fixup later).
///
/// review.md E5.5 (2026-05-27): `own_methods` carries qualified func names
/// only; the vtable slot key (simple name) is derived per entry via
/// `TypeDesc::derive_simple_method_name(class_name, fq)`.
fn merge_with_base(
    own_fields:  &[FieldSlot],
    own_methods: &[Box<str>],
    class_name:  &str,
    base_class_name: Option<&str>,
    registry:    &FxHashMap<String, Arc<TypeDesc>>,
) -> (Vec<FieldSlot>, NameIndex, Vec<(String, String)>, NameIndex) {
    let (mut fields, mut vtable, mut vtable_index) = match base_class_name.and_then(|b| registry.get(b)) {
        Some(base) => (base.fields.clone(), base.vtable.clone(), base.vtable_index.clone()),
        None       => (Vec::new(), Vec::new(), NameIndex::new()),
    };

    // Append own fields skipping name collisions (subclass can't shadow).
    for f in own_fields {
        if !fields.iter().any(|s| s.name == f.name) {
            fields.push(f.clone());
        }
    }
    let field_index: NameIndex = fields.iter().enumerate()
        .map(|(i, f)| (f.name.to_string(), i))
        .collect();

    // Apply own methods: override if base method same simple name, else append.
    for fq_func_name in own_methods {
        let simple_name = TypeDesc::derive_simple_method_name(class_name, fq_func_name);
        if let Some(&slot) = vtable_index.get(simple_name) {
            vtable[slot] = (simple_name.to_string(), fq_func_name.to_string());
        } else {
            let slot = vtable.len();
            vtable_index.insert(simple_name.to_string(), slot);
            vtable.push((simple_name.to_string(), fq_func_name.to_string()));
        }
    }

    (fields, field_index, vtable, vtable_index)
}

/// Walk the global type `registry` and, for any TypeDesc whose base class
/// has become resolvable since its last build, rebuild `fields` /
/// `field_index` / `vtable` / `vtable_index` from the global registry's
/// view of the base.
///
/// Returns the number of types newly fixed up — caller loops until this
/// returns 0 (fixed-point), so multi-level deferred chains converge.
///
/// **Mutation strategy**: types in the global registry are `Arc<TypeDesc>`,
/// but at lazy-load time (well before any instance is created from these
/// types) the registry is the sole strong-Arc holder. We use `Arc::get_mut`
/// to obtain `&mut TypeDesc` and mutate in place. If `get_mut` ever returns
/// `None` (an instance was created before all deps loaded — out-of-order
/// usage that the loader should prevent), we skip the entry and log a
/// warning rather than panic; the entry's `fields` stays as it was.
///
/// **Idempotent**: re-running with the same registry produces the same
/// merged layout; types already correctly fixed up are detected via the
/// `needs_fixup` predicate and skipped.
///
/// **Why one `&mut HashMap` argument (not separate targets + global)**:
/// a "snapshot" clone of the registry would `Arc::clone` every TypeDesc,
/// bumping the strong-count to 2 and breaking `Arc::get_mut` on the
/// mutation side. Instead, we do all reads against the same `registry`
/// in an immutable preprocessing pass, materialise the new layouts into
/// owned `Vec` / `HashMap`s (no shared Arc data), then run a separate
/// mutation pass that only borrows the registry mutably.
pub fn try_fixup_inheritance(
    registry: &mut FxHashMap<String, Arc<TypeDesc>>,
) -> usize {
    // ── Phase 1: immutable scan — compute new layouts without mutating anything.
    type MergedLayout = (Vec<FieldSlot>, NameIndex, Vec<(String, String)>, NameIndex);
    // unify-object-byte-layout (PR-2): recompose the object byte layout in lockstep
    // with `fields` — a cross-zpkg base contributed nothing at build time (own-only,
    // base_shift 0); once resolvable, recompose `base.composed ++ own`. `None` when
    // this class carries no own object layout (e.g. 0-own-field derived not emitting a
    // block). Convergence rides on the same `needs_fixup` gate as `fields`.
    let mut planned: Vec<(String, MergedLayout, Option<Arc<crate::metadata::types::ObjectLayout>>)> = Vec::new();
    for (name, td) in registry.iter() {
        if !needs_fixup(td, registry) {
            continue;
        }
        let layout = merge_with_base(
            td.own_fields(),
            td.own_methods(),
            &td.name,
            td.base_name.as_deref(),
            registry,
        );
        let composed = match td.cold.as_ref().and_then(|c| c.object_layout.as_ref()) {
            Some(own) => {
                let base_composed = td.base_name.as_deref()
                    .and_then(|b| registry.get(b))
                    .and_then(|b| b.composed_object_layout());
                // `layout.0` = freshly merged fields (base ++ own) for the access table.
                Some(Arc::new(crate::metadata::types::compose_object_layout(
                    base_composed.as_deref(), own, &layout.0,
                )))
            }
            // No zbc object block — synthesize over the full merged fields (mirrors the
            // build-time fallback), so a cross-zpkg normal class still gets a layout.
            None if !layout.0.is_empty() && (td.class_flags & (4 | 16 | 32 | 64)) == 0 => {
                Some(Arc::new(crate::metadata::types::synthesize_object_layout(&layout.0)))
            }
            None => None,
        };
        planned.push((name.clone(), layout, composed));
    }

    // ── Phase 2: apply mutations.
    let mut newly_fixed = 0;
    for (name, (new_fields, new_field_index, new_vtable, new_vtable_index), new_composed) in planned {
        let arc = match registry.get_mut(&name) {
            Some(arc) => arc,
            None      => continue, // unreachable in normal use
        };
        match Arc::get_mut(arc) {
            Some(td) => {
                td.fields       = new_fields;
                td.field_index  = new_field_index;
                td.vtable       = new_vtable;
                td.vtable_index = new_vtable_index;
                // unify-object-byte-layout (PR-2): install the recomposed layout.
                // `td.cold` is `Some` whenever `new_composed` is (both derive from the
                // own object layout living on cold); guard defensively regardless.
                if let (Some(cold), Some(composed)) = (td.cold.as_mut(), new_composed) {
                    cold.composed_object_layout = Some(composed);
                }
                newly_fixed += 1;
            }
            None => {
                tracing::warn!(
                    "try_fixup_inheritance: TypeDesc `{}` has additional Arc holders \
                     before fixup completed; cross-zpkg fields may be silently wrong",
                    name
                );
            }
        }
    }
    newly_fixed
}

/// Names of types still reporting `needs_fixup` — used only to make the
/// loader's non-convergence safety-cap error message actionable.
pub fn unconverged_type_names(registry: &FxHashMap<String, Arc<TypeDesc>>) -> Vec<String> {
    registry.iter()
        .filter(|(_, td)| needs_fixup(td, registry))
        .map(|(name, _)| name.clone())
        .collect()
}

/// True if this TypeDesc's currently-merged view is missing inherited
/// entries that have since become resolvable in `registry`. We compare
/// `fields.len()` against `own_fields.len() + base.fields.len()`; a
/// mismatch means a fixup is needed.
///
/// `own_methods` is allowed to contain multiple entries with the same
/// `simple_name` (arity-overloaded methods like `Foo$1` / `Foo$2` both
/// map to the same vtable slot `Foo`); count *distinct* simple names
/// for the vtable size projection, mirroring [`merge_with_base`].
fn needs_fixup(td: &TypeDesc, registry: &FxHashMap<String, Arc<TypeDesc>>) -> bool {
    let Some(base_name) = td.base_name.as_deref() else { return false; };
    let Some(base) = registry.get(base_name) else { return false; }; // base still unresolvable
    // Count *distinct* own field names not already in base — mirroring
    // `merge_with_base`, which pushes each own field only if its name isn't
    // already present (dedup against the growing layout). Counting with
    // multiplicity here would permanently disagree with the merged layout for
    // a class carrying duplicate field names (e.g. a copy-pasted declaration),
    // making `needs_fixup` true forever → the caller's fixed-point loop spins.
    let mut seen_f: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let expected_field_count = base.fields.len()
        + td.own_fields().iter()
            .filter(|f| !base.fields.iter().any(|b| b.name == f.name))
            .filter(|f| seen_f.insert(f.name.as_ref()))
            .count();
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let own_unique_methods = td.own_methods().iter()
        .map(|fq| TypeDesc::derive_simple_method_name(&td.name, fq))
        .filter(|simple| !base.vtable_index.contains_key(*simple))
        .filter(|simple| seen.insert(*simple))
        .count();
    let expected_vtable_count = base.vtable.len() + own_unique_methods;
    td.fields.len() != expected_field_count || td.vtable.len() != expected_vtable_count
}
