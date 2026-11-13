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
    drop(a);
    assert!(weak.upgrade().is_none(), "profiler must not create a Core lease cycle");
}
