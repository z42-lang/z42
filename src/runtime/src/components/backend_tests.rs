use super::*;
use crate::component_abi::*;
use crate::host::{LoadedProgram, LoadOptions, ExecMode, VmContext};
use std::sync::atomic::{AtomicUsize, Ordering};
fn load(mode: ExecMode, backends: BackendOptions) -> anyhow::Result<LoadedProgram> {
    LoadedProgram::load_bytes(&crate::host::program_tests::empty_zbc(), LoadOptions {
        mode, backends, ..Default::default()
    })
}
fn failure(mode: ExecMode, options: BackendOptions) -> String {
    format!("{:#}", load(mode, options).err().expect("backend selection must fail"))
}
#[test]
fn interpreter_mode_installs_only_the_baseline_and_workers_share_it() {
    let program = load(ExecMode::Interp, BackendOptions::default()).unwrap();
    assert_eq!(component_ids(program.context()), [INTERP_COMPONENT]);
    assert!(program.context().tier().is_none());
    let worker = VmContext::new_with_core(program.context().core_arc());
    assert_eq!(component_ids(&worker), [INTERP_COMPONENT]);
    assert!(worker.tier().is_none());
}
#[cfg(all(feature = "jit", z42_jit_supported))]
#[test]
fn jit_mode_registers_both_engines_without_eager_compilation() {
    let program = load(ExecMode::Jit, BackendOptions::default()).unwrap();
    assert_eq!(component_ids(program.context()), [INTERP_COMPONENT, JIT_COMPONENT]);
    assert!(program.context().tier().is_none());
    assert_eq!(program.context().counters_snapshot().jit_methods_compiled, 0);
}
#[test]
fn explicit_disable_and_missing_dynamic_library_fail() {
    let off = BackendOptions { interp: BackendSource::Disabled, ..Default::default() };
    assert!(failure(ExecMode::Interp, off).contains("interpreter backend is disabled"));
    let missing = std::env::temp_dir().join("z42-backend-missing-test").join("never-created.so");
    let options = BackendOptions { interp: BackendSource::Dynamic(missing), ..Default::default() };
    assert!(failure(ExecMode::Interp, options).contains("was not found"));
    if super::backends::jit_supported() {
        let off = BackendOptions { jit: BackendSource::Disabled, ..Default::default() };
        assert!(failure(ExecMode::Jit, off).contains("JIT backend is disabled"));
    }
}
#[cfg(not(all(feature = "jit", z42_jit_supported)))]
#[test]
fn absent_static_jit_is_an_error_without_fallback() {
    let options = BackendOptions { jit: BackendSource::Linked, ..Default::default() };
    let error = failure(ExecMode::Jit, options);
    assert!(error.contains(if super::backends::jit_supported() { "not statically linked" } else { "unsupported on this target" }), "{error}");
}
static FOREIGN: u8 = 0;
static ATTACH_CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn identity() -> ExecutionIdentity {
    ExecutionIdentity { image: core::ptr::addr_of!(FOREIGN).cast(), abi: crate::component_bridge::CORE_ABI }
}
unsafe extern "C" fn attach_foreign(_ctx: *const core::ffi::c_void, _config: *const ExecutionConfig) -> ComponentStatus {
    ATTACH_CALLS.fetch_add(1, Ordering::Relaxed); ComponentStatus::OK
}
static FOREIGN_API: ExecutionApi = ExecutionApi { header: ApiHeader::of::<ExecutionApi>(1), core_identity: identity, attach: attach_foreign };
unsafe extern "C" fn create(_api: *const RuntimeApi, _vm: VmHandle, out: *mut InstanceHandle) -> ComponentStatus {
    unsafe { *out = core::ptr::null_mut(); } ComponentStatus::OK
}
unsafe extern "C" fn destroy(_state: InstanceHandle) {}
unsafe extern "C" fn query(_state: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef) -> ComponentStatus {
    unsafe { *out = InterfaceRef::NONE; }
    if id != ExecutionApi::ID || version != ExecutionApi::VERSION { return ComponentStatus::UNAVAILABLE; }
    unsafe { *out = InterfaceRef { api: &FOREIGN_API.header, context: core::ptr::null_mut() }; }
    ComponentStatus::OK
}
static SPECS: [InterfaceSpec; 1] = [InterfaceSpec::of::<ExecutionApi>()];
static FOREIGN_DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
    header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION), build_id: COMPONENT_BUILD_ID,
    id: INTERP_COMPONENT, dependencies: IdSpan::EMPTY,
    interfaces: InterfaceSpan { data: SPECS.as_ptr(), len: 1 },
    create_vm: Some(create), destroy_vm: Some(destroy), query_interface: Some(query),
};
unsafe extern "C" fn foreign_init() -> *const ComponentDescriptor { &FOREIGN_DESCRIPTOR }
#[test]
fn another_core_is_rejected_before_any_vm_address_is_passed_to_it() {
    ATTACH_CALLS.store(0, Ordering::Relaxed);
    let ctx = crate::host::vm_defaults::construct(None, crate::pal::fs::default_vfs(), Default::default(),
        crate::exec::engine::EngineRegistry::new(None));
    let components = unsafe { ComponentOptions::default().with_static(INTERP_COMPONENT, foreign_init) };
    components.install_bound(&ctx, super::execution_roles::ExecutionBindings::new([(super::api::ExecutionRole::Baseline, INTERP_COMPONENT)]).unwrap()).unwrap();
    let error = super::backends::attach(&ctx, ExecMode::Interp).unwrap_err();
    assert!(error.to_string().contains("core image or ABI mismatch"));
    assert_eq!(ATTACH_CALLS.load(Ordering::Relaxed), 0);
}

unsafe extern "C" fn wrong_abi_identity() -> ExecutionIdentity {
    let mut abi = crate::component_bridge::CORE_ABI;
    abi.flags ^= 1;
    ExecutionIdentity { image: crate::component_bridge::core_identity(), abi }
}
static WRONG_ABI_API: ExecutionApi = ExecutionApi { header: ApiHeader::of::<ExecutionApi>(1),
    core_identity: wrong_abi_identity, attach: attach_foreign };
unsafe extern "C" fn query_wrong_abi(_state: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef) -> ComponentStatus {
    unsafe { *out = InterfaceRef::NONE; }
    if id != ExecutionApi::ID || version != ExecutionApi::VERSION { return ComponentStatus::UNAVAILABLE; }
    unsafe { *out = InterfaceRef { api: &WRONG_ABI_API.header, context: core::ptr::null_mut() }; }
    ComponentStatus::OK
}
static WRONG_ABI_DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
    query_interface: Some(query_wrong_abi), ..FOREIGN_DESCRIPTOR
};
unsafe extern "C" fn wrong_abi_init() -> *const ComponentDescriptor { &WRONG_ABI_DESCRIPTOR }
#[test]
fn same_image_with_incompatible_compiled_layout_is_rejected() {
    let ctx = crate::host::vm_defaults::construct(None, crate::pal::fs::default_vfs(), Default::default(),
        crate::exec::engine::EngineRegistry::new(None));
    let components = unsafe { ComponentOptions::default().with_static(INTERP_COMPONENT, wrong_abi_init) };
    components.install_bound(&ctx, super::execution_roles::ExecutionBindings::new([(super::api::ExecutionRole::Baseline, INTERP_COMPONENT)]).unwrap()).unwrap();
    assert!(super::backends::attach(&ctx, ExecMode::Interp).unwrap_err().to_string().contains("core image or ABI mismatch"));
    assert_eq!(ATTACH_CALLS.load(Ordering::Relaxed), 0);
}
