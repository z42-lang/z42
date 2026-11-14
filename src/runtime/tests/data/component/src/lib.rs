//! A protocol-only component: it owns its state and never accesses VM implementation.
use std::{
    ffi::c_void,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};
use z42::component_abi::*;

pub const A: ComponentId = ComponentId::new(0x100);
pub const B: ComponentId = ComponentId::new(0x200);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CounterApi {
    pub header: ApiHeader,
    pub read: unsafe extern "C" fn(InstanceHandle, *mut u64, *mut u64) -> ComponentStatus,
    pub increment: unsafe extern "C" fn(InstanceHandle) -> ComponentStatus,
}
unsafe impl ComponentInterface for CounterApi {
    const ID: InterfaceId = InterfaceId::new(0x1000);
    const VERSION: u32 = 1;
}
static LIVE: AtomicUsize = AtomicUsize::new(0);
static CREATED: AtomicU64 = AtomicU64::new(0);
static DESTROYED: AtomicU64 = AtomicU64::new(0);
static INITS: AtomicUsize = AtomicUsize::new(0);
fn record(log: &AtomicU64, id: u64) {
    let _ = log.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |x| {
        Some(x.wrapping_mul(16).wrapping_add(id))
    });
}
#[no_mangle]
pub extern "C" fn probe_live() -> usize {
    LIVE.load(Ordering::SeqCst)
}
#[no_mangle]
pub extern "C" fn probe_created() -> u64 {
    CREATED.load(Ordering::SeqCst)
}
#[no_mangle]
pub extern "C" fn probe_destroyed() -> u64 {
    DESTROYED.load(Ordering::SeqCst)
}
#[no_mangle]
pub extern "C" fn probe_inits() -> usize {
    INITS.load(Ordering::SeqCst)
}
#[no_mangle]
pub extern "C" fn probe_reset() {
    CREATED.store(0, Ordering::SeqCst);
    DESTROYED.store(0, Ordering::SeqCst);
    INITS.store(0, Ordering::SeqCst);
}
struct State {
    id: u64,
    value: AtomicU64,
    vm: VmHandle,
    info: *const VmInfoApi,
}
unsafe extern "C" fn read(
    state: InstanceHandle,
    value: *mut u64,
    vm_id: *mut u64,
) -> ComponentStatus {
    if state.is_null() || value.is_null() || vm_id.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    let state = unsafe { &*state.cast::<State>() };
    unsafe {
        *value = state.value.load(Ordering::SeqCst);
        ((*state.info).vm_id)(state.vm, vm_id)
    }
}
unsafe extern "C" fn increment(state: InstanceHandle) -> ComponentStatus {
    if state.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        (&*state.cast::<State>())
            .value
            .fetch_add(1, Ordering::SeqCst);
    }
    ComponentStatus::OK
}
static COUNTER: CounterApi = CounterApi {
    header: ApiHeader::of::<CounterApi>(1),
    read,
    increment,
};
static WRONG_COUNTER: CounterApi = CounterApi {
    header: ApiHeader::of::<CounterApi>(99),
    read,
    increment,
};
unsafe fn create(
    id: u64,
    runtime: *const RuntimeApi,
    vm: VmHandle,
    out: *mut InstanceHandle,
) -> ComponentStatus {
    if runtime.is_null() || vm.is_null() || out.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        *out = std::ptr::null_mut();
    }
    let header = unsafe { &(*runtime).header };
    if header.version != COMPONENT_ABI_VERSION
        || header.size as usize != std::mem::size_of::<RuntimeApi>()
    {
        return ComponentStatus::ERROR;
    }
    let mut info = InterfaceRef::NONE;
    let status = unsafe { ((*runtime).get_service)(vm, VmInfoApi::ID, 1, &mut info) };
    if status != ComponentStatus::OK || info.api.is_null() {
        return ComponentStatus::ERROR;
    }
    if id == 2 {
        let mut parent = InterfaceRef::NONE;
        let status = unsafe { ((*runtime).query_component)(vm, A, CounterApi::ID, 1, &mut parent) };
        if status != ComponentStatus::OK || parent.api.is_null() {
            return ComponentStatus::ERROR;
        }
        let table = unsafe { &*parent.api.cast::<CounterApi>() };
        if unsafe { (table.increment)(parent.context) } != ComponentStatus::OK {
            return ComponentStatus::ERROR;
        }
    }
    let state = Box::new(State {
        id,
        value: AtomicU64::new(0),
        vm: info.context,
        info: info.api.cast(),
    });
    unsafe {
        *out = Box::into_raw(state).cast::<c_void>();
    }
    LIVE.fetch_add(1, Ordering::SeqCst);
    record(&CREATED, id);
    ComponentStatus::OK
}
unsafe extern "C" fn create_a(
    r: *const RuntimeApi,
    vm: VmHandle,
    out: *mut InstanceHandle,
) -> ComponentStatus {
    unsafe { create(1, r, vm, out) }
}
unsafe extern "C" fn create_b(
    r: *const RuntimeApi,
    vm: VmHandle,
    out: *mut InstanceHandle,
) -> ComponentStatus {
    unsafe { create(2, r, vm, out) }
}
unsafe extern "C" fn create_failed(
    r: *const RuntimeApi,
    vm: VmHandle,
    out: *mut InstanceHandle,
) -> ComponentStatus {
    let status = unsafe { create(2, r, vm, out) };
    if status == ComponentStatus::OK {
        ComponentStatus::ERROR
    } else {
        status
    }
}
unsafe extern "C" fn unknown_status(
    _: *const RuntimeApi,
    _: VmHandle,
    out: *mut InstanceHandle,
) -> ComponentStatus {
    if !out.is_null() {
        unsafe {
            *out = std::ptr::null_mut();
        }
    }
    ComponentStatus(u32::MAX)
}
unsafe extern "C" fn destroy(state: InstanceHandle) {
    if state.is_null() {
        return;
    }
    let state = unsafe { Box::from_raw(state.cast::<State>()) };
    record(&DESTROYED, state.id);
    LIVE.fetch_sub(1, Ordering::SeqCst);
}
unsafe extern "C" fn query(
    state: InstanceHandle,
    id: InterfaceId,
    version: u32,
    out: *mut InterfaceRef,
) -> ComponentStatus {
    if out.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        *out = InterfaceRef::NONE;
    }
    if id != CounterApi::ID || version != 1 {
        return ComponentStatus::UNAVAILABLE;
    }
    unsafe {
        *out = InterfaceRef {
            api: &COUNTER.header,
            context: state,
        };
    }
    ComponentStatus::OK
}
unsafe extern "C" fn wrong_query(
    state: InstanceHandle,
    _: InterfaceId,
    _: u32,
    out: *mut InterfaceRef,
) -> ComponentStatus {
    if out.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        *out = InterfaceRef {
            api: &WRONG_COUNTER.header,
            context: state,
        };
    }
    ComponentStatus::OK
}
static DEPS_A: [ComponentId; 1] = [A];
static DEPS_B: [ComponentId; 1] = [B];
static INTERFACES: [InterfaceSpec; 1] = [InterfaceSpec::of::<CounterApi>()];
const fn descriptor(
    id: ComponentId,
    dependencies: IdSpan,
    create: CreateVm,
) -> ComponentDescriptor {
    ComponentDescriptor {
        header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION),
        build_id: COMPONENT_BUILD_ID,
        id,
        dependencies,
        interfaces: InterfaceSpan {
            data: INTERFACES.as_ptr(),
            len: 1,
        },
        create_vm: Some(create),
        destroy_vm: Some(destroy),
        query_interface: Some(query),
    }
}
static DESC_A: ComponentDescriptor = descriptor(A, IdSpan::EMPTY, create_a);
static DESC_B: ComponentDescriptor = descriptor(
    B,
    IdSpan {
        data: DEPS_A.as_ptr(),
        len: 1,
    },
    create_b,
);
static UNDECLARED: ComponentDescriptor = descriptor(B, IdSpan::EMPTY, create_b);
static FAIL: ComponentDescriptor = descriptor(
    B,
    IdSpan {
        data: DEPS_A.as_ptr(),
        len: 1,
    },
    create_failed,
);
static WRONG_INTERFACE: ComponentDescriptor = ComponentDescriptor {
    query_interface: Some(wrong_query),
    ..descriptor(
        B,
        IdSpan {
            data: DEPS_A.as_ptr(),
            len: 1,
        },
        create_b,
    )
};
static WRONG_VERSION: ComponentDescriptor = ComponentDescriptor {
    header: ApiHeader::of::<ComponentDescriptor>(99),
    ..descriptor(A, IdSpan::EMPTY, create_a)
};
static WRONG_BUILD: ComponentDescriptor = ComponentDescriptor {
    build_id: [0; 32],
    ..descriptor(A, IdSpan::EMPTY, create_a)
};
static UNKNOWN_STATUS: ComponentDescriptor = descriptor(A, IdSpan::EMPTY, unknown_status);
static NO_CALLBACK: ComponentDescriptor = ComponentDescriptor {
    create_vm: None,
    ..descriptor(A, IdSpan::EMPTY, create_a)
};
static CYCLE_A: ComponentDescriptor = descriptor(
    A,
    IdSpan {
        data: DEPS_B.as_ptr(),
        len: 1,
    },
    create_a,
);
static NULL_SPAN: ComponentDescriptor = descriptor(
    A,
    IdSpan {
        data: std::ptr::null(),
        len: 1,
    },
    create_a,
);
macro_rules! entry {
    ($name:ident, $descriptor:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name() -> *const ComponentDescriptor {
            INITS.fetch_add(1, Ordering::SeqCst);
            &$descriptor
        }
    };
}
entry!(z42_component_probe_a_init, DESC_A);
entry!(z42_component_probe_b_init, DESC_B);
entry!(z42_component_probe_undeclared_init, UNDECLARED);
entry!(z42_component_probe_failed_init, FAIL);
entry!(z42_component_probe_wrong_interface_init, WRONG_INTERFACE);
entry!(z42_component_probe_wrong_version_init, WRONG_VERSION);
entry!(z42_component_probe_wrong_build_init, WRONG_BUILD);
entry!(z42_component_probe_unknown_status_init, UNKNOWN_STATUS);
entry!(z42_component_probe_no_callback_init, NO_CALLBACK);
entry!(z42_component_probe_cycle_a_init, CYCLE_A);
entry!(z42_component_probe_null_span_init, NULL_SPAN);
#[no_mangle]
pub unsafe extern "C" fn z42_component_probe_null_init() -> *const ComponentDescriptor {
    std::ptr::null()
}
