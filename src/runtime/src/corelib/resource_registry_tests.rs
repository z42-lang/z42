use super::{ResourceRegistry, state};
use crate::{vm::context::VmContext, vm::exec::call::builtin_abi::BuiltinError};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

#[test]
fn resource_slots_are_monotonic_and_independent_per_registry() {
    let first = ResourceRegistry::new();
    let second = ResourceRegistry::new();
    assert_eq!(first.insert_new(11), 1);
    assert_eq!(second.insert_new(22), 1);
    assert_eq!(first.take(1), Some(11));
    assert_eq!(first.take(1), None);
    assert_eq!(first.insert_new(33), 2);
    assert_eq!(first.alloc_id(), 3);
    first.lock().insert(3, 44);
    assert_eq!(first.with_mut(3, |value| { *value += 1; *value }), Some(45));
    assert_eq!(first.count(), 2);
}

#[test]
fn cloned_handle_survives_close_and_releases_the_table_lock() {
    let table = ResourceRegistry::new();
    let id = table.insert_new(Arc::new(AtomicUsize::new(0)));
    let handle = table.get_cloned(id).unwrap();
    // Taking the slot immediately would deadlock if the clone retained the table lock.
    let removed = table.take(id).unwrap();
    handle.store(17, Ordering::Relaxed);
    assert_eq!(removed.load(Ordering::Relaxed), 17);
    assert_eq!(table.count(), 0);
    assert!(table.get_cloned(id).is_none());
}

#[test]
fn missing_required_state_is_an_internal_error() {
    struct Unregistered;
    let ctx = VmContext::new();
    assert!(matches!(state::<Unregistered>(&ctx), Err(BuiltinError::Internal(message))
        if message.contains("Unregistered")));
}

use crate::assembly::context::VmContextFactory as _;
