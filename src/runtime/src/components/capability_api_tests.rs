use super::{api, api::ComponentOptions, execution_roles::ExecutionBindings};
use crate::{components::abi::*, context::VmContext};
use std::sync::{Mutex, atomic::{AtomicUsize, Ordering}};

static LOCK: Mutex<()> = Mutex::new(());
static CREATES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
const FIRST: ComponentId = ComponentId::new(0xc001);
const RENAMED: ComponentId = ComponentId::new(0xc002);
const WRONG_VERSION: ComponentId = ComponentId::new(0xc003);

unsafe extern "C" fn identity() -> ExecutionIdentity {
    ExecutionIdentity { image: crate::components::bridge::core_identity(), abi: crate::components::bridge::CORE_ABI }
}
unsafe extern "C" fn attach(_: *const core::ffi::c_void, _: *const ExecutionConfig) -> ComponentStatus {
    ComponentStatus::OK
}
static EXECUTION: ExecutionApi = ExecutionApi {
    header: ApiHeader::of::<ExecutionApi>(1), core_identity: identity, attach,
};
unsafe extern "C" fn create(_: *const RuntimeApi, _: VmHandle, out: *mut InstanceHandle) -> ComponentStatus {
    CREATES.fetch_add(1, Ordering::SeqCst);
    LIVE.fetch_add(1, Ordering::SeqCst);
    unsafe { *out = Box::into_raw(Box::new(0u8)).cast(); }
    ComponentStatus::OK
}
unsafe extern "C" fn destroy(state: InstanceHandle) {
    unsafe { drop(Box::from_raw(state.cast::<u8>())); }
    LIVE.fetch_sub(1, Ordering::SeqCst);
}
unsafe extern "C" fn query(state: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef) -> ComponentStatus {
    unsafe { *out = InterfaceRef::NONE; }
    if id != ExecutionApi::ID || version != ExecutionApi::VERSION { return ComponentStatus::UNAVAILABLE; }
    unsafe { *out = InterfaceRef { api: &EXECUTION.header, context: state }; }
    ComponentStatus::OK
}
static SPEC: [InterfaceSpec; 1] = [InterfaceSpec::of::<ExecutionApi>()];
static BAD_SPEC: [InterfaceSpec; 1] = [InterfaceSpec { version: 2, ..InterfaceSpec::of::<ExecutionApi>() }];
const fn descriptor(id: ComponentId, specs: &'static [InterfaceSpec]) -> ComponentDescriptor {
    ComponentDescriptor {
        header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION), build_id: COMPONENT_BUILD_ID,
        id, dependencies: IdSpan::EMPTY, interfaces: InterfaceSpan { data: specs.as_ptr(), len: specs.len() },
        create_vm: Some(create), destroy_vm: Some(destroy), query_interface: Some(query),
    }
}
static ORIGINAL: ComponentDescriptor = descriptor(FIRST, &SPEC);
static REPLACEMENT: ComponentDescriptor = descriptor(RENAMED, &SPEC);
static WRONG: ComponentDescriptor = descriptor(WRONG_VERSION, &BAD_SPEC);
unsafe extern "C" fn first() -> *const ComponentDescriptor { &ORIGINAL }
unsafe extern "C" fn renamed() -> *const ComponentDescriptor { &REPLACEMENT }
unsafe extern "C" fn wrong() -> *const ComponentDescriptor { &WRONG }

fn consume(ctx: &VmContext) -> ExecutionIdentity {
    let execution = api::execution(ctx, api::ExecutionRole::Baseline).unwrap().unwrap();
    unsafe { (execution.core_identity)() }
}

#[test]
fn changing_provider_identity_does_not_change_the_role_consumer() {
    let _guard = LOCK.lock().unwrap();
    CREATES.store(0, Ordering::SeqCst);
    for (id, init) in [(FIRST, first as ComponentInit), (RENAMED, renamed as ComponentInit)] {
        let ctx = VmContext::new();
        assert!(api::execution(&ctx, api::ExecutionRole::Baseline).unwrap().is_none());
        let options = unsafe { ComponentOptions::default().with_static(id, init) };
        let bindings = ExecutionBindings::new([(api::ExecutionRole::Baseline, id)]).unwrap();
        options.install_bound(&ctx, bindings).unwrap();
        let observed = consume(&ctx);
        assert_eq!(observed.image, crate::components::bridge::core_identity());
        assert_eq!(observed.abi, crate::components::bridge::CORE_ABI);
        assert!(api::execution(&ctx, api::ExecutionRole::Optimizing).unwrap().is_none());
    }
    assert_eq!(CREATES.load(Ordering::SeqCst), 2);
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
}

#[test]
fn invalid_or_ambiguous_bindings_fail_before_instances_are_created() {
    let _guard = LOCK.lock().unwrap();
    CREATES.store(0, Ordering::SeqCst);
    assert!(ExecutionBindings::new([
        (api::ExecutionRole::Baseline, FIRST), (api::ExecutionRole::Baseline, RENAMED),
    ]).is_err());
    for (id, init, requested) in [
        (FIRST, first as ComponentInit, RENAMED),
        (WRONG_VERSION, wrong as ComponentInit, WRONG_VERSION),
    ] {
        let ctx = VmContext::new();
        let options = unsafe { ComponentOptions::default().with_static(id, init) };
        let bindings = ExecutionBindings::new([(api::ExecutionRole::Baseline, requested)]).unwrap();
        assert!(options.install_bound(&ctx, bindings).is_err());
        assert!(ctx.core().components().get().is_none());
        assert!(api::execution(&ctx, api::ExecutionRole::Baseline).unwrap().is_none());
    }
    for specs in [
        vec![],
        vec![InterfaceSpec { size: SPEC[0].size + 8, ..SPEC[0] }],
        vec![InterfaceSpec { alignment: SPEC[0].alignment * 2, ..SPEC[0] }],
    ] {
        let factory = unsafe { super::catalog::Factory::from_validated_descriptor(&ORIGINAL, vec![], specs) }.unwrap();
        let bindings = ExecutionBindings::new([(api::ExecutionRole::Baseline, FIRST)]).unwrap();
        assert!(api::VmComponents::create_bound(&[std::sync::Arc::new(factory)], 1, bindings).is_err());
    }
    assert_eq!(CREATES.load(Ordering::SeqCst), 0);
}

#[test]
fn role_queries_are_read_only_and_worker_leases_preserve_lifetime() {
    let _guard = LOCK.lock().unwrap();
    CREATES.store(0, Ordering::SeqCst);
    let ctx = VmContext::new();
    let options = unsafe { ComponentOptions::default().with_static(RENAMED, renamed) };
    for _ in 0..4 { assert!(api::execution(&ctx, api::ExecutionRole::Baseline).unwrap().is_none()); }
    assert_eq!(CREATES.load(Ordering::SeqCst), 0);
    options.install_bound(&ctx, ExecutionBindings::new([(api::ExecutionRole::Baseline, RENAMED)]).unwrap()).unwrap();
    let weak = std::sync::Arc::downgrade(ctx.core());
    let worker = VmContext::new_with_core(ctx.core_arc());
    let lease = api::execution(&worker, api::ExecutionRole::Baseline).unwrap().unwrap();
    assert_eq!(consume(&ctx).image, consume(&worker).image);
    assert_eq!(CREATES.load(Ordering::SeqCst), 1);
    drop(ctx); drop(worker);
    assert!(weak.upgrade().is_some());
    assert_eq!(LIVE.load(Ordering::SeqCst), 1);
    assert_eq!(unsafe { (lease.core_identity)() }.image, crate::components::bridge::core_identity());
    drop(lease);
    assert!(weak.upgrade().is_none());
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
}

#[test]
fn successful_attach_without_registering_the_requested_engine_is_rejected() {
    let _guard = LOCK.lock().unwrap();
    let ctx = crate::host::vm_defaults::construct(None, crate::pal::fs::default_vfs(), Default::default(),
        crate::exec::engine::EngineRegistry::new(None));
    let options = unsafe { ComponentOptions::default().with_static(RENAMED, renamed) };
    options.install_bound(&ctx, ExecutionBindings::new([(api::ExecutionRole::Baseline, RENAMED)]).unwrap()).unwrap();
    let error = super::api::attach_execution(&ctx, crate::zpkg::ExecMode::Interp).unwrap_err().to_string();
    assert!(error.contains("did not register"), "{error}");
    assert!(!ctx.core().exec().engines.has_baseline());
}

#[cfg(feature = "component-loading")]
#[test]
fn checking_a_real_library_candidate_does_not_open_it() {
    let fixture = super::component_tests::fixture();
    let path = fixture.with_file_name(crate::pal::dynlib::file_name(&format!("z42_query_probe_{}", std::process::id())));
    std::fs::copy(fixture, &path).unwrap();
    assert!(!crate::pal::dynlib::component_is_loaded(&path));
    assert!(super::backends::known_source(&path));
    assert!(!crate::pal::dynlib::component_is_loaded(&path), "a presence query loaded native code");
    let _image = unsafe { crate::pal::dynlib::Library::open_component(&path) }.unwrap();
    assert!(crate::pal::dynlib::component_is_loaded(&path), "explicit preparation loads the same valid image");
    // Windows keeps mapped DLL files locked; the build directory owns this copy
    // until the test process exits and releases its process-lifetime image cache.
}
