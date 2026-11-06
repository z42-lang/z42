use super::*;

/// Class parameters paired with the exact Type values in the constructed Type.
pub(super) struct MemberSubst {
    definition: Arc<TypeDesc>,
    arguments: Vec<Value>,
}

impl MemberSubst {
    pub(super) fn applies_to(&self, qualified: &str) -> bool {
        TypeDesc::derive_simple_method_name(&self.definition.name, qualified) != qualified
    }

    fn argument(&self, tag: &str, shadowed: &[String]) -> Option<Value> {
        if shadowed.iter().any(|name| name == tag) {
            return None;
        }
        self.definition.type_params().iter().position(|name| name == tag)
            .map(|i| self.arguments[i])
    }
}

pub(super) fn member_subst(ty: &Value) -> Option<MemberSubst> {
    let td = type_handle_of(ty)?;
    let params = td.type_params();
    if params.is_empty() {
        return None;
    }
    let Value::Array(arguments) = read_type_slot_of(ty, "__typeArgs") else {
        return None;
    };
    let arguments: Vec<Value> = arguments.borrow().iter_boxed().collect();
    (arguments.len() == params.len()).then_some(MemberSubst { definition: td, arguments })
}

/// Project a declaration's Type expression without resolving substituted names again.
pub(super) fn projected_member_type(
    ctx: &VmContext, tag: &str, subst: &MemberSubst, shadowed: &[String],
) -> Option<Value> {
    let tag = tag.trim();
    if let Some(argument) = subst.argument(tag, shadowed) {
        return Some(argument);
    }
    if let Some(elem) = tag.strip_suffix("[]") {
        let element = projected_member_type(ctx, elem, subst, shadowed)?;
        let name = type_value_arg_name(&element).unwrap_or_else(|| elem.to_string());
        return Some(make_array_type(ctx, &name, element));
    }
    let (definition, arguments) = tag.strip_suffix('>')?.split_once('<')?;
    let args = split_generic_args(arguments);
    // Resolving a sibling or the generic definition may run a lazy initializer.
    let mut roots = ProjectionRoots { heap: ctx.heap(), handles: Vec::new() };
    let projections: Vec<_> = args.iter().map(|arg| {
        projected_member_type(ctx, arg, subst, shadowed).map(|value| roots.pin(value))
    }).collect();
    if projections.iter().all(Option::is_none) {
        return None;
    }
    let arguments = projections.into_iter().zip(args).map(|(projection, arg)| {
        projection.unwrap_or_else(|| roots.pin(make_type_from_name(ctx, &arg)))
    }).collect();
    Some(make_constructed_type_from_values(ctx, definition.trim(), arguments))
}

/// Retain the name-based fallback for inherited metadata without its declaring
/// class's exact argument projection.
pub(super) fn inherited_member_type(
    ctx: &VmContext, tag: &str, subst: &MemberSubst, shadowed: &[String],
) -> Value {
    let (params, names): (Vec<_>, Vec<_>) = subst.definition.type_params().iter().zip(&subst.arguments)
        .filter(|(param, _)| !shadowed.contains(param))
        .filter_map(|(param, arg)| Some((param.clone(), type_value_arg_name(arg)?)))
        .unzip();
    let tag = crate::metadata::index::type_names::substitute(tag, &params, &names);
    make_type_from_name(ctx, &tag)
}

pub(super) fn signature_type(
    ctx: &VmContext, tag: &str, qualified: &str, subst: Option<&MemberSubst>, shadowed: &[String],
) -> Value {
    if let Some(subst) = subst {
        if subst.applies_to(qualified) {
            if let Some(projected) = projected_member_type(ctx, tag, subst, shadowed) {
                return projected;
            }
            // Keep the canonical spelling used by name-based substitution when
            // this signature does not reference a constructed type argument.
            return make_type_from_name(ctx, &crate::metadata::index::type_names::canonical(tag));
        } else {
            return inherited_member_type(ctx, tag, subst, shadowed);
        }
    }
    make_type_from_name(ctx, tag)
}

struct ProjectionRoots<'a> {
    heap: &'a crate::gc::Heap,
    handles: Vec<crate::gc::RootHandle>,
}

impl ProjectionRoots<'_> {
    fn pin(&mut self, value: Value) -> Value {
        self.handles.push(self.heap.pin_root(value));
        value
    }
}

impl Drop for ProjectionRoots<'_> {
    fn drop(&mut self) {
        for handle in self.handles.drain(..) {
            self.heap.unpin_root(handle);
        }
    }
}
