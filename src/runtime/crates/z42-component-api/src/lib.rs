//! Internal, same-build component protocol. No VM implementation or owned Rust values
//! cross this ABI. Descriptors/tables are immutable until their library is released.
#![no_std]
use core::ffi::c_void;

pub const COMPONENT_ABI_VERSION: u32 = 1;
include!(concat!(env!("OUT_DIR"), "/identity.rs"));

macro_rules! id_type {
    ($name:ident) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub [u8; 16]);
        impl $name {
            pub const fn new(value: u128) -> Self {
                Self(value.to_be_bytes())
            }
        }
        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                for byte in self.0 {
                    write!(f, "{byte:02x}")?;
                }
                Ok(())
            }
        }
    };
}
id_type!(ComponentId);
id_type!(InterfaceId);

/// Integer newtype accepts unknown native status values without enum-discriminant UB.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentStatus(pub u32);
impl ComponentStatus {
    pub const OK: Self = Self(0);
    pub const UNAVAILABLE: Self = Self(1);
    pub const ERROR: Self = Self(2);
    pub const INVALID_ARGUMENT: Self = Self(3);
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ApiHeader {
    pub version: u32,
    pub size: u32,
}
impl ApiHeader {
    pub const fn of<T>(version: u32) -> Self {
        Self {
            version,
            size: core::mem::size_of::<T>() as u32,
        }
    }
}

/// # Safety
/// Implementors must be immutable, C-layout, header-first function tables with no
/// owned Rust data. ID/version identify the exact table; callbacks obey their ABI
/// and synchronization contracts. Returned tables remain valid until destroy_vm.
pub unsafe trait ComponentInterface: Copy + Send + Sync + 'static {
    const ID: InterfaceId;
    const VERSION: u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InterfaceSpec {
    pub id: InterfaceId,
    pub version: u32,
    pub size: u32,
    pub alignment: u32,
}
impl InterfaceSpec {
    pub const fn of<T: ComponentInterface>() -> Self {
        Self {
            id: T::ID,
            version: T::VERSION,
            size: core::mem::size_of::<T>() as u32,
            alignment: core::mem::align_of::<T>() as u32,
        }
    }
}

pub type VmHandle = *mut c_void;
pub type InstanceHandle = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterfaceRef {
    pub api: *const ApiHeader,
    pub context: InstanceHandle,
}
impl InterfaceRef {
    pub const NONE: Self = Self {
        api: core::ptr::null(),
        context: core::ptr::null_mut(),
    };
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct IdSpan {
    pub data: *const ComponentId,
    pub len: usize,
}
impl IdSpan {
    pub const EMPTY: Self = Self {
        data: core::ptr::null(),
        len: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterfaceSpan {
    pub data: *const InterfaceSpec,
    pub len: usize,
}

/// Services live until every instance receiving this table has been destroyed.
/// query_component grants access only to declared, already-initialized dependencies.
#[repr(C)]
pub struct RuntimeApi {
    pub header: ApiHeader,
    pub get_service:
        unsafe extern "C" fn(VmHandle, InterfaceId, u32, *mut InterfaceRef) -> ComponentStatus,
    pub query_component: unsafe extern "C" fn(
        VmHandle,
        ComponentId,
        InterfaceId,
        u32,
        *mut InterfaceRef,
    ) -> ComponentStatus,
}

/// Core-owned, immutable identity service. The supplied handle is borrowed, not owned.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VmInfoApi {
    pub header: ApiHeader,
    pub vm_id: unsafe extern "C" fn(VmHandle, *mut u64) -> ComponentStatus,
}
unsafe impl ComponentInterface for VmInfoApi {
    const ID: InterfaceId = InterfaceId::new(1);
    const VERSION: u32 = 1;
}

pub type ComponentInit = unsafe extern "C" fn() -> *const ComponentDescriptor;
pub type CreateVm =
    unsafe extern "C" fn(*const RuntimeApi, VmHandle, *mut InstanceHandle) -> ComponentStatus;
pub type DestroyVm = unsafe extern "C" fn(InstanceHandle);
pub type QueryInterface =
    unsafe extern "C" fn(InstanceHandle, InterfaceId, u32, *mut InterfaceRef) -> ComponentStatus;

/// Same-build immutable factory. All pointer spans must be valid for the library's
/// lifetime. Callbacks/state must support concurrent interface leases. No unwinding
/// may cross the ABI. create_vm's nonnull output must always be destroyable, including
/// on failure; a successful stateless instance may use a null handle.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ComponentDescriptor {
    pub header: ApiHeader,
    pub build_id: [u8; 32],
    pub id: ComponentId,
    pub dependencies: IdSpan,
    pub interfaces: InterfaceSpan,
    pub create_vm: Option<CreateVm>,
    pub destroy_vm: Option<DestroyVm>,
    pub query_interface: Option<QueryInterface>,
}
// Descriptors are immutable; registering their factories is unsafe and requires all
// spans/callbacks to satisfy the contract above. They retain no mutable VM state.
unsafe impl Sync for ComponentDescriptor {}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
