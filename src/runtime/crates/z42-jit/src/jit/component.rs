//! Static and dynamic registration of the same execution backend.
use z42::components::abi::*;
use z42::components::bridge::{core_identity, CORE_ABI};
use core::ffi::c_void;
unsafe extern "C" fn identity() -> ExecutionIdentity {
    ExecutionIdentity { image: core_identity(), abi: CORE_ABI }
}
unsafe extern "C" fn attach(vm: *const c_void, config: *const ExecutionConfig) -> ComponentStatus {
    if vm.is_null() || config.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    let ctx = unsafe { &*vm.cast::<crate::context::VmContext>() };
    let config = unsafe { &*config };
    let _ = config;
    match ctx.core().exec().engines.register_jit(&super::engine::ENGINE, crate::exec::engine::JitConfig {
        threshold: config.threshold, osr_threshold: config.osr_threshold, interp_tierup: config.interp_tierup,
        profile: config.profile != 0, debug_promote: config.debug_promote != 0,
    }) { Ok(()) => ComponentStatus::OK, Err(_) => ComponentStatus::ERROR }
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
static DEPENDENCIES: [ComponentId; 1] = [INTERP_COMPONENT];
static DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
    header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION), build_id: COMPONENT_BUILD_ID,
    id: JIT_COMPONENT, dependencies: IdSpan { data: DEPENDENCIES.as_ptr(), len: DEPENDENCIES.len() },
    interfaces: InterfaceSpan { data: INTERFACES.as_ptr(), len: 1 },
    create_vm: Some(create), destroy_vm: Some(destroy), query_interface: Some(query),
};
#[no_mangle]
pub unsafe extern "C" fn z42_jit_init() -> *const ComponentDescriptor { &DESCRIPTOR }
