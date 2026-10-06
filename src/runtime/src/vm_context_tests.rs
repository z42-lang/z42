use super::*;

// ── static_fields ─────────────────────────────────────────────────────────────

#[test]
fn static_get_unset_returns_null() {
    let ctx = VmContext::new();
    assert!(matches!(ctx.static_get("Foo.Bar"), Value::Null));
}

#[test]
fn static_set_then_get_roundtrip() {
    let ctx = VmContext::new();
    ctx.static_set("Foo.Bar", Value::I64(42));
    assert!(matches!(ctx.static_get("Foo.Bar"), Value::I64(42)));
}

#[test]
fn static_fields_clear_drops_all() {
    let ctx = VmContext::new();
    ctx.static_set("A", Value::I64(1));
    ctx.static_set("B", Value::I64(2));
    ctx.static_fields_clear();
    assert!(matches!(ctx.static_get("A"), Value::Null));
    assert!(matches!(ctx.static_get("B"), Value::Null));
}

#[test]
fn two_contexts_static_fields_isolated() {
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new();
    ctx1.static_set("Foo", Value::I64(1));
    ctx2.static_set("Foo", Value::I64(2));
    assert!(matches!(ctx1.static_get("Foo"), Value::I64(1)));
    assert!(matches!(ctx2.static_get("Foo"), Value::I64(2)));
}

// ── pending_exception ─────────────────────────────────────────────────────────

#[test]
fn pending_exception_take_unset_is_none() {
    let ctx = VmContext::new();
    assert!(ctx.take_exception().is_none());
}

#[test]
fn pending_exception_set_then_take_consumes() {
    let ctx = VmContext::new();
    ctx.set_exception(Value::Str("oops".into()));
    let v = ctx.take_exception();
    assert!(matches!(v, Some(Value::Str(ref s)) if **s == *"oops"));
    // Subsequent take returns None (consumed).
    assert!(ctx.take_exception().is_none());
}

#[test]
fn two_contexts_exception_isolated() {
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new();
    ctx1.set_exception(Value::Str("ctx1".into()));
    // ctx2 stays empty.
    assert!(ctx2.take_exception().is_none());
    let v = ctx1.take_exception();
    assert!(matches!(v, Some(Value::Str(ref s)) if **s == *"ctx1"));
}

// ── pending_thrown ────────────────────────────────────────────────────────────

#[test]
fn pending_thrown_is_per_thread() {
    // Two threads of one VM share a `VmCore`; a builtin's thrown value must
    // reach only the thread whose builtin set it.
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new_with_core(std::sync::Arc::clone(&ctx1.core));
    ctx1.set_pending_thrown(Value::Str("ctx1".into()));
    assert!(ctx2.take_pending_thrown().is_none());
    let v = ctx1.take_pending_thrown();
    assert!(matches!(v, Some(Value::Str(ref s)) if **s == *"ctx1"));
}

#[test]
fn pending_thrown_is_a_gc_root() {
    let ctx = VmContext::new();
    let heap = ctx.heap();
    heap.set_mode(crate::gc::GcMode::StwMarkSweep);
    let exc = heap.alloc_array(vec![Value::I64(1)]);
    let weak = heap.make_weak(&exc).expect("heap object");
    ctx.set_pending_thrown(exc);
    heap.force_collect();
    assert!(heap.upgrade_weak(&weak).is_some(), "value held only by pending_thrown was swept");
    assert!(ctx.take_pending_thrown().is_some());
}

// ── GC heap ───────────────────────────────────────────────────────────────────

#[test]
fn heap_is_installed_by_default() {
    let ctx = VmContext::new();
    // Default Phase 1 backend: ArcMagrGC, alloc starts at 0.
    assert_eq!(ctx.heap().stats().allocations, 0);
}

#[test]
fn heap_alloc_array_increments_stats() {
    let ctx = VmContext::new();
    let v = ctx.heap().alloc_array(vec![Value::I64(1)]);
    assert!(matches!(v, Value::Array(_)));
    assert_eq!(ctx.heap().stats().allocations, 1);
}

#[test]
fn two_contexts_heap_isolated() {
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new();
    let _ = ctx1.heap().alloc_array(vec![]);
    let _ = ctx1.heap().alloc_array(vec![Value::I64(1)]);
    assert_eq!(ctx1.heap().stats().allocations, 2);
    assert_eq!(ctx2.heap().stats().allocations, 0);
}

// ── lazy_loader install / uninstall ───────────────────────────────────────────

#[test]
fn lazy_loader_install_then_uninstall_is_clean() {
    let ctx = VmContext::new();
    ctx.install_lazy_loader(None, 0);
    assert!(ctx.try_lookup_function("Std.IO.Console.WriteLine").is_none());
    ctx.uninstall_lazy_loader();
    assert!(ctx.try_lookup_function("Anything.Foo").is_none());
}

#[test]
fn lazy_loader_two_contexts_independent() {
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new();
    ctx1.install_lazy_loader(None, 0);
    // ctx2 has no loader installed.
    assert_eq!(ctx1.declared_namespaces(), Vec::<String>::new());
    assert_eq!(ctx2.declared_namespaces(), Vec::<String>::new());
}

// ── add-vmcontext-registry (2026-05-20) ───────────────────────────────────────

#[test]
fn vm_context_registers_self_on_new() {
    let ctx = VmContext::new();
    let registry = ctx.core.vm_contexts.lock();
    assert_eq!(registry.len(), 1, "newly-constructed VmContext should be in registry");
    let self_ptr = &*ctx as *const VmContext;
    assert!(
        registry.iter().any(|p| p.0 == self_ptr),
        "registry should contain self-pointer"
    );
}

#[test]
fn vm_context_drop_removes_from_registry() {
    let ctx = VmContext::new();
    let core = std::sync::Arc::clone(&ctx.core);
    let self_ptr = &*ctx as *const VmContext;
    assert!(core.vm_contexts.lock().iter().any(|p| p.0 == self_ptr));
    drop(ctx);
    assert_eq!(core.vm_contexts.lock().len(), 0, "drop should clear the entry");
}

#[test]
fn two_vm_contexts_both_registered_independently() {
    // Each `VmContext::new()` constructs its own VmCore; `new_with_core`
    // (below) covers the shared-core case for `__thread_spawn`. The intent
    // of this test is to verify independence: ctx1 dropping shouldn't
    // affect ctx2.
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new();
    assert_eq!(ctx1.core.vm_contexts.lock().len(), 1);
    assert_eq!(ctx2.core.vm_contexts.lock().len(), 1);
    drop(ctx1);
    assert_eq!(ctx2.core.vm_contexts.lock().len(), 1, "ctx2 unaffected by ctx1 drop");
}

// ── add-threading-stdlib Phase 3 (2026-05-20) ─────────────────────────────────

#[test]
fn vm_context_new_with_core_shares_core() {
    // Construct primary ctx; spawn-clone its core into a second ctx;
    // verify both register self into the same `vm_contexts` registry.
    let ctx1 = VmContext::new();
    let core = std::sync::Arc::clone(&ctx1.core);
    let ctx2 = VmContext::new_with_core(core);

    let registry = ctx1.core.vm_contexts.lock();
    assert_eq!(registry.len(), 2, "shared-core registry should contain both ctxs");

    let p1 = &*ctx1 as *const VmContext;
    let p2 = &*ctx2 as *const VmContext;
    assert!(registry.iter().any(|p| p.0 == p1));
    assert!(registry.iter().any(|p| p.0 == p2));
}

#[test]
fn vm_context_new_with_core_drop_only_removes_self() {
    let ctx1 = VmContext::new();
    let core = std::sync::Arc::clone(&ctx1.core);
    let ctx2 = VmContext::new_with_core(core);
    assert_eq!(ctx1.core.vm_contexts.lock().len(), 2);
    let p1 = &*ctx1 as *const VmContext;
    drop(ctx2);
    let registry = ctx1.core.vm_contexts.lock();
    assert_eq!(registry.len(), 1, "ctx2 drop should not remove ctx1");
    assert_eq!(registry[0].0, p1);
}

#[test]
fn vm_context_new_with_core_shares_static_fields() {
    // Static fields live on VmCore — both contexts should see the same
    // values when one writes.
    let ctx1 = VmContext::new();
    let ctx2 = VmContext::new_with_core(std::sync::Arc::clone(&ctx1.core));
    ctx1.static_set("Shared", Value::I64(7));
    assert!(matches!(ctx2.static_get("Shared"), Value::I64(7)));
}
