use crate::components::{abi, api};
use crate::{components::api::BackendSource, vm::VmContext, host};

#[test]
fn unused_profiler_has_no_backend_and_disabled_source_fails_on_demand() {
    let ctx = VmContext::new();
    ctx.core().profiling().set_source(BackendSource::Disabled).unwrap();
    crate::diagnostics::poll_sampling(&ctx);
    crate::diagnostics::prepare_sampling(&ctx).unwrap();
    let non_heap = crate::object::Value::Null;
    assert!(crate::diagnostics::retention::direct_referrers(&ctx, &non_heap).unwrap().is_empty());
    assert!(crate::diagnostics::retention::retaining_roots(&ctx, &non_heap).unwrap().is_empty());
    assert!(!host::profiler_loaded(&ctx));
    assert!(matches!(
        crate::corelib::core::gc::typed::write_heap_snapshot(&ctx, "unused"),
        Err(crate::exec::builtin_abi::BuiltinError::NotSupported(_)),
    ));
    let error = host::prepare_profiler(&ctx).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("disabled"), "{error}");
    assert_eq!(host::prepare_profiler(&ctx).unwrap_err().to_string(), error);
    assert!(!host::profiler_loaded(&ctx));
}
#[test]
fn missing_profiler_is_an_error_without_changing_normal_execution() {
    let ctx = VmContext::new();
    ctx.core().profiling().set_source(BackendSource::Dynamic(std::path::PathBuf::from("/missing/z42-profiler"))).unwrap();
    assert!(!host::profiler_loaded(&ctx));
    let error = host::prepare_profiler(&ctx).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("not found"), "{error}");
}
#[test]
fn linked_profiler_is_lazy_per_core_and_does_not_retain_the_vm() {
    let a = VmContext::new();
    let b = VmContext::new();
    a.core().profiling().set_source(BackendSource::Linked).unwrap();
    let weak = std::sync::Arc::downgrade(a.core());
    assert!(!host::profiler_loaded(&a));
    host::prepare_profiler(&a).unwrap();
    host::prepare_profiler(&a).unwrap();
    assert!(host::profiler_loaded(&a));
    assert!(!host::profiler_loaded(&b));
    assert!(api::component_ids(&a).contains(&abi::PROFILER_COMPONENT));
    let lease = api::component_interface::<abi::ProfilerApi>(&a, abi::PROFILER_COMPONENT).unwrap().unwrap();
    drop(a);
    assert!(weak.upgrade().is_some(), "public interface lease keeps the VM alive");
    drop(lease);
    assert!(weak.upgrade().is_none(), "profiler must not create a Core lease cycle");
}

#[test]
fn explicitly_selected_profiler_uses_the_existing_component_registry() {
    let components = unsafe { api::ComponentOptions::default().with_static(
        abi::PROFILER_COMPONENT, crate::profiler::component::z42_profiler_init) };
    let program = host::LoadedProgram::load_bytes_with_components(&crate::loader::test_support::empty_zbc(), host::LoadOptions {
        mode: host::ExecMode::Interp, ..Default::default()
    }, crate::vm::assembly::ComponentInputs { components, ..Default::default() }).unwrap();
    assert!(!host::profiler_loaded(program.context()));
    host::prepare_profiler(program.context()).unwrap();
    assert_eq!(api::component_ids(program.context()).iter().filter(|&&id| id == abi::PROFILER_COMPONENT).count(), 1);
}

#[test]
fn loaded_program_applies_profiler_source_without_eager_attachment() {
    let program = host::LoadedProgram::load_bytes(
        &crate::loader::test_support::empty_zbc(),
        host::LoadOptions {
            allow_profiling: false,
            mode: host::ExecMode::Interp,
            ..Default::default()
        },
    ).unwrap();
    assert!(!host::profiler_loaded(program.context()));
    let error = host::prepare_profiler(program.context()).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("disabled"), "{error}");
    assert!(!host::profiler_loaded(program.context()));
}

#[test]
fn legacy_profiler_interface_is_rejected_before_attachment() {
    use abi::{
        ApiHeader, ComponentDescriptor, ComponentInterface, ComponentStatus, ExecutionIdentity,
        IdSpan, InstanceHandle, InterfaceId, InterfaceRef, InterfaceSpan, InterfaceSpec, ProfilerApi,
        RuntimeApi, VmHandle,
    };
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    const LEGACY_VERSION: u32 = 2;
    static ATTACHED: AtomicBool = AtomicBool::new(false);
    static LEGACY_SPEC: [InterfaceSpec; 1] = [InterfaceSpec {
        version: LEGACY_VERSION,
        ..InterfaceSpec::of::<ProfilerApi>()
    }];
    static LEGACY_API: ProfilerApi = ProfilerApi {
        header: ApiHeader::of::<ProfilerApi>(LEGACY_VERSION),
        core_identity: identity,
        attach,
    };
    unsafe extern "C" fn identity() -> ExecutionIdentity {
        ExecutionIdentity {
            image: crate::components::bridge::core_identity(),
            abi: crate::components::bridge::CORE_ABI,
        }
    }
    unsafe extern "C" fn attach(_: *const c_void, _: u32, _: u32) -> ComponentStatus {
        ATTACHED.store(true, Ordering::Relaxed);
        ComponentStatus::ERROR
    }
    unsafe extern "C" fn create(
        _: *const RuntimeApi, _: VmHandle, out: *mut InstanceHandle,
    ) -> ComponentStatus {
        unsafe { *out = std::ptr::null_mut(); }
        ComponentStatus::OK
    }
    unsafe extern "C" fn destroy(_: InstanceHandle) {}
    unsafe extern "C" fn query(
        _: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef,
    ) -> ComponentStatus {
        if out.is_null() || id != ProfilerApi::ID || version != LEGACY_VERSION {
            return ComponentStatus::UNAVAILABLE;
        }
        unsafe { *out = InterfaceRef { api: &LEGACY_API.header, context: std::ptr::null_mut() }; }
        ComponentStatus::OK
    }
    unsafe extern "C" fn old_profiler() -> *const ComponentDescriptor {
        static DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
            header: ApiHeader::of::<ComponentDescriptor>(abi::COMPONENT_ABI_VERSION),
            build_id: abi::COMPONENT_BUILD_ID,
            id: abi::PROFILER_COMPONENT,
            dependencies: IdSpan { data: std::ptr::null(), len: 0 },
            interfaces: InterfaceSpan { data: LEGACY_SPEC.as_ptr(), len: 1 },
            create_vm: Some(create),
            destroy_vm: Some(destroy),
            query_interface: Some(query),
        };
        &DESCRIPTOR
    }

    let components = unsafe {
        api::ComponentOptions::default().with_static(abi::PROFILER_COMPONENT, old_profiler)
    };
    let program = host::LoadedProgram::load_bytes_with_components(
        &crate::loader::test_support::empty_zbc(),
        host::LoadOptions { mode: host::ExecMode::Interp, ..Default::default() },
        crate::vm::assembly::ComponentInputs { components, ..Default::default() },
    ).unwrap();
    let ctx = program.context();
    let error = host::prepare_profiler(ctx).unwrap_err().to_string();
    assert!(error.contains("does not match requested table"), "{error}");
    assert!(!ATTACHED.load(Ordering::Relaxed));
    assert!(!host::profiler_loaded(ctx));
}
