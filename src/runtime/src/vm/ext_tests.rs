use super::VmExtensions;
use crate::{pal::fs::default_vfs, vm_context::VmContext};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

#[test]
fn registered_state_is_borrowed_by_exact_type_and_missing_is_none() {
    let mut states = VmExtensions::new();
    states.register(String::from("tenant")).unwrap();
    states.register(42_u64).unwrap();
    let ctx = VmContext::with_extensions(None, default_vfs(), states);
    assert_eq!(ctx.ext::<String>().map(String::as_str), Some("tenant"));
    assert_eq!(ctx.ext::<u64>(), Some(&42));
    assert!(ctx.ext::<u32>().is_none());
    assert!(ctx.ext::<&'static str>().is_none());
}

#[test]
fn duplicate_registration_fails_without_overwriting_state() {
    let mut states = VmExtensions::new();
    states.register(String::from("first")).unwrap();
    let error = states.register(String::from("second")).unwrap_err();
    assert!(error.to_string().contains("already registered"));
    let ctx = VmContext::with_extensions(None, default_vfs(), states);
    assert_eq!(ctx.ext::<String>().map(String::as_str), Some("first"));
}

#[test]
fn different_vms_keep_independent_state() {
    let make = || {
        let mut states = VmExtensions::new();
        states.register(AtomicUsize::new(0)).unwrap();
        VmContext::with_extensions(None, default_vfs(), states)
    };
    let first = make();
    let second = make();
    first.ext::<AtomicUsize>().unwrap().store(17, Ordering::Relaxed);
    assert_eq!(second.ext::<AtomicUsize>().unwrap().load(Ordering::Relaxed), 0);
    assert!(!std::ptr::eq(first.ext::<AtomicUsize>().unwrap(), second.ext::<AtomicUsize>().unwrap()));
}

#[test]
fn contexts_on_another_thread_share_the_same_vm_state() {
    let mut states = VmExtensions::new();
    states.register(AtomicUsize::new(0)).unwrap();
    let parent = VmContext::with_extensions(None, default_vfs(), states);
    let core = parent.core_arc();
    std::thread::spawn(move || {
        let child = VmContext::new_with_core(core);
        child.ext::<AtomicUsize>().unwrap().fetch_add(1, Ordering::Relaxed);
    }).join().unwrap();
    assert_eq!(parent.ext::<AtomicUsize>().unwrap().load(Ordering::Relaxed), 1);
}

struct Marker<const N: usize>(Arc<Mutex<Vec<usize>>>);
impl<const N: usize> Drop for Marker<N> {
    fn drop(&mut self) { self.0.lock().unwrap().push(N); }
}

#[test]
fn state_lives_until_last_core_reference_and_drops_in_reverse_order() {
    let dropped = Arc::new(Mutex::new(Vec::new()));
    let mut states = VmExtensions::new();
    states.register(Marker::<1>(dropped.clone())).unwrap();
    states.register(Marker::<2>(dropped.clone())).unwrap();
    states.register(Marker::<3>(dropped.clone())).unwrap();
    let parent = VmContext::with_extensions(None, default_vfs(), states);
    let core = parent.core_arc();
    drop(parent);
    assert!(dropped.lock().unwrap().is_empty());
    let child = VmContext::new_with_core(core);
    assert!(child.ext::<Marker<2>>().is_some());
    drop(child);
    assert_eq!(*dropped.lock().unwrap(), vec![3, 2, 1]);
}

#[test]
fn unused_registrations_drop_their_state_in_reverse_order() {
    let dropped = Arc::new(Mutex::new(Vec::new()));
    let mut states = VmExtensions::new();
    states.register(Marker::<1>(dropped.clone())).unwrap();
    states.register(Marker::<2>(dropped.clone())).unwrap();
    drop(states);
    assert_eq!(*dropped.lock().unwrap(), vec![2, 1]);
}

#[test]
fn module_vfs_and_default_constructors_keep_their_contracts() {
    let module = crate::vm::runtime::Module::new(crate::metadata::image::ModuleData {
        name: "extensions".into(), string_pool: Vec::new(), classes: Vec::new(),
        func_index: Default::default(),
    }, Vec::new(), Default::default());
    let vfs = default_vfs();
    let mut states = VmExtensions::new();
    states.register(73_u64).unwrap();
    let ctx = VmContext::with_extensions(Some(module), vfs.clone(), states);
    assert_eq!(ctx.module().unwrap().name, "extensions");
    assert!(Arc::ptr_eq(ctx.vfs(), &vfs));
    assert_eq!(ctx.ext::<u64>(), Some(&73));
    drop(ctx);
    assert!(VmContext::new().ext::<u64>().is_none());
}
