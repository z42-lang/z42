//! `ScriptObject`'s cold metadata word (`ObjMeta`): interned type-arguments + boxed native.

use super::*;
use crate::metadata::context::ContextId;

fn td(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        fields: Vec::new(),
        field_index: crate::metadata::NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        cold: None,
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

fn obj(name: &str) -> ScriptObject {
    ScriptObject::new(td(name), ObjStorage::new(0))
}

fn names(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

#[test]
fn instances_of_one_instantiation_share_one_entry() {
    // Two sites (or two packages) intern from independently-built lists.
    let (mut a, mut b) = (obj("Demo.Box"), obj("Demo.Box"));
    a.install_type_args(TypeArgs::intern(&names(&["Std.Int32"])));
    b.install_type_args(TypeArgs::intern(&names(&["Std.Int32"])));
    assert_eq!(a.type_args(), &["Std.Int32".to_string()][..]);
    assert!(std::ptr::eq(a.type_args().as_ptr(), b.type_args().as_ptr()), "one shared entry");
    // Nothing is allocated per object for the arguments.
    assert_eq!(a.heap_payload_bytes(), 0);
}

#[test]
fn non_generic_object_has_no_metadata() {
    let mut o = obj("Demo.Plain");
    o.install_type_args(TypeArgs::empty());
    o.set_native(NativeData::None);
    assert!(o.type_args().is_empty());
    assert!(matches!(o.native(), NativeData::None));
    assert_eq!(o.heap_payload_bytes(), 0);
}

#[test]
fn native_backing_keeps_type_args_in_either_order() {
    let ta = TypeArgs::intern(&names(&["Std.String"]));

    let mut a = obj("Demo.Handle");
    a.set_type_args(ta);
    a.set_native(NativeData::LoadContextHandle(ContextId(7)));
    assert_eq!(a.type_args(), ta.as_slice());
    assert!(matches!(a.native(), NativeData::LoadContextHandle(ContextId(7))));

    let mut b = obj("Demo.Handle");
    b.set_native(NativeData::LoadContextHandle(ContextId(8)));
    b.set_type_args(ta);
    assert!(std::ptr::eq(b.type_args().as_ptr(), ta.as_slice().as_ptr()));
    assert!(matches!(b.native(), NativeData::LoadContextHandle(ContextId(8))));
    // Only the native box is charged.
    assert!(b.heap_payload_bytes() > 0);

    // Replacing the native keeps the arguments; clearing them keeps the native.
    b.set_native(NativeData::LoadContextHandle(ContextId(9)));
    assert_eq!(b.type_args(), ta.as_slice());
    b.set_type_args(TypeArgs::empty());
    assert!(b.type_args().is_empty());
    assert!(matches!(b.native(), NativeData::LoadContextHandle(ContextId(9))));
}

#[test]
fn native_backing_is_dropped_with_the_object() {
    let monitor = Arc::new(crate::corelib::monitor::Monitor::default());
    let probe = Arc::downgrade(&monitor);
    {
        let mut o = obj("Std.Threading.Mutex");
        o.set_type_args(TypeArgs::intern(&names(&["Std.Int32"])));
        o.set_native(NativeData::Monitor(monitor));
        assert!(probe.upgrade().is_some());
    }
    assert!(probe.upgrade().is_none(), "the extras box (and its Arc) is freed on drop");
}
