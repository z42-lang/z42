//! Type object allocation, weak publication and initialization boundaries.
use super::*;

pub(super) fn finish_type_initialization(ctx: &VmContext) {
    if !ctx.type_load_drain_is_noop(&[]) {
        ctx.drain_pending_type_loads();
    }
}

/// Allocate the managed Type layout and attach its stable native identity.
pub(super) fn build_type(ctx: &VmContext, simple: &str, full: &str, native: NativeData) -> Value {
    build_type_ex(ctx, simple, full, native, TypeProjection::Plain)
}

pub(super) enum TypeProjection<'a> {
    Plain,
    Array {
        element_name: &'a str,
        element: Value,
    },
    Generic(Vec<Value>),
}

pub(super) fn build_type_ex(
    ctx: &VmContext,
    simple: &str,
    full: &str,
    native: NativeData,
    projection: TypeProjection<'_>,
) -> Value {
    let _initialization_roots = if !ctx.type_load_drain_is_noop(&[]) {
        let mut roots = ProjectionRoots::new(ctx);
        match &projection {
            TypeProjection::Array { element, .. } => {
                roots.pin(*element);
            }
            TypeProjection::Generic(arguments) => {
                for argument in arguments {
                    roots.pin(*argument);
                }
            }
            TypeProjection::Plain => {}
        }
        ctx.drain_pending_type_loads();
        Some(roots)
    } else {
        None
    };
    let cache_key = match &native {
        NativeData::TypeHandle(handle) => Some(handle.key()),
        NativeData::TypeShape(shape) => Some(shape.key.clone()),
        _ => None,
    };
    // Explicit Type arguments keep their object identity through member projection.
    let arguments = match &projection {
        TypeProjection::Array {
            element: Value::Object(element),
            ..
        } => vec![*element],
        TypeProjection::Generic(arguments) => arguments
            .iter()
            .filter_map(|arg| {
                if let Value::Object(object) = arg {
                    Some(*object)
                } else {
                    None
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    if let Some(key) = &cache_key {
        if let Some(value) = ctx
            .core()
            .runtime()
            .reflection_types
            .get(ctx.heap(), key, &arguments)
        {
            return value;
        }
    }
    let related_type = match &projection {
        TypeProjection::Array { element, .. } => Some(element),
        TypeProjection::Generic(arguments) => arguments.first(),
        TypeProjection::Plain => None,
    };
    let type_td = match related_type {
        // Reuse the Type descriptor without a lazy lookup while fresh
        // reflection objects are held in native locals.
        Some(Value::Object(rc)) if rc.type_desc().name == well_known_names::STD_TYPE => {
            Some(rc.type_desc_arc().clone())
        }
        _ => ctx.try_lookup_type(well_known_names::STD_TYPE),
    };
    let value = match type_td {
        Some(type_td) => {
            let mut slots = vec![Value::Null; type_td.fields.len()];
            if let Some(&i) = type_td.field_index.get("IsArray") {
                slots[i] = Value::Bool(matches!(projection, TypeProjection::Array { .. }));
            }
            if let Some(&i) = type_td.field_index.get("__elementName") {
                let name = match &projection {
                    TypeProjection::Array { element_name, .. } => *element_name,
                    _ => "",
                };
                slots[i] = Value::Str(name.into());
            }
            match projection {
                TypeProjection::Array { element, .. } => {
                    if let Some(&i) = type_td.field_index.get("__elementType") {
                        slots[i] = element;
                    }
                }
                TypeProjection::Generic(arguments) => {
                    if let Some(&i) = type_td.field_index.get("__typeArgs") {
                        slots[i] = ctx.heap().alloc_array(arguments);
                    }
                }
                TypeProjection::Plain => {}
            }
            // align-type-memberinfo-hierarchy: `Name` is inherited from
            // `Std.Reflection.MemberInfo` (Type's base) — populate that slot so
            // `typeof(C).Name` / `(MemberInfo)typeof(C)).Name` resolve via the
            // shared base field (same as FieldInfo/MethodInfo). `__name` retained
            // for low-level golden / z42.harness direct reads.
            if let Some(&i) = type_td.field_index.get("Name") {
                slots[i] = Value::Str(simple.to_string().into());
            }
            if let Some(&i) = type_td.field_index.get("__name") {
                slots[i] = Value::Str(simple.to_string().into());
            }
            if let Some(&i) = type_td.field_index.get("__fullName") {
                slots[i] = Value::Str(full.to_string().into());
            }
            ctx.heap().alloc_object(type_td, slots, native)
        }
        // z42.core not loaded — return a bare null Type (degraded). Reflection
        // is meaningless without z42.core, so this path is effectively dead.
        None => Value::Null,
    };
    match cache_key {
        Some(key) => ctx
            .core()
            .runtime()
            .reflection_types
            .publish(ctx.heap(), key, arguments, value),
        _ => value,
    }
}
