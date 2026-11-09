//! Same-build execution registration. The VM is borrowed for attach only; all
//! persistent execution state is created inside the shared runtime core.
use crate::{ApiHeader, ComponentId, ComponentInterface, ComponentStatus, InterfaceId};
use core::ffi::c_void;
pub const INTERP_COMPONENT: ComponentId = ComponentId::new(0x7a34322e696e746572702e7631000000);
pub const JIT_COMPONENT: ComponentId = ComponentId::new(0x7a34322e6a69742e7631000000000000);
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ExecutionConfig {
    pub threshold: u32,
    pub osr_threshold: u32,
    pub interp_tierup: u32,
    pub profile: u32,
    pub debug_promote: u32,
}
/// Layout and build-configuration stamp copied from the core metadata used to
/// compile a backend. Image address alone cannot detect a differently configured
/// consumer that happens to resolve the same shared-library file.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionCoreAbi {
    pub flags: u64,
    pub layouts: [usize; 8],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExecutionIdentity {
    pub image: *const c_void,
    pub abi: ExecutionCoreAbi,
}
/// attach must only register the advertised backend, without executing managed
/// code. Host checks core_identity before passing any borrowed VM address.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExecutionApi {
    pub header: ApiHeader,
    pub core_identity: unsafe extern "C" fn() -> ExecutionIdentity,
    pub attach: unsafe extern "C" fn(*const c_void, *const ExecutionConfig) -> ComponentStatus,
}
unsafe impl ComponentInterface for ExecutionApi {
    const ID: InterfaceId = InterfaceId::new(0x7a34322e657865632e76310000000000);
    const VERSION: u32 = 1;
}
