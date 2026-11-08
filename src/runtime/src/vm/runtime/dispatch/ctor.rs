//! Constructor-site ownership, type arguments and module-local descriptor facts.
use std::sync::Arc;
use crate::zpkg::ClassDesc;
use crate::object::{TypeDesc, FieldSlot};
use crate::vm::runtime::{Module, ids::func_table::FuncTable};
use crate::vm::runtime::dispatch::ic::ObjNewSite;

/// The site cache to use for `module`: `site` when `module` is the VM's entry module (the
/// id space `ctor` is stored in), else `None` (resolve by name, cache nothing).
#[inline]
pub(crate) fn site_for<'s>(
    funcs: &FuncTable, module: &Module, site: Option<&'s ObjNewSite>,
) -> Option<&'s ObjNewSite> {
    site.filter(|_| funcs.is_entry(module))
}

/// The new instance's generic type-arguments: the site's own (`ObjNewInsn::type_args`),
/// else — complete-generic-class-identity P1: an instantiation's class name already carries
/// its arguments (`Demo.Box<int>`), so the compiler ships no second copy on the instruction —
/// the ones the registry parsed off the name. Both are interned handles (decode / registration
/// time), so this is a pointer pick: every instance of an instantiation shares one entry.
/// Both engines hand the result to `ScriptObject::install_type_args` before the ctor runs.
#[inline]
pub(crate) fn instance_type_args(
    site_args: crate::object::TypeArgs, class: &TypeDesc,
) -> crate::object::TypeArgs {
    if site_args.is_empty() { class.type_args_handle() } else { site_args }
}

/// Build a minimal TypeDesc from the ClassDesc chain — used when the registry
/// is absent (merged stdlib modules arrive without pre-built TypeDesc).
pub fn make_fallback_type_desc(module: &Module, class_name: &str) -> TypeDesc {
    let mut fields: Vec<FieldSlot> = Vec::new();
    let mut base_name: Option<String> = None;
    let mut cur = class_name;
    let mut chain: Vec<&ClassDesc> = Vec::new();
    loop {
        if let Some(desc) = module.classes.iter().find(|c| c.name == cur) {
            chain.push(desc);
            match &desc.base_class {
                Some(b) => { base_name = Some(b.clone()); cur = b.as_str(); }
                None    => break,
            }
        } else {
            break;
        }
    }
    for desc in chain.iter().rev() {
        for f in &desc.fields {
            if !fields.iter().any(|s: &FieldSlot| &*s.name == f.name.as_str()) {
                fields.push(FieldSlot {
                    name: f.name.clone().into_boxed_str(),
                    type_tag: f.type_tag.clone().into_boxed_str(),
                    visibility: f.visibility,
                });
            }
        }
    }
    let field_index = fields.iter().enumerate().map(|(i, f)| (f.name.to_string(), i)).collect();
    // Fallback type — there's no separate "own vs inherited" split because
    // `chain` walked the inheritance chain by-name within this module. Mark
    // all fields as own (cross-zpkg fixup won't re-process this entry since
    // base resolution above is already complete).
    let own_fields = fields.clone();
    // Like the loader's fallback for a class without an object block: a synthesized layout
    // (references in type-parameter cells), so field access and GC walks read a stored layout
    // instead of synthesizing one per access.
    let composed = (!fields.is_empty())
        .then(|| std::sync::Arc::new(crate::object::synthesize_object_layout(&fields)));
    TypeDesc {
        name: class_name.to_string(),
        base_name,
        class_flags: 0,  // fallback TypeDesc — no class-shape info
        visibility: 0,
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::zpkg::NameIndex::new(),
        cold: Some(Box::new(crate::object::TypeDescCold {
            composed_object_layout: composed,
            own_fields: own_fields.into(),
            ..Default::default()
        })),
        id: crate::vm::runtime::ids::tokens::TypeId::UNRESOLVED,
    }
}
