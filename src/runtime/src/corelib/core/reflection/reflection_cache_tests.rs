use super::reflection_tests::bare_td;
use super::*;
use crate::object::{FieldSlot, ReflectedTypeHandle, TypeDescCold};
use crate::metadata::ids::{
    tokens::{alloc_type_id_block, TypeId},
    type_table::TypeTable,
};

fn declaration(name: &str) -> Arc<TypeDesc> {
    let mut td = bare_td(name);
    Arc::get_mut(&mut td).unwrap().id = TypeId(alloc_type_id_block(1));
    td
}

fn fields(mut td: Arc<TypeDesc>, names: &[&str]) -> Arc<TypeDesc> {
    for name in names {
        let td = Arc::make_mut(&mut td);
        td.field_index.insert((*name).into(), td.fields.len());
        td.fields.push(FieldSlot {
            name: (*name).into(),
            type_tag: "object".into(),
            visibility: 0,
        });
    }
    td
}

fn publish(ctx: &VmContext, td: &Arc<TypeDesc>) {
    let mut loader = ctx.core.loader.lazy_loader.write();
    let loader = loader.as_mut().unwrap();
    loader.insert_type(td.name.clone(), td.clone());
    loader.publish_types();
}

fn reflection_cache_context() -> std::pin::Pin<Box<VmContext>> {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    let shell = fields(
        bare_td("Std.Type"),
        &[
            "Name",
            "__name",
            "__fullName",
            "IsArray",
            "__elementType",
            "__typeArgs",
            "__attributeCache",
        ],
    );
    publish(&ctx, &shell);
    publish(
        &ctx,
        &fields(bare_td(STD_REFLECTION_FIELDINFO), &["Name", "FieldType"]),
    );
    ctx
}

fn same_object(a: &Value, b: &Value) -> bool {
    matches!((a, b), (Value::Object(a), Value::Object(b)) if crate::object::ObjRef::ptr_eq(a, b))
}

#[test]
fn type_objects_survive_unrelated_loads_and_pending_initialization() {
    let ctx = reflection_cache_context();
    let td = declaration("Demo.CacheTarget");
    publish(&ctx, &td);
    let first = make_type_object(&ctx, td.clone());
    publish(&ctx, &declaration("Demo.Added"));
    ctx.enqueue_type_load("Demo.Pending");
    assert!(same_object(&first, &make_type_object(&ctx, td.clone())));
    assert!(ctx.type_load_drain_is_noop(&[]));
    assert!(same_object(&first, &make_type_object(&ctx, td)));
}

#[test]
fn held_type_reads_republished_metadata_without_changing_identity() {
    let ctx = reflection_cache_context();
    let initial = declaration("Demo.CacheTarget");
    publish(&ctx, &initial);
    let held = make_type_object(&ctx, initial.clone());
    assert!(!type_is_sealed(&held));
    assert_eq!(
        super::reflection_tests::array_len(&type_fields(&ctx, &held).unwrap()),
        0
    );
    let mut updated = fields(initial.clone(), &["Added"]);
    Arc::make_mut(&mut updated).class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    publish(&ctx, &updated);
    assert!(type_is_sealed(&held));
    assert!(Arc::ptr_eq(&type_handle_of(&held).unwrap(), &updated));
    assert_eq!(
        super::reflection_tests::array_len(&type_fields(&ctx, &held).unwrap()),
        1
    );
    assert!(same_object(&held, &make_type_object(&ctx, updated)));
    assert!(same_object(&held, &make_type_object(&ctx, initial)));
}

#[test]
fn constructed_type_uses_current_definition_and_preserves_its_arguments() {
    let ctx = reflection_cache_context();
    let mut initial = declaration("Demo.Box");
    Arc::make_mut(&mut initial).cold = Some(Box::new(TypeDescCold {
        type_params: vec!["T".into()].into_boxed_slice(),
        ..Default::default()
    }));
    publish(&ctx, &initial);
    let argument = make_type_object(&ctx, declaration("Plugin.Argument"));
    let held = super::type_object::build_constructed_type(
        &ctx,
        Some(initial.clone()),
        initial.name.clone(),
        vec![argument],
    );
    let mut updated = (*initial).clone();
    updated.class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    publish(&ctx, &Arc::new(updated));
    assert!(type_is_sealed(&held));
    let repeated = super::type_object::build_constructed_type(
        &ctx,
        Some(initial.clone()),
        initial.name.clone(),
        vec![argument],
    );
    assert!(same_object(&held, &repeated));
    let Value::Array(args) = type_generic_args(&ctx, &held) else {
        panic!()
    };
    assert!(same_object(&argument, &args.borrow().get(0).unwrap()));
}

#[test]
fn type_object_cache_does_not_root_the_managed_object() {
    let ctx = reflection_cache_context();
    let td = declaration("Demo.CacheTarget");
    publish(&ctx, &td);
    let first = make_type_object(&ctx, td.clone());
    let key = reflected_type_key(&first);
    let weak = ctx.heap().make_weak(&first).unwrap();
    ctx.heap().force_collect();
    assert!(ctx.heap().upgrade_weak(&weak).is_none());
    let recreated = make_type_object(&ctx, td);
    assert!(matches!(recreated, Value::Object(_)));
    assert_eq!(reflected_type_key(&recreated), key);
}

#[test]
fn type_object_cache_is_owned_by_one_vm() {
    let left = reflection_cache_context();
    let right = reflection_cache_context();
    let td = declaration("Demo.CacheTarget");
    assert!(!same_object(
        &make_type_object(&left, td.clone()),
        &make_type_object(&right, td)
    ));
}

#[test]
fn same_named_declarations_keep_identity_and_assignability_separate() {
    let ctx = reflection_cache_context();
    let first = declaration("Plugin.Item");
    let second = declaration("Plugin.Item");
    publish(&ctx, &first);
    let left = make_type_object(&ctx, first.clone());
    let right = make_type_object(&ctx, second.clone());
    assert_ne!(reflected_type_key(&left), reflected_type_key(&right));
    assert!(!type_is_assignable_from(&ctx, &left, &right));
    assert!(!type_is_assignable_from(&ctx, &right, &left));
    assert!(same_object(&left, &make_type_object(&ctx, first)));
    publish(&ctx, &second);
    assert_ne!(
        type_handle_of(&left).unwrap().id,
        type_handle_of(&right).unwrap().id
    );
}

#[test]
fn published_handle_does_not_keep_its_owner_alive_or_resurrect_metadata() {
    let table = Arc::new(TypeTable::default());
    let td = declaration("Demo.Target");
    table.publish(&td);
    let owner = Arc::downgrade(&table);
    let handle = ReflectedTypeHandle::new(&table, td);
    assert!(handle.resolve().is_some());
    drop(table);
    assert!(owner.upgrade().is_none());
    assert!(handle.resolve().is_none());
}

#[test]
fn thread_type_lookup_cache_follows_same_id_publication() {
    let ctx = reflection_cache_context();
    let initial = declaration("Demo.Target");
    publish(&ctx, &initial);
    ctx.runtime
        .type_lookup_cache
        .lock()
        .insert(initial.name.clone().into(), initial.clone());
    let mut updated = (*initial).clone();
    updated.class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    let updated = Arc::new(updated);
    publish(&ctx, &updated);
    assert!(Arc::ptr_eq(
        &ctx.try_lookup_type(&initial.name).unwrap(),
        &updated
    ));
}

#[test]
fn type_attribute_cache_tracks_its_descriptor_without_replacing_type() {
    let ctx = reflection_cache_context();
    let initial = declaration("Demo.Target");
    publish(&ctx, &initial);
    let held = make_type_object(&ctx, initial.clone());
    let root = ctx.heap().pin_root(held);
    let first = type_custom_attributes(&ctx, &held).unwrap();
    assert_eq!(first, type_custom_attributes(&ctx, &held).unwrap());
    publish(&ctx, &declaration("Demo.Unrelated"));
    assert_eq!(first, type_custom_attributes(&ctx, &held).unwrap());
    let mut updated = (*initial).clone();
    updated.class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    publish(&ctx, &Arc::new(updated));
    let refreshed = type_custom_attributes(&ctx, &held).unwrap();
    assert_ne!(first, refreshed);
    assert_eq!(refreshed, type_custom_attributes(&ctx, &held).unwrap());
    ctx.heap().force_collect();
    assert_eq!(refreshed, type_custom_attributes(&ctx, &held).unwrap());
    ctx.heap().unpin_root(root);
}

#[test]
fn isolated_object_name_does_not_gain_root_object_assignability() {
    let ctx = reflection_cache_context();
    let root = declaration(STD_OBJECT);
    publish(&ctx, &root);
    let root = make_type_object(&ctx, root);
    let isolated = make_type_object(&ctx, declaration(STD_OBJECT));
    let value_type = make_type_object(&ctx, declaration("Plugin.Value"));
    assert!(type_is_assignable_from(&ctx, &root, &value_type));
    assert!(!type_is_assignable_from(&ctx, &isolated, &value_type));
}

#[test]
fn array_element_keeps_identity_and_reads_current_metadata() {
    let ctx = reflection_cache_context();
    let initial = declaration("Demo.Element");
    publish(&ctx, &initial);
    let element = make_type_object(&ctx, initial.clone());
    let array = super::type_object::make_array_type(&ctx, &initial.name, element);
    let mut updated = (*initial).clone();
    updated.class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    publish(&ctx, &Arc::new(updated));
    let projected = type_element(&ctx, &array);
    assert!(same_object(&element, &projected));
    assert!(type_is_sealed(&projected));
}

#[test]
fn late_attribute_result_cannot_restore_an_old_metadata_cache() {
    let ctx = reflection_cache_context();
    let initial = declaration("Demo.Target");
    publish(&ctx, &initial);
    let held = make_type_object(&ctx, initial.clone());
    let stale = type_custom_attributes(&ctx, &held).unwrap();
    let mut updated = (*initial).clone();
    updated.class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    publish(&ctx, &Arc::new(updated));
    let current = type_custom_attributes(&ctx, &held).unwrap();
    let late = ctx.types().with_current(&initial, || {
        ctx.core.runtime.reflection_types.store_attributes(
            ctx.heap(),
            &held,
            Some(initial.id),
            stale,
        )
    });
    assert!(late.is_none());
    assert_eq!(current, type_custom_attributes(&ctx, &held).unwrap());
}

#[test]
fn metadata_update_invalidates_all_live_projection_objects() {
    let ctx = reflection_cache_context();
    let mut definition = declaration("Demo.Box");
    Arc::make_mut(&mut definition).cold = Some(Box::new(TypeDescCold {
        type_params: vec!["T".into()].into_boxed_slice(),
        ..Default::default()
    }));
    publish(&ctx, &definition);
    let argument = declaration("Plugin.Argument");
    let shell = ctx.try_lookup_type("Std.Type").unwrap();
    let make_argument = || {
        ctx.heap().alloc_object(
            shell.clone(),
            vec![
                Value::Str("Argument".into()),
                Value::Str("Argument".into()),
                Value::Str("Plugin.Argument".into()),
                Value::Bool(false),
                Value::Null,
                Value::Null,
                Value::Null,
            ],
            NativeData::TypeHandle(ReflectedTypeHandle::declaration(argument.clone())),
        )
    };
    let first = super::type_object::build_constructed_type(
        &ctx,
        Some(definition.clone()),
        definition.name.clone(),
        vec![make_argument()],
    );
    let second = super::type_object::build_constructed_type(
        &ctx,
        Some(definition.clone()),
        definition.name.clone(),
        vec![make_argument()],
    );
    assert!(!same_object(&first, &second));
    let old_first = type_custom_attributes(&ctx, &first).unwrap();
    let old_second = type_custom_attributes(&ctx, &second).unwrap();
    let mut updated = (*definition).clone();
    updated.class_flags |= crate::zpkg::ir::CLASS_FLAG_SEALED;
    publish(&ctx, &Arc::new(updated));
    assert_ne!(old_first, type_custom_attributes(&ctx, &first).unwrap());
    assert_ne!(old_second, type_custom_attributes(&ctx, &second).unwrap());
}
