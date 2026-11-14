use crate::{components::BackendSource, context::VmContext, host};

#[test]
fn unused_profiler_has_no_backend_and_disabled_source_fails_on_demand() {
    let ctx = VmContext::new();
    let sampler = &ctx.core().diagnostics().sampler;
    sampler.set_source(BackendSource::Disabled).unwrap();
    sampler.poll(&ctx);
    assert!(!host::profiler_loaded(&ctx));
    let error = host::prepare_profiler(&ctx).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("disabled"), "{error}");
    assert_eq!(host::prepare_profiler(&ctx).unwrap_err().to_string(), error);
    assert!(!host::profiler_loaded(&ctx));
}
#[test]
fn missing_profiler_is_an_error_without_changing_normal_execution() {
    let ctx = VmContext::new();
    ctx.core().diagnostics().sampler.set_source(BackendSource::Dynamic(std::path::PathBuf::from("/missing/z42-profiler"))).unwrap();
    assert!(!host::profiler_loaded(&ctx));
    let error = host::prepare_profiler(&ctx).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("not found"), "{error}");
}
#[test]
fn linked_profiler_is_lazy_per_core_and_does_not_retain_the_vm() {
    let a = VmContext::new();
    let b = VmContext::new();
    a.core().diagnostics().sampler.set_source(BackendSource::Linked).unwrap();
    let weak = std::sync::Arc::downgrade(a.core());
    assert!(!host::profiler_loaded(&a));
    host::prepare_profiler(&a).unwrap();
    host::prepare_profiler(&a).unwrap();
    assert!(host::profiler_loaded(&a));
    assert!(!host::profiler_loaded(&b));
    assert!(host::component_ids(&a).contains(&host::PROFILER_COMPONENT));
    let lease = host::component_interface::<host::ProfilerApi>(&a, host::PROFILER_COMPONENT).unwrap().unwrap();
    drop(a);
    assert!(weak.upgrade().is_some(), "public interface lease keeps the VM alive");
    drop(lease);
    assert!(weak.upgrade().is_none(), "profiler must not create a Core lease cycle");
}

#[test]
fn explicitly_selected_profiler_uses_the_existing_component_registry() {
    let components = unsafe { host::ComponentOptions::default().with_static(
        host::PROFILER_COMPONENT, crate::profiler::component::z42_profiler_init) };
    let program = host::LoadedProgram::load_bytes(&crate::host::program_tests::empty_zbc(), host::LoadOptions {
        components, mode: host::ExecMode::Interp, ..Default::default()
    }).unwrap();
    assert!(!host::profiler_loaded(program.context()));
    host::prepare_profiler(program.context()).unwrap();
    assert_eq!(host::component_ids(program.context()).iter().filter(|&&id| id == host::PROFILER_COMPONENT).count(), 1);
}

#[test]
fn loaded_program_applies_profiler_source_without_eager_attachment() {
    let program = host::LoadedProgram::load_bytes(
        &crate::host::program_tests::empty_zbc(),
        host::LoadOptions {
            profiler: BackendSource::Disabled,
            mode: host::ExecMode::Interp,
            ..Default::default()
        },
    ).unwrap();
    assert!(!host::profiler_loaded(program.context()));
    let error = host::prepare_profiler(program.context()).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("disabled"), "{error}");
    assert!(!host::profiler_loaded(program.context()));
}
