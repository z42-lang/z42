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
    let loader = RuntimeLoader::new(&state, &metadata, &cache, events.module_loads());
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

#[test]
fn context_load_registers_a_private_assembly_and_rejects_inactive_contexts() {
    use crate::loader::contexts::{ContextLoadError, UnloadOutcome};

    static CATALOG: crate::metadata::builtins::BuiltinCatalog =
        crate::metadata::builtins::BuiltinCatalog::new(&[]);
    let fs = Arc::new(crate::pal::fs::MemoryFs::new());
    let artifact = include_bytes!(concat!(
        env!("Z42_RUNTIME_DIR"),
        "/../compiler/z42.package/tests/fixtures/zpkg-format/indexed-minimal/source.zbc"
    ));
    fs.mount("/fixtures/plugin.zbc", artifact.to_vec());
    let state = crate::loader::LoaderState::new(fs, crate::loader::FUSION_INPUTS);
    let metadata = crate::metadata::RuntimeState::new(None, &CATALOG);
    let cache = crate::metadata::ThreadRuntimeState::new();
    let events = crate::diagnostics::RuntimeObserverRegistry::new();
    let loader = RuntimeLoader::new(&state, &metadata, &cache, events.module_loads());
    loader.install_lazy_loader(None, 0);
    let context = loader.create_collectible_context("plugins");

    assert!(matches!(
        loader.load_context_artifact(context, "/missing.zbc"),
        Err(ContextLoadError::Artifact(_))
    ));
    let assembly = loader
        .load_context_artifact(context, "/fixtures/plugin.zbc")
        .unwrap();
    assert_eq!(loader.assembly_name(assembly).as_deref(), Some("plugin"));
    assert_eq!(loader.context_assemblies(context), [assembly]);
    let functions = loader.assembly_function_names(assembly);
    assert!(!functions.is_empty());
    assert!(functions.iter().all(|name| loader.try_lookup_function(name).is_none()));

    assert_eq!(loader.unload_context(context), UnloadOutcome::Marked);
    assert!(matches!(
        loader.load_context_artifact(context, "/missing.zbc"),
        Err(ContextLoadError::Inactive)
    ));
    assert_eq!(loader.context_assemblies(context), [assembly]);
}
