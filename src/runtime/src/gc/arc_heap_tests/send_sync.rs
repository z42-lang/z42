//! Send + Sync compile-time assertions for the Phase 3 multi-threading
//! foundation (`add-multithreading-foundation`, 2026-05-20).
//!
//! These tests don't actually run anything — they assert at compile time
//! that the key VM types are `Send + Sync`. If a future change introduces
//! a `Rc<...>` or `RefCell<...>` field anywhere in the type tree, the
//! `assert_send_sync` invocation will fail to compile, surfacing the
//! regression immediately.

use std::sync::Arc;

use crate::gc::{GcRef, MagrGC, ArcMagrGC};
use crate::metadata::Value;
use crate::vm_context::{VmContext, VmContextPtr, VmCore};

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn vm_core_is_send_sync() {
    assert_send_sync::<VmCore>();
    assert_send_sync::<Arc<VmCore>>();
}

#[test]
fn vm_context_is_send_sync() {
    assert_send_sync::<VmContext>();
}

#[test]
fn gc_ref_value_types_are_send_sync() {
    // The two concrete GcRef instantiations that appear in Value variants.
    assert_send_sync::<GcRef<Vec<Value>>>();
    // ScriptObject path checked via Value enum below — separate explicit
    // crate::gc::ObjRef assertion requires re-exporting ScriptObject so
    // is covered indirectly through `Value`.
}

#[test]
fn value_is_send_sync() {
    assert_send_sync::<Value>();
}

#[test]
fn rc_magr_gc_is_send_sync() {
    assert_send_sync::<ArcMagrGC>();
}

#[test]
fn dyn_magr_gc_is_send_sync() {
    // `Arc<dyn MagrGC>` is what callers actually pass around once VmCore
    // shares the heap; verify the dyn version preserves Send + Sync.
    assert_send::<Box<dyn MagrGC>>();
    assert_sync::<Box<dyn MagrGC>>();
}

#[test]
fn vm_context_ptr_is_send_sync() {
    // add-vmcontext-registry (2026-05-20): VmContextPtr is a raw-pointer
    // newtype with `unsafe impl Send + Sync` — pin this at compile time
    // so removing the impl breaks the build.
    assert_send_sync::<VmContextPtr>();
}
