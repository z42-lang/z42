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
        id: crate::vm::runtime::ids::tokens::TypeId::UNRESOLVED,
    })
}

fn obj(name: &str) -> OwnedObject {
    OwnedObject::new(td(name), 0)
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
    assert_eq!(a.extras_bytes(), 0);
}

#[test]
fn non_generic_object_has_no_metadata() {
    let mut o = obj("Demo.Plain");
    o.install_type_args(TypeArgs::empty());
    o.set_native(NativeData::None);
    assert!(o.type_args().is_empty());
    assert!(matches!(o.native(), NativeData::None));
    assert_eq!(o.extras_bytes(), 0);
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
    assert!(b.extras_bytes() > 0);

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

#[test]
fn the_dst_fields_start_right_after_the_sized_header_prefix() {
    // `ObjHeader` is the object view of the 16 B block header and `ScriptObject` is read over
    // the whole block: under `#[repr(C)]` they share the prefix, and the field area starts at the
    // block's `DATA_OFFSET`.
    use crate::object::header::GcBlockHeader;
    let o = OwnedObject::new(td("Layout"), 24);
    let base = (&*o as *const ScriptObject).cast::<u8>() as usize;
    assert_eq!(o.cells().base_ptr() as usize - base, GcBlockHeader::DATA_OFFSET);
    assert_eq!(std::mem::size_of_val(&*o), GcBlockHeader::DATA_OFFSET + 24, "the reference covers header + fields");
    assert_eq!(o.type_desc.name, "Layout", "type_desc reads through the prefix");
    let odd = OwnedObject::new(td("Odd"), 4);
    assert!(std::mem::size_of_val(&*odd) <= GcBlockHeader::DATA_OFFSET + 8, "rounded up to 8 only");
    assert_eq!(std::mem::size_of::<ObjHeader>(), GcBlockHeader::DATA_OFFSET);
}

/// Object model R8: a generic / native-backed object carries its meta word at the payload's end,
/// after the field area; `payload_len` and the cell view stop before it, the reference covers it.
#[test]
fn the_meta_word_trails_the_fields_and_only_generic_or_native_objects_have_one() {
    use crate::object::header::GcBlockHeader;
    let plain = OwnedObject::new(td("Plain"), 12);
    assert!(!plain.has_meta());
    let mut g = OwnedObject::new(td("Gen"), 12);
    g.cells().store_bits(8, 4, 0xDEAD_BEEF);
    g.set_type_args(TypeArgs::intern(&names(&["Std.Int32"])));
    assert!(g.has_meta(), "giving it type-arguments grows the meta word");
    assert_eq!(g.payload_len(), 12, "the field area is unchanged");
    assert_eq!(g.cells().load_bits(8, 4), Some(0xDEAD_BEEF), "the fields moved with the object");
    assert_eq!(std::mem::size_of_val(&*g), GcBlockHeader::DATA_OFFSET + 12 + 8 + 4, "covers fields + meta, rounded to 8");
    assert_eq!(g.type_args(), &["Std.Int32".to_string()][..]);
    // Clearing the arguments keeps the (now null) word.
    g.set_type_args(TypeArgs::empty());
    assert!(g.type_args().is_empty() && g.has_meta());
    let built = OwnedObject::new_instance(td("Gen"), TypeArgs::intern(&names(&["Std.String"])));
    assert!(built.has_meta() && built.payload_len() == 0);
    assert_eq!(built.type_args(), &["Std.String".to_string()][..]);
}

/// What compiled field access reads off a handle (`obj_jit_layout`): the TypeId through the
/// header's `Arc<TypeDesc>`, the field cells, and the age / remembered bits of the block header.
#[cfg(all(feature = "jit", target_pointer_width = "64"))]
#[test]
fn jit_layout_matches_the_accessors() {
    use super::obj_jit_layout as ol;
    let mut d = (*td("Demo.Pin")).clone();
    d.id = crate::vm::runtime::ids::tokens::TypeId(0x5151_0001);
    let r = crate::object::ObjRef::leak(OwnedObject::new(Arc::new(d), 16));
    let hdr = (r.to_tagged_bits() & ol::HANDLE_ADDR_MASK) as *const u8;
    unsafe {
        // The type word read as a pointer (as compiled code does — a load, no integer round-trip).
        let td_word = std::ptr::read(hdr.add(ol::TYPE_DESC_OFFSET as usize) as *const *const u8);
        let tid = std::ptr::read(td_word.add(ol::type_id_offset() as usize) as *const u32);
        assert_eq!(tid, r.type_desc().id.0);
        assert_eq!(hdr.add(ol::FIELDS_OFFSET as usize), r.borrow().cells().base_ptr());
        let tt = || *hdr.add(ol::TYPE_TAG_OFFSET as usize) as i64;
        for age in 1..=crate::object::header::MAX_GEN_AGE {
            r.var_ref().raise_gen_age_to(age);
            assert_eq!((tt() >> ol::AGE_SHIFT) & ol::AGE_MASK, crate::object::ObjRef::gen_age(&r) as i64);
        }
        assert_eq!(tt() & ol::REMEMBERED_BIT, 0);
        assert!(r.var_ref().try_set_remembered());
        assert_ne!(tt() & ol::REMEMBERED_BIT, 0);
        assert_eq!((tt() >> ol::AGE_SHIFT) & ol::AGE_MASK, crate::object::header::MAX_GEN_AGE as i64);
    }
}
