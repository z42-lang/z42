use super::*;

/// Class parameters paired with the exact Type values in the constructed Type.
pub(super) struct MemberSubst {
    pub(super) definition: Arc<TypeDesc>,
    arguments: Vec<Value>,
}

impl MemberSubst {
    pub(super) fn applies_to(&self, qualified: &str) -> bool {
        TypeDesc::derive_simple_method_name(&self.definition.name, qualified) != qualified
            || qualified.strip_prefix(self.definition.name.as_str()).is_some_and(|suffix| {
                suffix.starts_with('<') && suffix.contains(">.")
            })
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

pub(super) fn projected_base_type(
    ctx: &VmContext, definition: &TypeDesc, subst: &MemberSubst,
) -> Option<Value> {
    let template = definition.custom_attributes().iter().find(|a| a.type_name == "$ReflectionBase")?;
    projected_member_type(ctx, &template.factory_func, subst, &[])
}

/// Member signatures must use the arguments of the class that declared them.
pub(super) struct MemberScopes<'a> {
    leaf: Option<MemberSubst>,
    inherited: Vec<MemberSubst>,
    roots: ProjectionRoots<'a>,
}

impl<'a> MemberScopes<'a> {
    pub(super) fn new(ctx: &'a VmContext, ty: &Value, definition: &Arc<TypeDesc>) -> Self {
        let mut scopes = Self {
            leaf: member_subst(ty),
            inherited: Vec::new(),
            roots: ProjectionRoots::new(ctx),
        };
        let mut seen = HashSet::new();
        let mut base = base_declaration(ctx, definition, scopes.leaf.as_ref(), &mut scopes.roots);
        while let Some((td, subst)) = base {
            if Arc::ptr_eq(&td, definition) || !seen.insert(Arc::as_ptr(&td)) {
                break;
            }
            base = base_declaration(ctx, &td, subst.as_ref(), &mut scopes.roots);
            scopes.inherited.extend(subst);
        }
        scopes
    }

    pub(super) fn leaf(&self) -> Option<&MemberSubst> {
        self.leaf.as_ref()
    }

    pub(super) fn iter(&self) -> impl DoubleEndedIterator<Item = &MemberSubst> {
        self.leaf.iter().chain(&self.inherited)
    }

    pub(super) fn for_method(&self, qualified: &str) -> Option<&MemberSubst> {
        self.iter().find(|scope| scope.applies_to(qualified))
    }

    pub(super) fn for_class(&self, td: &Arc<TypeDesc>) -> Option<&MemberSubst> {
        self.iter().find(|scope| {
            Arc::ptr_eq(td, &scope.definition)
                || TypeDesc::erased_definition_name(&td.name) == Some(scope.definition.name.as_str())
        })
    }

    pub(super) fn has_inherited(&self) -> bool {
        !self.inherited.is_empty()
    }
}

fn base_declaration(
    ctx: &VmContext, td: &TypeDesc, subst: Option<&MemberSubst>, roots: &mut ProjectionRoots<'_>,
) -> Option<(Arc<TypeDesc>, Option<MemberSubst>)> {
    let name = td.base_name.as_deref()?;
    if name == STD_OBJECT {
        return None;
    }
    let projected = subst.and_then(|scope| projected_base_type(ctx, td, scope));
    if let Some(base) = projected.or_else(|| name.contains('<').then(|| make_type_from_name(ctx, name))) {
        let definition = type_handle_of(&base)?;
        roots.pin(base);
        return Some((definition, member_subst(&base)));
    }
    let definition = ctx.module().and_then(|m| m.type_registry.get(name).cloned())
        .or_else(|| ctx.try_lookup_type(name))?;
    Some((definition, None))
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
    let mut roots = ProjectionRoots::new(ctx);
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
        } else {
            return inherited_member_type(ctx, tag, subst, shadowed);
        }
    }
    make_type_from_name(ctx, tag)
}

pub(super) struct ProjectionRoots<'a> {
    heap: &'a crate::gc::Heap,
    handles: Vec<crate::gc::RootHandle>,
}

impl<'a> ProjectionRoots<'a> {
    pub(super) fn new(ctx: &'a VmContext) -> Self {
        Self { heap: ctx.heap(), handles: Vec::new() }
    }

    pub(super) fn pin(&mut self, value: Value) -> Value {
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
