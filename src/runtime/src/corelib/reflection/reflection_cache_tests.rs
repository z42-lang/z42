use super::reflection_tests::bare_td;
use super::*;
use crate::vm::runtime::ids::tokens::TypeId;

fn reflection_cache_context() -> std::pin::Pin<Box<VmContext>> {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    ctx.core
        .loader
        .lazy_loader
        .write()
        .as_mut()
        .unwrap()
        .insert_type("Std.Type".into(), bare_td("Std.Type"));
    ctx
}

#[test]
fn type_objects_are_reused_within_one_metadata_snapshot() {
    let ctx = reflection_cache_context();
    let td = bare_td("Demo.CacheTarget");
    let first = make_type_object(&ctx, td.clone());
    let second = make_type_object(&ctx, td.clone());
    let (Value::Object(a), Value::Object(b)) = (&first, &second) else {
        panic!()
    };
    assert!(crate::object::ObjRef::ptr_eq(a, b));

    ctx.core
        .loader
        .lazy_loader
        .write()
        .as_mut()
        .unwrap()
        .insert_type("Demo.Added".into(), bare_td("Demo.Added"));
    let Value::Object(new) = make_type_object(&ctx, td) else {
        panic!()
    };
    assert!(!crate::object::ObjRef::ptr_eq(a, &new));
    assert_eq!(
        reflected_type_key(&first),
        reflected_type_key(&Value::Object(new))
    );
}

#[test]
fn type_object_cache_does_not_root_the_managed_object() {
    let ctx = reflection_cache_context();
    let td = bare_td("Demo.CacheTarget");
    let first = make_type_object(&ctx, td.clone());
    let weak = ctx.heap().make_weak(&first).unwrap();
    ctx.heap().force_collect();
    assert!(ctx.heap().upgrade_weak(&weak).is_none());
    let recreated = make_type_object(&ctx, td);
    assert!(matches!(recreated, Value::Object(_)));
    assert_eq!(
        reflected_type_key(&recreated),
        Some(crate::object::ReflectedTypeKey::Symbol(
            "Demo.CacheTarget".into()
        ))
    );
}

#[test]
fn type_object_cache_is_owned_by_one_vm() {
    let left = reflection_cache_context();
    let right = reflection_cache_context();
    let td = bare_td("Demo.CacheTarget");
    let Value::Object(a) = make_type_object(&left, td.clone()) else {
        panic!()
    };
    let Value::Object(b) = make_type_object(&right, td) else {
        panic!()
    };
    assert!(!crate::object::ObjRef::ptr_eq(&a, &b));
}

#[test]
fn type_object_cache_separates_same_named_declarations() {
    let ctx = reflection_cache_context();
    let mut first = bare_td("Plugin.Item");
    let mut second = bare_td("Plugin.Item");
    Arc::get_mut(&mut first).unwrap().id = TypeId(101);
    Arc::get_mut(&mut second).unwrap().id = TypeId(102);
    let left = make_type_object(&ctx, first.clone());
    let right = make_type_object(&ctx, second);
    assert_ne!(reflected_type_key(&left), reflected_type_key(&right));
    let Value::Object(a) = left else { panic!() };
    let Value::Object(b) = make_type_object(&ctx, first) else {
        panic!()
    };
    assert!(crate::object::ObjRef::ptr_eq(&a, &b));
}

#[test]
fn type_object_cache_rejects_a_late_publication_from_an_old_snapshot() {
    let ctx = reflection_cache_context();
    let td = bare_td("Demo.CacheTarget");
    let old_revision = ctx.reflection_revision();
    let old = make_type_object(&ctx, td.clone());
    let key = reflected_type_key(&old).unwrap();
    ctx.core
        .loader
        .lazy_loader
        .write()
        .as_mut()
        .unwrap()
        .insert_type("Demo.Added".into(), bare_td("Demo.Added"));
    let current = make_type_object(&ctx, td.clone());
    ctx.core
        .runtime
        .reflection_types
        .publish(ctx.heap(), old_revision, key, Vec::new(), old);
    let Value::Object(a) = current else { panic!() };
    let Value::Object(b) = make_type_object(&ctx, td) else {
        panic!()
    };
    assert!(crate::object::ObjRef::ptr_eq(&a, &b));
}
