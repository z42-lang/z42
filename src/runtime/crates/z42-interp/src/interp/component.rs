//! Static and dynamic registration of the same execution backend.
use z42_component_api::*;
use core::ffi::c_void;
unsafe extern "C" fn identity() -> ExecutionIdentity {
    #[cfg(feature = "shared-core")]
    { let _ = z42_runtime_shared::link_anchor(); }
    ExecutionIdentity { image: crate::core_identity(), abi: crate::CORE_ABI }
}
unsafe extern "C" fn attach(vm: *const c_void, config: *const ExecutionConfig) -> ComponentStatus {
    if vm.is_null() || config.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    let ctx = unsafe { &*vm.cast::<crate::vm::context::VmContext>() };
    let config = unsafe { &*config };
    let _ = config;
    match ctx.core().exec().engines.register_baseline(&super::engine::ENGINE) { Ok(()) => ComponentStatus::OK, Err(_) => ComponentStatus::ERROR }
}
static API: ExecutionApi = ExecutionApi { header: ApiHeader::of::<ExecutionApi>(1), core_identity: identity, attach };
unsafe extern "C" fn create(runtime: *const RuntimeApi, _vm: VmHandle, out: *mut InstanceHandle) -> ComponentStatus {
    if runtime.is_null() || out.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    unsafe { *out = core::ptr::null_mut(); }
    ComponentStatus::OK
}
unsafe extern "C" fn destroy(_state: InstanceHandle) {}
unsafe extern "C" fn query(_state: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef) -> ComponentStatus {
    if out.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    unsafe { *out = InterfaceRef::NONE; }
    if id != ExecutionApi::ID || version != ExecutionApi::VERSION { return ComponentStatus::UNAVAILABLE; }
    unsafe { *out = InterfaceRef { api: &API.header, context: core::ptr::null_mut() }; }
    ComponentStatus::OK
}
static INTERFACES: [InterfaceSpec; 1] = [InterfaceSpec::of::<ExecutionApi>()];
static DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
    header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION), build_id: COMPONENT_BUILD_ID,
    id: INTERP_COMPONENT, dependencies: IdSpan { data: core::ptr::null(), len: 0 },
    interfaces: InterfaceSpan { data: INTERFACES.as_ptr(), len: 1 },
    create_vm: Some(create), destroy_vm: Some(destroy), query_interface: Some(query),
};
#[cfg_attr(feature = "shared-core", no_mangle)]
pub unsafe extern "C" fn z42_interp_init() -> *const ComponentDescriptor { &DESCRIPTOR }
