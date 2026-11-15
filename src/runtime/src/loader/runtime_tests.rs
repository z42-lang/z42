use super::*;

#[test]
fn lookup_and_queue_drain_need_no_vm_heap_or_execution_engine() {
    static CATALOG: crate::metadata::builtins::BuiltinCatalog = crate::metadata::builtins::BuiltinCatalog::new(&[]);
    let metadata = crate::metadata::RuntimeState::new(None, &CATALOG);
    let cache = crate::metadata::ThreadRuntimeState::new();
    let state = crate::loader::LoaderState::new(
        Arc::new(crate::pal::fs::MemoryFs::new()),
        || crate::loader::fusion_inputs::FusionInputs { no_fusion: true, no_typed_fusion: true, fusion_debug: false },
    );
    let events = crate::diagnostics::RuntimeObserverRegistry::new();
    let loader = RuntimeLoader::new(&state, &metadata, &cache, &events);
    loader.install_lazy_loader(None, 0);
    assert!(loader.try_lookup_function("Missing.Run").is_none());
    assert!(loader.try_lookup_type("Missing.Type").is_none());
    loader.enqueue_type_load("Missing.Type");
    assert!(!loader.type_load_drain_is_noop(&[]));
    loader.drain_pending_type_loads();
    assert!(loader.type_load_drain_is_noop(&[]));
    assert!(!metadata.cctors.any_pending());
    loader.uninstall_lazy_loader();
    assert!(state.lazy_loader.read().is_none());
}
