//! Static and dynamic registration of the same profiling backend.
use z42::components::abi::*;
use z42::components::bridge::{core_identity, CORE_ABI};
use core::ffi::c_void;
unsafe extern "C" fn identity() -> ExecutionIdentity {
    ExecutionIdentity { image: core_identity(), abi: CORE_ABI }
}
unsafe extern "C" fn attach(vm: *const c_void, hz: u32, trace: u32) -> ComponentStatus {
    if vm.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    #[cfg(target_arch = "wasm32")]
    if hz != 0 { return ComponentStatus::UNAVAILABLE; }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let ctx = unsafe { &*vm.cast::<z42::context::VmContext>() };
        let sampler = if hz == 0 { super::cpu::Sampler::disabled() }
            else { super::cpu::Sampler::start(hz, trace != 0) };
        ctx.core().diagnostics().sampler.install(Box::new(sampler))
    }));
    match result { Ok(Ok(())) => ComponentStatus::OK, _ => ComponentStatus::ERROR }
}
static API: ProfilerApi = ProfilerApi { header: ApiHeader::of::<ProfilerApi>(1), core_identity: identity, attach };
unsafe extern "C" fn create(runtime: *const RuntimeApi, _vm: VmHandle, out: *mut InstanceHandle) -> ComponentStatus {
    if runtime.is_null() || out.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    unsafe { *out = core::ptr::null_mut(); }
    ComponentStatus::OK
}
unsafe extern "C" fn destroy(_state: InstanceHandle) {}
unsafe extern "C" fn query(_state: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef) -> ComponentStatus {
    if out.is_null() { return ComponentStatus::INVALID_ARGUMENT; }
    unsafe { *out = InterfaceRef::NONE; }
    if id != ProfilerApi::ID || version != ProfilerApi::VERSION { return ComponentStatus::UNAVAILABLE; }
    unsafe { *out = InterfaceRef { api: &API.header, context: core::ptr::null_mut() }; }
    ComponentStatus::OK
}
static INTERFACES: [InterfaceSpec; 1] = [InterfaceSpec::of::<ProfilerApi>()];
static DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
    header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION), build_id: COMPONENT_BUILD_ID,
    id: PROFILER_COMPONENT, dependencies: IdSpan { data: core::ptr::null(), len: 0 },
    interfaces: InterfaceSpan { data: INTERFACES.as_ptr(), len: 1 },
    create_vm: Some(create), destroy_vm: Some(destroy), query_interface: Some(query),
};
#[no_mangle]
pub unsafe extern "C" fn z42_profiler_init() -> *const ComponentDescriptor { &DESCRIPTOR }
